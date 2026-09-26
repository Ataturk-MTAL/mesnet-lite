//! `legacy_reconcile` testleri: farklı eski değerlerin dönem başında olaya
//! dönüştüğü, ikinci çalıştırmanın hiçbir şey eklemediği, aktarımdan SONRA
//! yeni yoldan yapılan bir düzenlemenin geri alınmadığı ve yedek alınamazsa
//! hiçbir şeyin yazılmadığı.

use chrono::NaiveDate;
use sqlx::SqlitePool;

use super::legacy_reconcile::reconcile_legacy_board;
use crate::db::{assignments, change_log, companies, company_hours, init_pool, projection, settings, teachers, terms};
use crate::domain::history::decide::{ChangeCommand, ChangeRequest, CompanyHoursRow, NewChangeSet, PlannedEvent, StreamKey};
use crate::domain::history::events::{CoordinationState, EventPayload, HoursState, Labels, Stream};
use crate::domain::history::rejection::RejectionCode;
use crate::domain::models::{NewCompany, NewTeacher};
use crate::error::AppError;
use crate::services::change_service::{execute_change, ChangeMode, ChangeOutcome};
use crate::services::history_delete::delete_change_set;
use crate::services::history_service::{list_history, HistoryFilter};

/// `legacy_reconcile::RECONCILE_REASON`in birebir kopyası — sabit `pub`
/// olmadığı için (yalnız bu modülün bilmesi gereken bir uygulama detayı)
/// testte gerekçe metniyle kümeyi bulmak için tekrarlanır.
const RECONCILE_REASON: &str = "Eski panodan tek seferlik aktarım";

const TERM: &str = "2026-2027/1";
const FLAG_KEY: &str = "legacy_board_reconciled";

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

/// `init_pool` kendisi de açılışta `reconcile_legacy_board`i çağırır (brief
/// sözleşmesi); TAZE bir test veritabanında eski tablolar zaten boş olduğu
/// için bu ilk çağrı FARKSIZ bulur ve bayrağı hemen yazar. Testler bu
/// bayrağı SIFIRLAR ki "eski tablolarda veri VARDI ama henüz aktarılmadı"
/// senaryosunu (gerçek yükseltme durumu) kurabilsinler.
async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
    sqlx::query("DELETE FROM settings WHERE key = ?1").bind(FLAG_KEY).execute(&pool).await.unwrap();
    (dir, pool)
}

/// `company_hours::get` yoktur (tek çağıranı bu testlerdi, kaldırıldı);
/// panonun kendisi de bu şekilde okur — `list` üzerinden filtreler.
async fn open_hours(pool: &SqlitePool, company_id: i64) -> Option<company_hours::CompanyTermHours> {
    company_hours::list(pool, TERM).await.unwrap().into_iter().find(|h| h.company_id == company_id)
}

async fn a_company(pool: &SqlitePool, name: &str) -> i64 {
    companies::create(
        pool,
        &NewCompany {
            name: name.into(),
            contact_first_name: String::new(),
            contact_last_name: String::new(),
            phone: String::new(),
            email: String::new(),
            address_text: "Test adres".into(),
            latitude: None,
            longitude: None,
            one_way_distance_km: Some(5.0),
            district: String::new(),
            notes: String::new(),
        },
    )
    .await
    .unwrap()
    .id
}

/// Bir öğrenciyi GERÇEK yazma yolundan (`execute_change`) işletmeye
/// yerleştirir — `cap_for` 0 öğrencide mesafeden bağımsız `Some(0)` tavan
/// koyar; kapasiteli bir düzenleme sınayan testlerin en az bir öğrenciye
/// ihtiyacı var.
async fn place_student(pool: &SqlitePool, company_id: i64) {
    let req = ChangeRequest {
        term: TERM.to_string(),
        effective_date: None,
        document_date: None,
        reason: "test".into(),
        command: ChangeCommand::CreateStudent {
            student: crate::domain::history::decide::NewStudentInput {
                first_name: "Test".into(),
                last_name: "Öğrenci".into(),
                student_no: None,
                grade: "12/C".into(),
                branch: "Elektronik Haberleşme".into(),
                submitted_at: None,
            },
            company_id: Some(company_id),
        },
    };
    let outcome = execute_change(pool, req, ChangeMode::Commit { expected_high_water: None }, ymd(2026, 8, 15)).await.unwrap();
    assert!(matches!(outcome, ChangeOutcome::Committed { .. }));
}

