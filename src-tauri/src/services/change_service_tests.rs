//! `change_service` testleri: tek kapının atomikliği, önizlemenin yazmaması,
//! bayat önizleme reddi, havuz kullanmama ve karar başına bir senaryo.

use std::time::Duration;

use chrono::NaiveDate;
use serde_json::json;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use super::change_service::{execute_change, ChangeMode, ChangeOutcome};
use super::change_service_test_support::*;
use crate::db::{change_log, projection, terms};
use crate::domain::history::decide::{ChangeCommand, ImpactSummary, TransferTarget};
use crate::domain::history::rejection::RejectionCode;
use crate::domain::terms::TermDates;
use crate::error::AppError;

fn transfer_to_new(student_id: i64, from: i64, name: &str) -> ChangeCommand {
    ChangeCommand::TransferStudent { student_id, from_company_id: from, to: TransferTarget::New { company: new_company(name, Some(3.0)) } }
}

fn transfer_to_existing(student_id: i64, from: i64, to: i64) -> ChangeCommand {
    ChangeCommand::TransferStudent { student_id, from_company_id: from, to: TransferTarget::Existing { company_id: to } }
}

fn expect_rejected(outcome: ChangeOutcome) -> (RejectionCode, Option<NaiveDate>) {
    match outcome {
        ChangeOutcome::Rejected { code, suggested_date, .. } => (code, suggested_date),
        other => panic!("Rejected beklenirdi, gelen: {other:?}"),
    }
}

// ---------------------------------------------------------------- kapı davranışı

/// Önizleme, yerinde işletme oluşturma dahil HİÇBİR tabloya yazmaz.
#[tokio::test]
async fn preview_writes_nothing() {
    let w = world().await;
    let before = table_counts(&w.pool).await;

    let outcome = preview(&w.pool, request(Some(ymd(2026, 11, 3)), transfer_to_new(w.students[0], w.company_a, "Yeni İşletme")), november_today()).await;

    assert!(matches!(outcome, ChangeOutcome::Preview { .. }), "Preview beklenirdi: {outcome:?}");
    assert_eq!(table_counts(&w.pool).await, before, "önizleme hiçbir tabloyu değiştirmemeli");
}

/// `preview_writes_nothing`in boş bir test olmadığının kanıtı: aynı istek
/// commit edilince günlük, projeksiyon ve yerinde oluşturulan işletme yazılır.
#[tokio::test]
async fn commit_writes_log_projection_and_in_place_company() {
    let w = world().await;
    let companies_before = count_of(&w.pool, "companies").await;
    let sets_before = count_of(&w.pool, "change_sets").await;

    let req = request(Some(ymd(2026, 11, 3)), transfer_to_new(w.students[0], w.company_a, "Yeni İşletme"));
    let (change_set_id, _) = expect_committed(commit(&w.pool, req, november_today()).await);

    assert_eq!(count_of(&w.pool, "companies").await, companies_before + 1);
    assert_eq!(count_of(&w.pool, "change_sets").await, sets_before + 1);
    let new_company: i64 = sqlx::query_scalar("SELECT id FROM companies WHERE name = 'Yeni İşletme'").fetch_one(&w.pool).await.unwrap();
    assert_eq!(open_placement(&w.pool, w.students[0]).await, Some(new_company), "projeksiyon yeni işletmeyi göstermeli");

    let mut conn = w.pool.acquire().await.unwrap();
    assert!(projection::verify_term(&mut conn, TERM).await.unwrap().is_empty(), "projeksiyon günlükle tutarlı olmalı");
    assert!(change_set_id > 0);
}

/// Projeksiyon yazımı ortada patlarsa change set de, yerinde oluşturulan
/// işletme de, olaylar da kalmaz (spec §5 adım 7). Hata, projeksiyon
/// tablosundaki bir tetikleyiciyle zorlanır: log ve işletme o noktada
/// çoktan yazılmıştır.
#[tokio::test]
async fn commit_is_atomic() {
    let w = world().await;
    sqlx::query("CREATE TRIGGER force_projection_failure BEFORE INSERT ON student_placements BEGIN SELECT RAISE(ABORT, 'zorlanmış projeksiyon hatası'); END")
        .execute(&w.pool)
        .await
        .unwrap();
    let before = table_counts(&w.pool).await;

    let req = request(Some(ymd(2026, 11, 3)), transfer_to_new(w.students[0], w.company_a, "Yeni İşletme"));
    let result = execute_change(&w.pool, req, ChangeMode::Commit { expected_high_water: None }, november_today()).await;

    let err = result.expect_err("zorlanmış hata yayılmalı, Committed dönmemeli");
    assert!(err.to_string().contains("zorlanmış projeksiyon hatası"), "hata nedeni korunmalı: {err}");
    assert_eq!(table_counts(&w.pool).await, before, "change set, olaylar ve işletme geri alınmalı");
}

