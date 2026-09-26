//! Dönem tarihleri: okuma, yazma, ve güncelleme kuralları (spec §5.1).
//!
//! Kalıcılık katmanıdır — kuralların KENDİSİ (ay penceresi, yürürlük tarihi
//! çözümü) `domain::terms::TermDates`'te yaşar; burada yalnız DB'ye özgü
//! kısıtlar vardır: önceki bir ayın gün kümesi değişemez, başlangıç açılış
//! dışındaki bir olayın tarihinden sonraya alınamaz, bitiş herhangi bir
//! olayın tarihinden önceye alınamaz (spec §5.1 "update_term_dates kuralları").

use std::collections::BTreeSet;

use chrono::NaiveDate;
use sqlx::{SqlitePool, SqliteConnection};

use crate::domain::history::events::{EventPayload, StoredEvent};
use crate::domain::terms::{first_of_month, TermDates};
use crate::error::{AppError, AppResult};

use super::{change_log, projection};

const SELECT_COLUMNS: &str = "term, start_date, end_date, dates_confirmed";

#[derive(sqlx::FromRow)]
struct TermRow {
    term: String,
    start_date: NaiveDate,
    end_date: NaiveDate,
    dates_confirmed: i64,
}

impl From<TermRow> for TermDates {
    fn from(row: TermRow) -> Self {
        TermDates {
            term: row.term,
            start: row.start_date,
            end: row.end_date,
            dates_confirmed: row.dates_confirmed != 0,
        }
    }
}

pub async fn get_in(conn: &mut SqliteConnection, term: &str) -> AppResult<TermDates> {
    let sql = format!("SELECT {SELECT_COLUMNS} FROM terms WHERE term = ?1");
    let row: TermRow = sqlx::query_as(&sql)
        .bind(term)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Dönem bulunamadı: {term}")))?;
    Ok(row.into())
}