async fn a_teacher(pool: &SqlitePool, last_name: &str) -> i64 {
    teachers::create(
        pool,
        &NewTeacher {
            first_name: "Test".into(),
            last_name: last_name.into(),
            registry_no: String::new(),
            field: "Elektrik-Elektronik Teknolojisi".into(),
            branches: vec![],
            employment_type: "tenured".into(),
            base_hours: 20,
            max_extra_hours: 24,
            other_extra_hours: 0,
            chief_type: "none".into(),
            is_active: true,
        },
    )
    .await
    .unwrap()
    .id
}

/// Eski panonun "canlı" yazdığı satırı doğrudan taklit eder — bu tabloların
/// gerçek eski yazıcıları bu iş tarafından KALDIRILDI, bu yüzden test
/// verisini ham SQL ile kurmak zorundayız (tablo şeması hâlâ DUR).
async fn seed_legacy_hours(pool: &SqlitePool, company_id: i64, awarded: i64, notes: &str) {
    sqlx::query(
        "INSERT INTO company_term_hours
            (company_id, term, max_hours_snapshot, awarded_hours, is_honorary, is_locked, notes, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 0, 0, ?5, '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z')",
    )
    .bind(company_id)
    .bind(TERM)
    .bind(awarded + 2)
    .bind(awarded)
    .bind(notes)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_legacy_assignment(pool: &SqlitePool, teacher_id: i64, company_id: i64, day: i64, hour: i64) {
    sqlx::query(
        "INSERT INTO assignments (teacher_id, company_id, term, visit_day, visit_hour, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z')",
    )
    .bind(teacher_id)
    .bind(company_id)
    .bind(TERM)
    .bind(day)
    .bind(hour)
    .execute(pool)
    .await
    .unwrap();
}

/// Dönem başında (`terms.start_date`) `kind = 'opening'` bir `hours_set`
/// olayı yazar — 0006/0008 göçlerinin gerçek şirket açılışında bıraktığı
/// BAYAT değeri taklit eder. Aktarımın kendi olayı da AYNI güne (dönem
/// başına) yazılır; canlı veride tam bu çakışma var.
async fn seed_opening_hours(pool: &SqlitePool, company_id: i64, awarded: i64) {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let term_start = terms::get_in(&mut tx, TERM).await.unwrap().start;
    let cs = NewChangeSet { term: TERM.to_string(), kind: "opening".to_string(), effective_date: term_start, document_date: None, reason: "test açılışı".to_string(), revokes_change_set_id: None };
    let cs_id = change_log::append_change_set(&mut tx, &cs, "test", "{}").await.unwrap();
    let state = HoursState { awarded_hours: awarded, max_hours_snapshot: awarded.max(1), is_honorary: false, is_locked: false, notes: String::new() };
    let event = PlannedEvent { stream: Stream::CompanyHours, subject_id: company_id, effective_date: term_start, payload: EventPayload::HoursSet { state, previous_awarded: None, labels: Labels(Default::default()) }, caused_by: None, revokes: None };
    change_log::append_events(&mut tx, cs_id, TERM, &[event]).await.unwrap();
    projection::rebuild_streams(&mut tx, &[StreamKey { stream: Stream::CompanyHours, subject_id: company_id, term: TERM.to_string() }], term_start).await.unwrap();
    tx.commit().await.unwrap();
}

/// Dönem başında `kind = 'opening'` bir `coordinator_assigned` olayı yazar.
async fn seed_opening_coordinator(pool: &SqlitePool, company_id: i64, teacher_id: i64, visit_day: i64, visit_hour: i64) {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let term_start = terms::get_in(&mut tx, TERM).await.unwrap().start;
    let cs = NewChangeSet { term: TERM.to_string(), kind: "opening".to_string(), effective_date: term_start, document_date: None, reason: "test açılışı".to_string(), revokes_change_set_id: None };
    let cs_id = change_log::append_change_set(&mut tx, &cs, "test", "{}").await.unwrap();
    let state = CoordinationState { teacher_id, visit_day, visit_hour, is_forced: false, force_reason: None };
    let event = PlannedEvent { stream: Stream::Coordination, subject_id: company_id, effective_date: term_start, payload: EventPayload::CoordinatorAssigned { state, from_teacher_id: None, labels: Labels(Default::default()) }, caused_by: None, revokes: None };
    change_log::append_events(&mut tx, cs_id, TERM, &[event]).await.unwrap();
    projection::rebuild_streams(&mut tx, &[StreamKey { stream: Stream::Coordination, subject_id: company_id, term: TERM.to_string() }], term_start).await.unwrap();
    tx.commit().await.unwrap();
}

/// Ana senaryo: eski tablodaki değerler projeksiyondan (boş) farklı; açılış
/// sonrası projeksiyon eski tabloyla eşitlenir, olaylar dönem başında
/// (2026-09-01) görünür, bayrak yazılır, yedek dosyası oluşur.
#[tokio::test]
async fn reconcile_moves_differing_legacy_values_into_the_projection_at_term_start() {
    let (dir, pool) = test_pool().await;
    let company_id = a_company(&pool, "İşletme A").await;
    let teacher_id = a_teacher(&pool, "Yılmaz").await;
    seed_legacy_hours(&pool, company_id, 9, "eski not").await;
    seed_legacy_assignment(&pool, teacher_id, company_id, 3, 4).await;

    reconcile_legacy_board(&pool, dir.path(), ymd(2026, 11, 10)).await.unwrap();

    let hours = open_hours(&pool, company_id).await.unwrap();
    assert_eq!(hours.awarded_hours, 9);
    assert_eq!(hours.notes, "eski not");

    let assignment = assignments::get_for_company(&pool, company_id, TERM).await.unwrap().unwrap();
    assert_eq!((assignment.visit_day, assignment.visit_hour), (3, 4));

    let hours_date: NaiveDate = sqlx::query_scalar(
        "SELECT effective_date FROM change_events WHERE stream = 'company_hours' AND subject_id = ?1",
    )
    .bind(company_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(hours_date, ymd(2026, 9, 1), "yürürlük tarihi dönem başı olmalı");

    assert_eq!(settings::get(&pool, FLAG_KEY).await.unwrap().as_deref(), Some("1"));
    assert!(dir.path().join("backups").join("pre-legacy-reconcile-2026-11-10.db").exists());
}

/// Eski tabloda ataması OLMAYAN ama açık koordinatörlük satırı olan işletme
/// için koordinatörlük BİTİRİLİR.
#[tokio::test]
async fn reconcile_ends_a_coordination_with_no_legacy_counterpart() {
    let (dir, pool) = test_pool().await;
    let company_id = a_company(&pool, "İşletme A").await;
    let teacher_id = a_teacher(&pool, "Yılmaz").await;

    let req = ChangeRequest {
        term: TERM.to_string(),
        effective_date: None,
        document_date: None,
        reason: "test".into(),
        command: ChangeCommand::AssignCoordinators {
            rows: vec![crate::domain::history::decide::CoordinatorRow {
                company_id,
                teacher_id,
                visit_day: 1,
                visit_hour: 3,
                is_forced: false,
                force_reason: None,
            }],
        },
    };
    let outcome = execute_change(&pool, req, ChangeMode::Commit { expected_high_water: None }, ymd(2026, 8, 15)).await.unwrap();
    assert!(matches!(outcome, ChangeOutcome::Committed { .. }));

    reconcile_legacy_board(&pool, dir.path(), ymd(2026, 11, 10)).await.unwrap();

    assert!(assignments::get_for_company(&pool, company_id, TERM).await.unwrap().is_none(), "koordinatörlük bitirilmeli");
}

/// İkinci çalıştırma bayrak yüzünden HİÇBİR olay eklemez.
#[tokio::test]
async fn second_run_adds_no_events() {
    let (dir, pool) = test_pool().await;
    let company_id = a_company(&pool, "İşletme A").await;
    seed_legacy_hours(&pool, company_id, 9, "eski not").await;

    reconcile_legacy_board(&pool, dir.path(), ymd(2026, 11, 10)).await.unwrap();
    let count_after_first: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_events").fetch_one(&pool).await.unwrap();

    reconcile_legacy_board(&pool, dir.path(), ymd(2026, 11, 11)).await.unwrap();
    let count_after_second: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_events").fetch_one(&pool).await.unwrap();

    assert_eq!(count_after_first, count_after_second, "ikinci çalıştırma yeni olay eklememeli");
}

/// KRİTİK davranış: aktarımdan SONRA yeni yoldan yapılan bir düzenleme,
/// bayrak yüzünden ikinci bir açılışta GERİ ALINMAZ. Karşılaştırmaya dayalı
/// (bayraksız) bir idempotency burada kullanıcının düzenlemesini eski
/// değere döndürürdü — bu tam olarak brief'in uyardığı hata.
#[tokio::test]
async fn a_later_edit_through_the_new_system_survives_a_second_startup() {
    let (dir, pool) = test_pool().await;
    let company_id = a_company(&pool, "İşletme A").await;
    place_student(&pool, company_id).await;
    seed_legacy_hours(&pool, company_id, 9, "eski not").await;

    reconcile_legacy_board(&pool, dir.path(), ymd(2026, 11, 10)).await.unwrap();

    // Kullanıcı artık YENİ yoldan (tarihçe kapısından) düzenliyor.
    let req = ChangeRequest {
        term: TERM.to_string(),
        effective_date: Some(ymd(2026, 11, 15)),
        document_date: None,
        reason: "kullanıcı düzenlemesi".into(),
        command: ChangeCommand::SetCompanyHours {
            rows: vec![CompanyHoursRow { company_id, awarded_hours: 3, is_honorary: false, is_locked: false, notes: "yeni not".into() }],
        },
    };
    let outcome = execute_change(&pool, req, ChangeMode::Commit { expected_high_water: None }, ymd(2026, 11, 15)).await.unwrap();
    assert!(matches!(outcome, ChangeOutcome::Committed { .. }), "kullanıcı düzenlemesi kabul edilmeli");

    // İkinci "açılış": eski tablo (donmuş) hâlâ 9 diyor, fark yeniden doğdu,
    // ama bayrak zaten var — hiçbir şey değişmemeli.
    reconcile_legacy_board(&pool, dir.path(), ymd(2026, 11, 20)).await.unwrap();

    let hours = open_hours(&pool, company_id).await.unwrap();
    assert_eq!(hours.awarded_hours, 3, "kullanıcının düzenlemesi GERİ ALINMAMALI");
    assert_eq!(hours.notes, "yeni not");
}

/// Yedek alınamazsa aktarım hiçbir şey yazmaz, bayrak da yazılmaz.
#[cfg(unix)]
#[tokio::test]
async fn writes_nothing_when_the_backup_cannot_be_taken() {
    use std::os::unix::fs::PermissionsExt;

    let (dir, pool) = test_pool().await;
    let company_id = a_company(&pool, "İşletme A").await;
    seed_legacy_hours(&pool, company_id, 9, "eski not").await;

    let backup_dir = dir.path().join("backups");
    std::fs::create_dir_all(&backup_dir).unwrap();
    std::fs::set_permissions(&backup_dir, std::fs::Permissions::from_mode(0o500)).unwrap();

    reconcile_legacy_board(&pool, dir.path(), ymd(2026, 11, 10)).await.unwrap();

    // Testin geçici dizini silinebilsin diye izinleri geri veriyoruz.
    std::fs::set_permissions(&backup_dir, std::fs::Permissions::from_mode(0o700)).unwrap();

    assert!(settings::get(&pool, FLAG_KEY).await.unwrap().is_none(), "bayrak yazılmamalı");
    assert!(open_hours(&pool, company_id).await.is_none(), "projeksiyona hiçbir şey yazılmamalı");
}

/// Canlı veride tam bu durum var: dönem başında bayat bir `opening`
/// `hours_set` olayı (0 saat) zaten var; eski tabloda 6 saat. Aktarımın
/// olayı AYNI güne (dönem başı) yazılır — `fold_intervals`in "aynı günün
/// SON (en yüksek id'li) olayı kazanır" kuralı sayesinde `company_hour_periods`e
/// TEK satır yazılmalı (`UNIQUE (company_id, term, valid_from)` çakışmamalı)
/// ve o satır eski tablonun değerini (6) taşımalı.
#[tokio::test]
async fn reconcile_resolves_a_same_day_collision_with_the_opening_event_for_hours() {
    let (dir, pool) = test_pool().await;
    let company_id = a_company(&pool, "İşletme A").await;
    seed_opening_hours(&pool, company_id, 0).await;
    seed_legacy_hours(&pool, company_id, 6, "eski not").await;

    reconcile_legacy_board(&pool, dir.path(), ymd(2026, 11, 10)).await.unwrap();

    let hours = open_hours(&pool, company_id).await.unwrap();
    assert_eq!(hours.awarded_hours, 6, "aktarımın değeri açılışın bayat değerini geçmeli");

    let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM company_hour_periods WHERE company_id = ?1 AND term = ?2")
        .bind(company_id)
        .bind(TERM)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row_count, 1, "aynı gün tek dönem satırına katlanmalı, UNIQUE çakışmamalı");

    assert_eq!(settings::get(&pool, FLAG_KEY).await.unwrap().as_deref(), Some("1"));
}

async fn reconcile_change_set_id(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT id FROM change_sets WHERE reason = ?1")
        .bind(RECONCILE_REASON)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Aktarım kümesi `kind = 'opening'`dir: geri alınamaz ve kalıcı silinemez
/// (`decide/revoke.rs::validate_revocable`, `domain::history::deletion::is_deletable`);
/// Tarihçe'de yalnız `includeOpening: true` iken görünür — düzeltme #1'in kanıtı.
#[tokio::test]
async fn the_reconcile_set_is_an_opening_set_that_cannot_be_revoked_or_deleted() {
    let (dir, pool) = test_pool().await;
    let company_id = a_company(&pool, "İşletme A").await;
    seed_legacy_hours(&pool, company_id, 6, "eski not").await;
    reconcile_legacy_board(&pool, dir.path(), ymd(2026, 11, 10)).await.unwrap();
    let change_set_id = reconcile_change_set_id(&pool).await;

    let revoke_req = ChangeRequest {
        term: TERM.to_string(),
        effective_date: None,
        document_date: None,
        reason: "test".into(),
        command: ChangeCommand::Revoke { change_set_id },
    };
    let outcome = execute_change(&pool, revoke_req, ChangeMode::Commit { expected_high_water: None }, ymd(2026, 11, 10)).await.unwrap();
    match outcome {
        ChangeOutcome::Rejected { code, .. } => assert_eq!(code, RejectionCode::NotRevocable, "açılış kümesi NotRevocable ile reddedilmeli"),
        other => panic!("Rejected(NotRevocable) beklenirdi: {other:?}"),
    }

    let delete_err = delete_change_set(&pool, change_set_id).await.unwrap_err();
    assert!(matches!(delete_err, AppError::Validation(_)), "açılış kümesi kalıcı silinemez: {delete_err:?}");

    let hidden = list_history(
        &pool,
        &HistoryFilter { term: TERM.to_string(), stream: None, subject_id: None, company_id: None, teacher_id: None, include_opening: false, before_change_set_id: None, limit: 50 },
        ymd(2026, 11, 10),
    )
    .await
    .unwrap();
    assert!(hidden.entries.iter().all(|e| e.change_set_id != change_set_id), "includeOpening: false iken aktarım kümesi görünmemeli");

    let shown = list_history(
        &pool,
        &HistoryFilter { term: TERM.to_string(), stream: None, subject_id: None, company_id: None, teacher_id: None, include_opening: true, before_change_set_id: None, limit: 50 },
        ymd(2026, 11, 10),
    )
    .await
    .unwrap();
    assert!(shown.entries.iter().any(|e| e.change_set_id == change_set_id), "includeOpening: true iken aktarım kümesi görünmeli");
}

/// Aynısı koordinatörlük için: dönem başında açılış ataması öğretmen A, eski
/// tabloda öğretmen B. Aktarımdan sonra açık satır B'yi göstermeli, tek satır
/// olmalı.
#[tokio::test]
async fn reconcile_resolves_a_same_day_collision_with_the_opening_event_for_coordination() {
    let (dir, pool) = test_pool().await;
    let company_id = a_company(&pool, "İşletme A").await;
    let teacher_a = a_teacher(&pool, "Ayşe").await;
    let teacher_b = a_teacher(&pool, "Bora").await;
    seed_opening_coordinator(&pool, company_id, teacher_a, 1, 3).await;
    seed_legacy_assignment(&pool, teacher_b, company_id, 2, 4).await;

    reconcile_legacy_board(&pool, dir.path(), ymd(2026, 11, 10)).await.unwrap();

    let assignment = assignments::get_for_company(&pool, company_id, TERM).await.unwrap().unwrap();
    assert_eq!(assignment.teacher_id, teacher_b, "aktarımın öğretmeni açılışın bayat öğretmenini geçmeli");
    assert_eq!((assignment.visit_day, assignment.visit_hour), (2, 4));

    let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM coordination_periods WHERE company_id = ?1 AND term = ?2")
        .bind(company_id)
        .bind(TERM)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row_count, 1, "aynı gün tek dönem satırına katlanmalı, UNIQUE çakışmamalı");

    assert_eq!(settings::get(&pool, FLAG_KEY).await.unwrap().as_deref(), Some("1"));
}