#[tokio::test]
async fn stale_high_water_returns_stale_and_writes_nothing() {
    let w = world().await;
    let stale_req = request(Some(ymd(2026, 11, 3)), transfer_to_new(w.students[0], w.company_a, "Yeni İşletme"));

    let ChangeOutcome::Preview { high_water, .. } = preview(&w.pool, stale_req.clone(), november_today()).await else {
        panic!("Preview beklenirdi");
    };
    // Önizlemeden sonra başka bir kayıt günlüğe girer.
    let other = request(Some(ymd(2026, 11, 4)), transfer_to_existing(w.students[1], w.company_a, w.company_b));
    expect_committed(commit(&w.pool, other, november_today()).await);
    let before = table_counts(&w.pool).await;

    let outcome = execute_change(&w.pool, stale_req.clone(), ChangeMode::Commit { expected_high_water: Some(high_water) }, november_today())
        .await
        .unwrap();

    assert!(matches!(outcome, ChangeOutcome::Stale { .. }), "Stale beklenirdi: {outcome:?}");
    assert_eq!(table_counts(&w.pool).await, before, "Stale hiçbir şey yazmamalı; yerinde işletme de oluşturulmamalı");

    // Yeniden önizleme güncel değeri verir ve commit kabul edilir.
    let ChangeOutcome::Preview { high_water: fresh, .. } = preview(&w.pool, stale_req.clone(), november_today()).await else {
        panic!("Preview beklenirdi");
    };
    assert_ne!(fresh, high_water);
    let accepted = execute_change(&w.pool, stale_req, ChangeMode::Commit { expected_high_water: Some(fresh) }, november_today()).await.unwrap();
    assert!(matches!(accepted, ChangeOutcome::Committed { .. }), "güncel high-water kabul edilmeli: {accepted:?}");
}

#[tokio::test]
async fn preview_high_water_equals_the_log_high_water() {
    let w = world().await;
    let mut conn = w.pool.acquire().await.unwrap();
    let expected = change_log::high_water(&mut conn).await.unwrap();
    drop(conn);
    assert!(expected > 0, "sahne olay üretmiş olmalı");

    let outcome = preview(&w.pool, request(Some(ymd(2026, 11, 3)), transfer_to_existing(w.students[0], w.company_a, w.company_b)), november_today()).await;

    let ChangeOutcome::Preview { high_water, .. } = outcome else { panic!("Preview beklenirdi") };
    assert_eq!(high_water, expected);
}

/// Reddedilen istek, yerinde oluşturduğu işletmeyle birlikte geri alınır.
#[tokio::test]
async fn rejected_rolls_back() {
    let w = world().await;
    let before = table_counts(&w.pool).await;

    // 5 Ekim: önceki ay, `decide` reddeder — ama işletme o noktada çoktan açılmıştır.
    let req = request(Some(ymd(2026, 10, 5)), transfer_to_new(w.students[0], w.company_a, "Yeni İşletme"));
    let outcome = commit(&w.pool, req, november_today()).await;

    assert_eq!(expect_rejected(outcome), (RejectionCode::PreviousMonthClosed, Some(ymd(2026, 11, 1))));
    assert_eq!(table_counts(&w.pool).await, before, "reddedilen istek hiçbir şey bırakmamalı");
}

/// `max_connections(1)`: transaction açıkken havuzdan bağlantı istenirse
/// istek sonsuza dek (burada 3 sn) bekler ve test düşer.
#[tokio::test]
async fn no_pool_use_inside_tx() {
    let (dir, seeded) = test_pool().await;
    seeded.close().await;
    let options = SqliteConnectOptions::new().filename(dir.path().join("test.db")).foreign_keys(true);
    let pool = SqlitePoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(3)).connect_with(options).await.unwrap();

    let a = add_company(&pool, "İşletme A", 3.0).await;
    let s = add_student(&pool, "Ada", Some(a), planning_today()).await;

    let req = request(Some(ymd(2026, 11, 3)), transfer_to_new(s, a, "Yeni İşletme"));
    let outcome = execute_change(&pool, req, ChangeMode::Commit { expected_high_water: None }, november_today())
        .await
        .expect("tek bağlantılı havuzda transaction içinde havuz kullanılmamalı");

    assert!(matches!(outcome, ChangeOutcome::Committed { .. }), "{outcome:?}");
}

