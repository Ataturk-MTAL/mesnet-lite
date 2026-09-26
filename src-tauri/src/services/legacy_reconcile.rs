//! Tek seferlik aktarım: eski `company_term_hours`/`assignments` panolarının
//! LIVE değerlerini (bu iş öncesi TEK yazılan kaynak) tarihçe projeksiyonlarına
//! taşır. Canlı veritabanında teşhis edilen fark: eski tabloda 149 saat / açık
//! projeksiyonda 4 saat, eski tabloda 27 atama / `coordination_periods` boş.
//!
//! **YALNIZ BİR KEZ çalışır** (`settings.legacy_board_reconciled = '1'`
//! bayrağı). Karşılaştırmaya dayalı idempotency YETMEZ: PR sonrası eski
//! tablolar donar; kullanıcı yeni yoldan bir düzenleme yapınca fark yeniden
//! doğar ve bayraksız bir ikinci çalıştırma bu düzenlemeyi GERİ ALIRDI. Bayrak
//! bu yüzden, bir şey yazılmasa bile (fark yoksa da) HER tamamlanmış
//! çalıştırmada yazılır.
//!
//! Yürürlük tarihi HER ZAMAN dönem başıdır (`terms.start_date`) — kullanıcı
//! kararı. `decide()` BİLEREK atlanır: ay penceresi / planlama kapısı bu
//! sistem düzeltmesi için anlamsızdır (aktarılan değer zaten dönem başında
//! geçerliydi, bugünün ayına göre "önceki ay kapandı" reddi burada yersizdir)
//! — olaylar `change_log`/`projection::rebuild_streams` PRİMİTİFLERİYLE
//! doğrudan yazılır.

use std::path::Path;

use chrono::NaiveDate;
use sqlx::{SqliteConnection, SqlitePool};

use super::backup;
use super::legacy_reconcile_plan::{build_plan, fetch_open_coordination, fetch_open_hours, ReconcilePlan};
use crate::db::{change_log, projection, settings, terms};
use crate::domain::history::decide::{NewChangeSet, PlannedEvent, StreamKey};
use crate::domain::history::events::{CoordinationState, EventPayload, HoursState, Labels, Stream};
use crate::error::{AppError, AppResult};

const RECONCILE_FLAG_KEY: &str = "legacy_board_reconciled";
const RECONCILE_REASON: &str = "Eski panodan tek seferlik aktarım";
/// `change_sets.actor`taki sistem işlemlerini kullanıcı adlarından ayırt eder.
const SYSTEM_ACTOR: &str = "sistem";

/// Açılışta, göçlerden ve `backfill_missing_term_rows`'tan SONRA çağrılır
/// (`db::init_pool`). Hata YUTULMAZ ama uygulamanın açılmasını da ENGELLEMEZ:
/// her başarısızlık `eprintln!` ile yazılır, işlem o açılışta atlanır.
pub async fn reconcile_legacy_board(pool: &SqlitePool, db_dir: &Path, today: NaiveDate) -> AppResult<()> {
    if settings::get(pool, RECONCILE_FLAG_KEY).await?.is_some() {
        return Ok(());
    }

    let plan = match build_plan(pool).await {
        Ok(plan) => plan,
        Err(e) => {
            eprintln!("Eski pano aktarımı okunamadı; bu açılışta atlandı: {e}");
            return Ok(());
        }
    };

    if plan.untouched_hours_only_in_projection > 0 {
        eprintln!(
            "Eski pano aktarımı: {} işletmenin projeksiyonda açık saat kaydı var ama eski tabloda karşılığı yok; \
             bunlara DOKUNULMADI.",
            plan.untouched_hours_only_in_projection
        );
    }

    if plan.is_empty() {
        // Yazılacak bir şey yok; yedek gerekmez ama bayrak GEREKİR (bkz. modül
        // başlığı) — aksi halde bir sonraki açılış aynı (boş) karşılaştırmayı
        // tekrar yapar; zararsız ama gereksiz. Asıl risk, kullanıcı arada bir
        // düzenleme YAPARSA (fark yeniden doğar) bayraksız ikinci çalıştırmanın
        // bunu geri almasıdır.
        settings::set(pool, RECONCILE_FLAG_KEY, "1").await?;
        return Ok(());
    }

    let backup_dir = backup::auto_backup_dir(db_dir);
    if let Err(e) = backup::create_pre_reconcile_backup(pool, &backup_dir, today).await {
        eprintln!("Eski pano aktarımı için yedek alınamadı; bu açılışta atlandı: {e}");
        return Ok(());
    }

    if let Err(e) = commit_plan(pool, &plan).await {
        eprintln!("Eski pano aktarımı başarısız oldu, hiçbir şey değiştirilmedi: {e}");
    }
    Ok(())
}