pub async fn list(pool: &SqlitePool) -> AppResult<Vec<TermDates>> {
    let sql = format!("SELECT {SELECT_COLUMNS} FROM terms ORDER BY term DESC");
    let rows: Vec<TermRow> = sqlx::query_as(&sql).fetch_all(pool).await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn insert_in(conn: &mut SqliteConnection, dates: &TermDates) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO terms (term, start_date, end_date, dates_confirmed, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&dates.term)
    .bind(dates.start)
    .bind(dates.end)
    .bind(i64::from(dates.dates_confirmed))
    .bind(change_log::recorded_at_now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Bilinen ama `terms` satırı olmayan bir dönem için varsayılan tarihleri
/// yazar. `INSERT OR IGNORE` kullanır: satır zaten varsa (özellikle
/// kullanıcının onayladığı tarihlerle) HİÇ dokunulmaz — bu üç çağrı yerinin
/// (`create_term`, `save_settings`, açılış tamamlaması) hepsinin idempotent
/// olmasını sağlayan tek kuraldır. Tarih kuralı burada TEKRAR yazılmaz;
/// tek kaynak `TermDates::default_for`, aynı seed migration 0006'nın
/// kullandığı varsayılanla birebir.
pub async fn ensure_in(conn: &mut SqliteConnection, term: &str) -> AppResult<()> {
    if term.trim().is_empty() {
        return Ok(());
    }
    let dates = TermDates::default_for(term);
    sqlx::query(
        "INSERT OR IGNORE INTO terms (term, start_date, end_date, dates_confirmed, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&dates.term)
    .bind(dates.start)
    .bind(dates.end)
    .bind(i64::from(dates.dates_confirmed))
    .bind(change_log::recorded_at_now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// `ensure_in`'in havuz üzerinden çağrılan sürümü — transaction içinde
/// olmayan çağıranlar (`create_term`, `save_settings`, açılış tamamlaması)
/// için.
pub async fn ensure(pool: &SqlitePool, term: &str) -> AppResult<()> {
    let mut conn = pool.acquire().await?;
    ensure_in(&mut conn, term).await
}

/// Dönem başı/sonunu günceller. Kabul edilirse aynı transaction içinde
/// `projection::rebuild_term` çalışır — spec §5.1'in son maddesi.
pub async fn update_dates(
    pool: &SqlitePool,
    term: &str,
    start: NaiveDate,
    end: NaiveDate,
    confirm: bool,
    today: NaiveDate,
) -> AppResult<TermDates> {
    if end <= start {
        return Err(AppError::Validation(
            "Dönem bitiş tarihi başlangıçtan sonra olmalıdır.".to_string(),
        ));
    }

    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let current = get_in(&mut tx, term).await?;
    reject_if_touches_a_past_month(&current, start, end, today)?;

    let events = change_log::load_term_events(&mut tx, term).await?;
    reject_if_start_moves_past_a_non_opening_event(&events, start)?;
    reject_if_end_moves_before_any_event(&events, end)?;

    sqlx::query("UPDATE terms SET start_date = ?1, end_date = ?2, dates_confirmed = ?3 WHERE term = ?4")
        .bind(start)
        .bind(end)
        .bind(i64::from(confirm))
        .bind(term)
        .execute(&mut *tx)
        .await?;

    projection::rebuild_term(&mut tx, term).await?;
    let updated = get_in(&mut tx, term).await?;
    tx.commit().await?;
    Ok(updated)
}

/// Ek ders puantajı ay sonunda mutemede hazırlanır (spec §2 madde 3):
/// bugünkü ayın 1'inden ÖNCEKİ hiçbir günün "dönem içinde mi" durumu
/// değişemez. Gün gün ilerlemek (en kötü durumda birkaç bin gün) bu
/// uygulamanın ölçeğinde önemsizdir; kapalı biçim yerine doğrudan ve
/// denetlenebilir bir karşılaştırmadır.
fn reject_if_touches_a_past_month(
    current: &TermDates,
    new_start: NaiveDate,
    new_end: NaiveDate,
    today: NaiveDate,
) -> AppResult<()> {
    let boundary = first_of_month(today);
    let mut d = current.start.min(new_start);
    while d < boundary {
        let was_in = d >= current.start && d <= current.end;
        let now_in = d >= new_start && d <= new_end;
        if was_in != now_in {
            return Err(AppError::Validation(format!(
                "{d} önceki bir aya düşüyor; o ayın ek ders puantajı ilçeye gönderildi, \
                 dönem tarihleri artık o günü değiştiremez."
            )));
        }
        d = d.succ_opt().expect("takvim gün sayacı taşmaz");
    }
    Ok(())
}

fn reject_if_start_moves_past_a_non_opening_event(events: &[StoredEvent], new_start: NaiveDate) -> AppResult<()> {
    let offender = live_events(events).into_iter().filter(|e| !e.is_opening).find(|e| e.effective_date < new_start);
    if let Some(e) = offender {
        return Err(AppError::Validation(format!(
            "Dönem başlangıcı {} tarihli bir kayıttan sonraya alınamaz.",
            e.effective_date
        )));
    }
    Ok(())
}

fn reject_if_end_moves_before_any_event(events: &[StoredEvent], new_end: NaiveDate) -> AppResult<()> {
    let offender = live_events(events).into_iter().find(|e| e.effective_date > new_end);
    if let Some(e) = offender {
        return Err(AppError::Validation(format!(
            "Dönem bitişi {} tarihli bir kayıttan önceye alınamaz.",
            e.effective_date
        )));
    }
    Ok(())
}

/// Geri alınmış olayları ve geri alma işaretlerini eler — `order_events`'in
/// yaptığı aynı filtreyi, tarihe göre yeniden SIRALAMADAN uygular (burada
/// sıraya değil, yalnız "canlı mı" sorusuna ihtiyaç var).
fn live_events(events: &[StoredEvent]) -> Vec<&StoredEvent> {
    let revoked_ids: BTreeSet<i64> = events.iter().filter_map(|e| e.revokes).collect();
    events
        .iter()
        .filter(|e| !matches!(e.payload, EventPayload::Revoked))
        .filter(|e| !revoked_ids.contains(&e.id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    /// `init_pool`'un seed'inden gelen '2026-2027/1' dışında bir dönem
    /// kaydeder — testler dönemi kendi başına kurup gözlemlemek ister.
    async fn seed_term(pool: &SqlitePool, term: &str, start: NaiveDate, end: NaiveDate) {
        let mut conn = pool.acquire().await.unwrap();
        insert_in(
            &mut conn,
            &TermDates { term: term.to_string(), start, end, dates_confirmed: true },
        )
        .await
        .unwrap();
    }

    /// Bir değişiklik kümesi + olay ekler; `kind='opening'` ise açılış
    /// sayılır (bkz. `change_log::load_term_events`'in `is_opening` türetimi).
    async fn seed_event(pool: &SqlitePool, term: &str, cs_kind: &str, effective_date: NaiveDate) {
        let mut conn = pool.acquire().await.unwrap();
        let cs_id: i64 = sqlx::query_scalar(
            "INSERT INTO change_sets (term, kind, effective_date, document_date, reason, actor, recorded_at, revokes_change_set_id, impact_json)
             VALUES (?1, ?2, ?3, NULL, '', 'tester', datetime('now'), NULL, '{}') RETURNING id",
        )
        .bind(term)
        .bind(cs_kind)
        .bind(effective_date)
        .fetch_one(&mut *conn)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO change_events (change_set_id, stream, subject_id, term, kind, kind_version, effective_date, payload, caused_by, revokes)
             VALUES (?1, 'placement', 1, ?2, 'student_placed', 1, ?3, '{\"toCompanyId\":1,\"fromCompanyId\":null,\"source\":\"manual\",\"labels\":{}}', NULL, NULL)",
        )
        .bind(cs_id)
        .bind(term)
        .bind(effective_date)
        .execute(&mut *conn)
        .await
        .unwrap();
    }

    /// Biçimi bozuk bir dönem adı en geniş aralığı almalı (spec §9 adım 1),
    /// `TermDates::default_for`'un "bozuk" kolu ile birebir.
    #[tokio::test]
    async fn malformed_legacy_term_gets_wide_dates() {
        let dir = tempfile::tempdir().unwrap();
        let options = SqliteConnectOptions::new().filename(dir.path().join("legacy.db")).create_if_missing(true).foreign_keys(true);
        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();

        for file in ["0001_initial.sql", "0002_seed_hour_rules.sql", "0003_students_term.sql", "0004_company_term_hours.sql", "0005_term_branch_hours.sql"] {
            let sql = std::fs::read_to_string(format!("migrations/{file}")).unwrap();
            sqlx::raw_sql(&sql).execute(&pool).await.unwrap();
        }

        sqlx::query("INSERT INTO students (first_name, last_name, grade, branch, company_id, term) VALUES ('Test', 'Öğrenci', '12/C', 'Dal', NULL, ?1)")
            .bind("bozuk-dönem-adı")
            .execute(&pool)
            .await
            .unwrap();

        let sql = std::fs::read_to_string("migrations/0006_history.sql").unwrap();
        sqlx::raw_sql(&sql).execute(&pool).await.unwrap();

        let row: TermRow = sqlx::query_as("SELECT term, start_date, end_date, dates_confirmed FROM terms WHERE term = ?1")
            .bind("bozuk-dönem-adı")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.start_date, ymd(2000, 1, 1));
        assert_eq!(row.end_date, ymd(2099, 12, 31));
        assert_eq!(row.dates_confirmed, 0);
    }

    #[tokio::test]
    async fn update_dates_rejects_changes_to_a_past_month() {
        let (_dir, pool) = test_pool().await;
        seed_term(&pool, "2026-2027/9", ymd(2026, 9, 1), ymd(2027, 1, 31)).await;

        // today 2026-11-15 -> boundary 2026-11-01; başlangıcı Ekim'e
        // çekmek Ekim'in gün kümesini değiştirir.
        let err = update_dates(&pool, "2026-2027/9", ymd(2026, 10, 1), ymd(2027, 1, 31), true, ymd(2026, 11, 15))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn update_dates_allows_moving_start_past_opening_events() {
        let (_dir, pool) = test_pool().await;
        seed_term(&pool, "2026-2027/9", ymd(2026, 9, 1), ymd(2027, 1, 31)).await;
        seed_event(&pool, "2026-2027/9", "opening", ymd(2026, 9, 1)).await;

        // today hâlâ Eylül'de: boundary 2026-09-01, başlangıcı 09-15'e
        // çekmek yalnız >= boundary günleri etkiler; açılış olayı hariçtir.
        let updated = update_dates(&pool, "2026-2027/9", ymd(2026, 9, 15), ymd(2027, 1, 31), true, ymd(2026, 9, 20))
            .await
            .unwrap();
        assert_eq!(updated.start, ymd(2026, 9, 15));
    }

    #[tokio::test]
    async fn update_dates_rejects_start_after_a_non_opening_event() {
        let (_dir, pool) = test_pool().await;
        seed_term(&pool, "2026-2027/9", ymd(2026, 9, 1), ymd(2027, 1, 31)).await;
        seed_event(&pool, "2026-2027/9", "place_student", ymd(2026, 9, 10)).await;

        let err = update_dates(&pool, "2026-2027/9", ymd(2026, 9, 15), ymd(2027, 1, 31), true, ymd(2026, 9, 12))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn update_dates_rejects_end_before_any_event() {
        let (_dir, pool) = test_pool().await;
        seed_term(&pool, "2026-2027/9", ymd(2026, 9, 1), ymd(2027, 1, 31)).await;
        seed_event(&pool, "2026-2027/9", "place_student", ymd(2026, 12, 10)).await;

        let err = update_dates(&pool, "2026-2027/9", ymd(2026, 9, 1), ymd(2026, 11, 30), true, ymd(2026, 9, 12))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn insert_then_get_roundtrips() {
        let (_dir, pool) = test_pool().await;
        seed_term(&pool, "2026-2027/9", ymd(2026, 9, 1), ymd(2027, 1, 31)).await;

        let mut conn = pool.acquire().await.unwrap();
        let fetched = get_in(&mut conn, "2026-2027/9").await.unwrap();
        assert_eq!(fetched.start, ymd(2026, 9, 1));
        assert!(fetched.dates_confirmed);
    }

    #[tokio::test]
    async fn list_includes_the_seeded_active_term() {
        let (_dir, pool) = test_pool().await;
        let terms = list(&pool).await.unwrap();
        assert!(terms.iter().any(|t| t.term == "2026-2027/1"), "0006 tohumu aktif dönemi kaydetmeli");
    }

    /// Yeni bir dönem için `ensure_in`, `TermDates::default_for` ile birebir
    /// aynı tarihleri ve onaysız durumu yazmalı (0006'nın seed'iyle aynı kural).
    #[tokio::test]
    async fn ensure_in_creates_default_unconfirmed_dates_for_a_new_term() {
        let (_dir, pool) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        ensure_in(&mut conn, "2027-2028/1").await.unwrap();

        let fetched = get_in(&mut conn, "2027-2028/1").await.unwrap();
        let expected = TermDates::default_for("2027-2028/1");
        assert_eq!(fetched.start, expected.start);
        assert_eq!(fetched.end, expected.end);
        assert!(!fetched.dates_confirmed);
    }

    /// `INSERT OR IGNORE` mevcut satıra dokunmamalı — onaylanmış tarihler
    /// yeniden çağrıda (idempotent olması gereken üç çağrı yeri için) korunur.
    #[tokio::test]
    async fn ensure_in_never_touches_an_existing_confirmed_row() {
        let (_dir, pool) = test_pool().await;
        seed_term(&pool, "2026-2027/9", ymd(2026, 10, 1), ymd(2027, 2, 15)).await;
        let mut conn = pool.acquire().await.unwrap();

        ensure_in(&mut conn, "2026-2027/9").await.unwrap();

        let fetched = get_in(&mut conn, "2026-2027/9").await.unwrap();
        assert_eq!(fetched.start, ymd(2026, 10, 1));
        assert_eq!(fetched.end, ymd(2027, 2, 15));
        assert!(fetched.dates_confirmed);
    }

    /// Boş dönem adı hiçbir şey yazmamalı — serbest metin kutusunun boş
    /// bırakılması ya da henüz dönem seçilmemiş durum sessizce yutulmamalı,
    /// ama hata da fırlatmamalı (çağıranlar zaten "boşsa çağırma" mantığında).
    #[tokio::test]
    async fn ensure_in_is_a_no_op_for_an_empty_term_name() {
        let (_dir, pool) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        ensure_in(&mut conn, "").await.unwrap();

        let err = get_in(&mut conn, "").await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }
}