// ---------------------------------------------------------------- sözleşme (spec §8)

fn empty_impact_json() -> serde_json::Value {
    json!({"effectiveDate": "2026-11-03", "isPlanning": false, "shadowedUntil": null, "primary": [], "automatic": [], "warnings": [], "notices": []})
}

fn wire(outcome: &ChangeOutcome) -> serde_json::Value {
    serde_json::from_str(&serde_json::to_string(outcome).unwrap()).unwrap()
}

#[test]
fn rejected_outcome_serializes_to_the_spec_shape() {
    let outcome = ChangeOutcome::Rejected {
        code: RejectionCode::PreviousMonthClosed,
        reason: "kapalı".to_string(),
        conflicting_change_set_ids: vec![4, 7],
        suggested_date: Some(ymd(2026, 11, 1)),
    };
    let expected = json!({"status": "rejected", "code": "previousMonthClosed", "reason": "kapalı", "conflictingChangeSetIds": [4, 7], "suggestedDate": "2026-11-01"});
    assert_eq!(wire(&outcome), expected);
}

/// `suggestedDate` yoksa alan ATLANMAZ, `null` gelir (spec §8: `suggestedDate | null`).
#[test]
fn rejected_outcome_keeps_a_null_suggested_date() {
    let outcome = ChangeOutcome::Rejected { code: RejectionCode::HasDependents, reason: "önce #5".to_string(), conflicting_change_set_ids: vec![5], suggested_date: None };
    let expected = json!({"status": "rejected", "code": "hasDependents", "reason": "önce #5", "conflictingChangeSetIds": [5], "suggestedDate": null});
    assert_eq!(wire(&outcome), expected);
}

#[test]
fn stale_outcome_serializes_to_the_spec_shape() {
    let outcome = ChangeOutcome::Stale { message: "bayat".to_string() };
    assert_eq!(wire(&outcome), json!({"status": "stale", "message": "bayat"}));
}

#[test]
fn preview_outcome_serializes_to_the_spec_shape() {
    let outcome = ChangeOutcome::Preview { impact: ImpactSummary::empty(ymd(2026, 11, 3), false), high_water: 42 };
    assert_eq!(wire(&outcome), json!({"status": "preview", "impact": empty_impact_json(), "highWater": 42}));
}

#[test]
fn committed_outcome_serializes_to_the_spec_shape() {
    let outcome = ChangeOutcome::Committed { change_set_id: 9, impact: ImpactSummary::empty(ymd(2026, 11, 3), false) };
    assert_eq!(wire(&outcome), json!({"status": "committed", "changeSetId": 9, "impact": empty_impact_json()}));
}

// ---------------------------------------------------------------- karar başına senaryo

/// D2 + D6: ay penceresi ve dönem sınır günleri (spec §5.1).
#[tokio::test]
async fn d2_d6_month_window_and_term_boundaries_in_started_term() {
    let w = world().await;
    let leave = |date: Option<NaiveDate>| request(date, ChangeCommand::StudentLeaves { student_id: w.students[0], from_company_id: w.company_a });
    let today = november_today();

    // Ayın 1'i (pencerenin ilk günü) kabul edilir; bir gün öncesi reddedilir.
    assert!(matches!(preview(&w.pool, leave(Some(ymd(2026, 11, 1))), today).await, ChangeOutcome::Preview { .. }));
    assert_eq!(expect_rejected(preview(&w.pool, leave(Some(ymd(2026, 10, 31))), today).await), (RejectionCode::PreviousMonthClosed, Some(ymd(2026, 11, 1))));

    // Dönem sonu dahil; ertesi gün dönem dışı.
    assert!(matches!(preview(&w.pool, leave(Some(ymd(2027, 1, 31))), today).await, ChangeOutcome::Preview { .. }));
    assert_eq!(expect_rejected(preview(&w.pool, leave(Some(ymd(2027, 2, 1))), today).await).0, RejectionCode::OutOfTerm);

    // Dönem başladıktan sonra tarih zorunlu.
    assert_eq!(expect_rejected(preview(&w.pool, leave(None), today).await).0, RejectionCode::EffectiveDateRequired);
}