/// Planı TEK transaction'da yazar: her dönem için bir değişiklik kümesi,
/// ardından doğrulama. Doğrulama başarısız olursa `?` transaction'ı düşürür
/// (sqlx ROLLBACK yapar); bayrak da YAZILMAMIŞ olur.
async fn commit_plan(pool: &SqlitePool, plan: &ReconcilePlan) -> AppResult<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

    for term in plan.terms() {
        commit_term(&mut tx, plan, &term).await?;
    }
    verify_plan(&mut tx, plan).await?;
    write_reconciled_flag(&mut tx).await?;

    tx.commit().await?;
    Ok(())
}

/// Bir dönemin tüm farklarını TEK değişiklik kümesi olarak yazar ve
/// dokunulan akışları yeniden kurar.
async fn commit_term(conn: &mut SqliteConnection, plan: &ReconcilePlan, term: &str) -> AppResult<()> {
    terms::ensure_in(conn, term).await?;
    let term_start = terms::get_in(conn, term).await?.start;

    warn_about_later_events(conn, plan, term, term_start).await?;

    let events = build_events(plan, term, term_start);
    if events.is_empty() {
        return Ok(());
    }

    // `kind` BİLEREK "opening"tir: geri alınamazlık ve kalıcı silme kuralı
    // yalnız `kind = 'opening'` kümelerine uygulanır (`decide/revoke.rs`,
    // `domain::history::deletion::is_deletable`, `db/change_log.rs`in
    // `is_opening` türetmesi). Aktarım, kullanıcı kararıyla dönem başından
    // GEÇERLİ, düzeltilmiş bir açılıştır — anlamca "opening"in ta kendisi.
    // Hiçbir çalışma zamanı kodu bir dönem başına TEK açılış kümesi
    // varsaymaz (bu varsayım yalnız zaten çalışmış 0006/0008 göçlerinde
    // vardı); "Geri al"/kalıcı silme açık kalsaydı biri basınca saatler
    // bayat değerlere, atamalar sıfıra dönerdi.
    let change_set = NewChangeSet {
        term: term.to_string(),
        kind: "opening".to_string(),
        effective_date: term_start,
        document_date: None,
        reason: RECONCILE_REASON.to_string(),
        revokes_change_set_id: None,
    };
    let change_set_id = change_log::append_change_set(conn, &change_set, SYSTEM_ACTOR, "{}").await?;
    change_log::append_events(conn, change_set_id, term, &events).await?;

    let keys = touched_keys(plan, term);
    projection::rebuild_streams(conn, &keys, term_start).await
}

fn touched_keys(plan: &ReconcilePlan, term: &str) -> Vec<StreamKey> {
    let mut keys = Vec::new();
    for diff in plan.hours.iter().filter(|d| d.term == term) {
        keys.push(StreamKey { stream: Stream::CompanyHours, subject_id: diff.company_id, term: term.to_string() });
    }
    for diff in plan.coordinators.iter().filter(|d| d.term == term) {
        keys.push(StreamKey { stream: Stream::Coordination, subject_id: diff.company_id, term: term.to_string() });
    }
    for diff in plan.coordinator_ends.iter().filter(|d| d.term == term) {
        keys.push(StreamKey { stream: Stream::Coordination, subject_id: diff.company_id, term: term.to_string() });
    }
    keys
}