/// D6: planlamada tarih boş bırakılabilir (dönem başı kullanılır); dönem
/// başından önceki gün dönem dışıdır. (Komut, başlangıç gününde yerleştirilmiş
/// bir öğrenciyi o gün ayırmaz: `d⁻` boş olduğundan `FactNotTrueAtDate`
/// olurdu; bu yüzden hiç öğrencisi olmayan B'nin saatine dokunulur.)
#[tokio::test]
async fn d6_planning_defaults_to_term_start_and_rejects_the_day_before() {
    let w = world().await;
    let zero_hours_for_b = |date: Option<NaiveDate>| request(date, ChangeCommand::SetCompanyHours { rows: vec![hours_row(w.company_b, 0)] });

    let outcome = preview(&w.pool, zero_hours_for_b(None), planning_today()).await;
    let ChangeOutcome::Preview { impact, .. } = outcome else { panic!("Preview beklenirdi, gelen: {outcome:?}") };
    assert_eq!(impact.effective_date, ymd(2026, 9, 1));
    assert!(impact.is_planning);

    assert_eq!(expect_rejected(preview(&w.pool, zero_hours_for_b(Some(ymd(2026, 8, 31))), planning_today()).await).0, RejectionCode::OutOfTerm);
    assert!(matches!(preview(&w.pool, zero_hours_for_b(Some(ymd(2026, 9, 1))), planning_today()).await, ChangeOutcome::Preview { .. }));
}

/// D3: zincir. Üç öğrencili işletmeden biri ayrılınca tavan 9'dan 8'e düşer;
/// takdir edilen 9 saat kendiliğinden 8'e iner ve bu, birincil olaya bağlı
/// otomatik bir olay olarak günlüğe girer.
#[tokio::test]
async fn d3_chain_caps_hours_when_a_student_leaves() {
    let w = world().await;
    let date = ymd(2026, 11, 3);
    assert_eq!(awarded_hours_on(&w.pool, w.company_a, date).await, Some(9));

    let req = request(Some(date), transfer_to_existing(w.students[0], w.company_a, w.company_b));
    let (_, impact) = expect_committed(commit(&w.pool, req, november_today()).await);

    assert!(impact.automatic.iter().any(|line| line.kind == "hours_capped"), "otomatik tavan düşüşü beklenirdi: {:?}", impact.automatic);
    assert_eq!(awarded_hours_on(&w.pool, w.company_a, date).await, Some(8), "projeksiyon düşürülmüş saati göstermeli");
    assert_eq!(awarded_hours_on(&w.pool, w.company_a, ymd(2026, 10, 15)).await, Some(9), "Ekim değişmemeli");

    let caused: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_events WHERE kind = 'hours_capped' AND caused_by IS NOT NULL")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(caused, 1, "caused_by gerçek olay kimliğine çözülmüş olmalı");
}

/// D4: öğretmen tarafı. Koordinatörün ek dersi artınca kapasite aşımı uyarısı
/// gelir; yük değişikliği yine de kaydedilir (uyarı engellemez).
#[tokio::test]
async fn d4_teacher_load_change_warns_when_capacity_is_exceeded() {
    let w = world().await;
    let mut load = standard_load();
    load.other_extra_hours = 20; // kalan bütçe 24-0-20 = 4 < A'nın 9 saati
    let req = request(Some(ymd(2026, 11, 5)), ChangeCommand::SetTeacherLoad { teacher_id: w.teacher, load });

    let (_, impact) = expect_committed(commit(&w.pool, req, november_today()).await);

    assert!(
        impact.warnings.iter().any(|x| x.code == crate::domain::history::impact::WarningCode::CapacityExceeded),
        "kapasite aşımı uyarısı beklenirdi: {:?}",
        impact.warnings
    );
    let other_extra: i64 = sqlx::query_scalar("SELECT other_extra_hours FROM teacher_load_periods WHERE teacher_id = ?1 AND valid_to IS NULL")
        .bind(w.teacher)
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(other_extra, 20);
}

// ---------------------------------------------------------------- satır eylemleri

/// Yerinde oluşturulan işletmeye yapılan nakil geri alınırsa `decide`
/// `DeactivateCompany` ister; bu, aynı transaction'da uygulanır (spec §5.4).
#[tokio::test]
async fn revoking_a_transfer_to_a_new_company_deactivates_it() {
    let w = world().await;
    let req = request(Some(ymd(2026, 11, 3)), transfer_to_new(w.students[0], w.company_a, "Yeni İşletme"));
    let (transfer_set, _) = expect_committed(commit(&w.pool, req, november_today()).await);
    let new_company: i64 = sqlx::query_scalar("SELECT id FROM companies WHERE name = 'Yeni İşletme'").fetch_one(&w.pool).await.unwrap();

    expect_committed(commit(&w.pool, request(Some(ymd(2026, 11, 3)), ChangeCommand::Revoke { change_set_id: transfer_set }), november_today()).await);

    let is_active: i64 = sqlx::query_scalar("SELECT is_active FROM companies WHERE id = ?1").bind(new_company).fetch_one(&w.pool).await.unwrap();
    assert_eq!(is_active, 0, "işletme silinmez, pasif olur");
    assert_eq!(open_placement(&w.pool, w.students[0]).await, Some(w.company_a), "öğrenci eski işletmesine döner");
}

/// Düzeltme, yerinde oluşturmayı `replacement` içinden de tetikler.
#[tokio::test]
async fn correct_materializes_the_replacement_company() {
    let w = world().await;
    let req = request(Some(ymd(2026, 11, 3)), transfer_to_existing(w.students[0], w.company_a, w.company_b));
    let (transfer_set, _) = expect_committed(commit(&w.pool, req, november_today()).await);

    let correction = ChangeCommand::Correct { change_set_id: transfer_set, replacement: Box::new(transfer_to_new(w.students[0], w.company_a, "Düzeltme İşletmesi")) };
    expect_committed(commit(&w.pool, request(Some(ymd(2026, 11, 3)), correction), november_today()).await);

    let created: i64 = sqlx::query_scalar("SELECT id FROM companies WHERE name = 'Düzeltme İşletmesi'").fetch_one(&w.pool).await.unwrap();
    assert_eq!(open_placement(&w.pool, w.students[0]).await, Some(created));
}

/// Silinebilecek öğrenci (hiç olayı yok) `DeleteStudent` satır eylemiyle
/// silinir; geçmişi olan öğrenci silinmez.
#[tokio::test]
async fn delete_student_applies_the_row_action_only_without_history() {
    let w = world().await;
    let lonely = add_student(&w.pool, "Yalnız", None, planning_today()).await;

    expect_committed(commit(&w.pool, request(None, ChangeCommand::DeleteStudent { student_id: lonely }), planning_today()).await);
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM students WHERE id = ?1").bind(lonely).fetch_one(&w.pool).await.unwrap();
    assert_eq!(remaining, 0, "satır eylemi uygulanmalı");

    let outcome = commit(&w.pool, request(None, ChangeCommand::DeleteStudent { student_id: w.students[0] }), planning_today()).await;
    // Açılış dışında olayı olan öğrenci: HasHistory. (Bu sahnede yerleştirme
    // planlamada `create_student` kümesiyle yapıldığı için açılış değildir.)
    assert_eq!(expect_rejected(outcome).0, RejectionCode::HasHistory);
    let kept: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM students WHERE id = ?1").bind(w.students[0]).fetch_one(&w.pool).await.unwrap();
    assert_eq!(kept, 1);
}

/// `createTeacher` yerinde öğretmen satırını ve yük olayını aynı kümede yazar.
#[tokio::test]
async fn create_teacher_writes_the_row_and_the_load_event() {
    let (_dir, pool) = test_pool().await;
    let id = add_teacher(&pool, "Kemal", planning_today()).await;

    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_events WHERE stream = 'teacher_load' AND subject_id = ?1").bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(events, 1);
    let base: i64 = sqlx::query_scalar("SELECT base_hours FROM teacher_load_periods WHERE teacher_id = ?1").bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(base, 15);
}

/// `copySchedulesFromTerm`: kaynak dönemin son programı bağlama taşınmazsa
/// komut hiçbir şey kopyalayamaz. Kaynak dönem burada `execute_in`in
/// `source_term` parametresiyle yüklenir.
#[tokio::test]
async fn copy_schedules_reads_the_source_term() {
    let (_dir, pool) = test_pool().await;
    let teacher = add_teacher(&pool, "Kemal", planning_today()).await;
    seed_previous_term_schedule(&pool, teacher).await;

    let command = ChangeCommand::CopySchedulesFromTerm { from_term: "2025-2026/1".to_string() };
    expect_committed(commit(&pool, request(None, command), planning_today()).await);

    let slots: String = sqlx::query_scalar("SELECT slots_json FROM teacher_schedule_periods WHERE teacher_id = ?1 AND term = ?2 AND valid_to IS NULL")
        .bind(teacher)
        .bind(TERM)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(slots, "[[2,4]]", "kaynak dönemin SON programı taşınmalı");
}