/// Bir dönemin farklarını dönem başında (`term_start`) geçerli olaylara
/// çevirir. `caused_by`/`revokes` yoktur: bunlar birbirinden bağımsız,
/// birincil aktarım olaylarıdır.
fn build_events(plan: &ReconcilePlan, term: &str, term_start: NaiveDate) -> Vec<PlannedEvent> {
    let mut events = Vec::new();

    for diff in plan.hours.iter().filter(|d| d.term == term) {
        events.push(PlannedEvent {
            stream: Stream::CompanyHours,
            subject_id: diff.company_id,
            effective_date: term_start,
            payload: EventPayload::HoursSet { state: diff.state.clone(), previous_awarded: diff.previous_awarded, labels: Labels(Default::default()) },
            caused_by: None,
            revokes: None,
        });
    }
    for diff in plan.coordinators.iter().filter(|d| d.term == term) {
        events.push(PlannedEvent {
            stream: Stream::Coordination,
            subject_id: diff.company_id,
            effective_date: term_start,
            payload: EventPayload::CoordinatorAssigned { state: diff.state.clone(), from_teacher_id: diff.from_teacher_id, labels: Labels(Default::default()) },
            caused_by: None,
            revokes: None,
        });
    }
    for diff in plan.coordinator_ends.iter().filter(|d| d.term == term) {
        events.push(PlannedEvent {
            stream: Stream::Coordination,
            subject_id: diff.company_id,
            effective_date: term_start,
            payload: EventPayload::CoordinatorEnded { from_teacher_id: diff.from_teacher_id, labels: Labels(Default::default()) },
            caused_by: None,
            revokes: None,
        });
    }
    events
}

/// Dönem başlangıcından SONRAKİ bir olayı olan (stream, özne) çiftleri için
/// uyarı yazar: aktarılan değer, o tarihten sonra zaten geçerli olan daha
/// yeni bir olay tarafından "gölgelenir" (spec fold kuralı gereği doğru
/// davranış, ama sessiz kalınırsa kafa karıştırır — brief bunu ister).
async fn warn_about_later_events(conn: &mut SqliteConnection, plan: &ReconcilePlan, term: &str, term_start: NaiveDate) -> AppResult<()> {
    let mut subjects: Vec<(Stream, i64)> = plan.hours.iter().filter(|d| d.term == term).map(|d| (Stream::CompanyHours, d.company_id)).collect();
    subjects.extend(plan.coordinators.iter().filter(|d| d.term == term).map(|d| (Stream::Coordination, d.company_id)));

    for (stream, subject_id) in subjects {
        let events = change_log::load_stream_events(conn, term, stream, subject_id).await?;
        if events.iter().any(|e| e.effective_date > term_start) {
            eprintln!(
                "Eski pano aktarımı: {} akışında {subject_id} numaralı öznenin dönem başından SONRAKİ bir olayı \
                 var; aktarılan değer yalnızca o tarihe kadar geçerli olacak.",
                stream.as_str()
            );
        }
    }
    Ok(())
}

/// Yazılanların gerçekten açık projeksiyon satırlarına yansıdığını doğrular
/// (spec: "yeniden kurulumdan sonra her eski tablo satırı için açık
/// projeksiyon satırı eşit olmalı"). Uyuşmazlık `AppError::Database` fırlatır;
/// çağıran bunu transaction'ı düşürerek (ROLLBACK) ele alır.
async fn verify_plan(conn: &mut SqliteConnection, plan: &ReconcilePlan) -> AppResult<()> {
    for diff in &plan.hours {
        verify_hours(conn, diff.company_id, &diff.term, &diff.state).await?;
    }
    for diff in &plan.coordinators {
        verify_coordination(conn, diff.company_id, &diff.term, Some(&diff.state)).await?;
    }
    for diff in &plan.coordinator_ends {
        verify_coordination(conn, diff.company_id, &diff.term, None).await?;
    }
    Ok(())
}

async fn verify_hours(conn: &mut SqliteConnection, company_id: i64, term: &str, expected: &HoursState) -> AppResult<()> {
    let current = fetch_open_hours(&mut *conn, company_id, term).await?;
    if current.as_ref() == Some(expected) {
        return Ok(());
    }
    Err(AppError::Database(format!(
        "Aktarım doğrulaması başarısız: {company_id} numaralı işletmenin {term} dönemindeki saat satırı beklenenle eşleşmiyor"
    )))
}

async fn verify_coordination(conn: &mut SqliteConnection, company_id: i64, term: &str, expected: Option<&CoordinationState>) -> AppResult<()> {
    let current = fetch_open_coordination(&mut *conn, company_id, term).await?;
    if current.as_ref() == expected {
        return Ok(());
    }
    Err(AppError::Database(format!(
        "Aktarım doğrulaması başarısız: {company_id} numaralı işletmenin {term} dönemindeki koordinatörlük satırı beklenenle eşleşmiyor"
    )))
}

async fn write_reconciled_flag(conn: &mut SqliteConnection) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?1, '1')
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(RECONCILE_FLAG_KEY)
    .execute(conn)
    .await?;
    Ok(())
}