async fn seed_previous_term_schedule(pool: &SqlitePool, teacher_id: i64) {
    let mut conn = pool.acquire().await.unwrap();
    let previous = TermDates { term: "2025-2026/1".to_string(), start: ymd(2025, 9, 1), end: ymd(2026, 1, 31), dates_confirmed: true };
    terms::insert_in(&mut conn, &previous).await.unwrap();
    let set_id: i64 = sqlx::query_scalar(
        "INSERT INTO change_sets (term, kind, effective_date, document_date, reason, actor, recorded_at, revokes_change_set_id, impact_json)
         VALUES ('2025-2026/1', 'set_teacher_schedule', '2025-10-01', NULL, '', 'tester', datetime('now'), NULL, '{}') RETURNING id",
    )
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    let payload = r#"{"schedule":[[2,4]],"previousSlotCount":null,"source":"manual","labels":{}}"#;
    sqlx::query(
        "INSERT INTO change_events (change_set_id, stream, subject_id, term, kind, kind_version, effective_date, payload, caused_by, revokes)
         VALUES (?1, 'teacher_schedule', ?2, '2025-2026/1', 'schedule_set', 1, '2025-10-01', ?3, NULL, NULL)",
    )
    .bind(set_id)
    .bind(teacher_id)
    .bind(payload)
    .execute(&mut *conn)
    .await
    .unwrap();
}

// ---------------------------------------------------------------- sınırda doğrulama

/// Yerinde oluşturulan kayıtların girdisi, satır açılmadan önce doğrulanır;
/// hata `AppError::Validation` olarak yayılır ve hiçbir şey yazılmaz.
#[tokio::test]
async fn blank_in_place_inputs_are_validation_errors_and_write_nothing() {
    let w = world().await;
    let before = table_counts(&w.pool).await;
    let today = november_today();
    let date = Some(ymd(2026, 11, 3));

    let mut blank_student = student_input("Boş");
    blank_student.last_name = "  ".to_string();
    let student_cmd = ChangeCommand::CreateStudent { student: blank_student, company_id: None };
    let company_cmd = ChangeCommand::TransferStudent {
        student_id: w.students[0],
        from_company_id: w.company_a,
        to: TransferTarget::New { company: new_company("   ", Some(3.0)) },
    };
    let mut blank_teacher = teacher_profile("Boş");
    blank_teacher.field = String::new();
    let teacher_cmd = ChangeCommand::CreateTeacher { teacher: blank_teacher, load: standard_load() };

    for command in [student_cmd, company_cmd, teacher_cmd] {
        let result = execute_change(&w.pool, request(date, command), ChangeMode::Commit { expected_high_water: None }, today).await;
        assert!(matches!(result, Err(AppError::Validation(_))), "Validation beklenirdi: {result:?}");
    }
    assert_eq!(table_counts(&w.pool).await, before);
}

#[tokio::test]
async fn negative_hours_and_bad_weekdays_are_validation_errors() {
    let w = world().await;
    let today = november_today();

    let mut load = standard_load();
    load.other_extra_hours = -1;
    let bad_load = ChangeCommand::SetTeacherLoad { teacher_id: w.teacher, load };
    let bad_hours = ChangeCommand::SetCompanyHours { rows: vec![hours_row(w.company_a, -3)] };
    let mut row = coordinator_row(w.company_a, w.teacher);
    row.visit_day = 6;
    let bad_day = ChangeCommand::AssignCoordinators { rows: vec![row] };

    for command in [bad_load, bad_hours, bad_day] {
        let result = execute_change(&w.pool, request(Some(ymd(2026, 11, 3)), command), ChangeMode::Preview, today).await;
        assert!(matches!(result, Err(AppError::Validation(_))), "Validation beklenirdi: {result:?}");
    }
}

/// Bilinmeyen dönem `Rejected`a çevrilmez, `AppError` olarak yayılır.
#[tokio::test]
async fn unknown_term_propagates_as_an_app_error() {
    let w = world().await;
    let mut req = request(None, ChangeCommand::ClearCoordination);
    req.term = "1999-2000/1".to_string();

    let result = execute_change(&w.pool, req, ChangeMode::Preview, planning_today()).await;

    assert!(matches!(result, Err(AppError::NotFound(_))), "NotFound beklenirdi: {result:?}");
}
