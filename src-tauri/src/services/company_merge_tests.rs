//! `company_merge` testleri: kaynaktaki her şeyin hedefe taşınması, tek
//! transaction'ın atomikliği ve sınırda doğrulama.

use sqlx::SqlitePool;

use super::change_service_test_support::*;
use super::company_merge::{apply_company_merge, preview_company_merge};
use crate::domain::history::decide::ChangeCommand;
use crate::error::AppError;

/// Kurulumdaki öğrenciler dönem başında (2026-09-01) yerleştirilir; birleştirme
/// AYNI günü kullanırsa `d⁻` boş kalır ve "beklenen işletmede değil" reddi
/// gelir (bkz. `change_service_test_support::world`'ün aynı notu). Bu yüzden
/// mutlu yol testleri, yerleştirmeden SONRAKİ bir günü açıkça verir.
fn merge_date() -> Option<chrono::NaiveDate> {
    Some(ymd(2026, 9, 15))
}

/// İki işletme, her birinde bir öğrenci, ikisine de tavan içi bir saat
/// takdir edilmiş — brief'in "gerçek senaryo" fikstürü. Planlama evresinde
/// kurulur ki kurulum adımları tarih vermeden commit edilebilsin.
struct Pair {
    pool: SqlitePool,
    from: i64,
    into: i64,
    student_from: i64,
    student_into: i64,
}

async fn pair_with_students_and_hours() -> (tempfile::TempDir, Pair) {
    let (dir, pool) = test_pool().await;
    let today = planning_today();
    let from = add_company(&pool, "örnek Mekatronik Sanayi A.Ş.", 3.0).await;
    let into = add_company(&pool, "ÖRNEK MEKATRONİK SANAYİ", 3.0).await;
    let student_from = add_student(&pool, "Ada", Some(from), today).await;
    let student_into = add_student(&pool, "Bora", Some(into), today).await;

    // Gidiş-dönüş 6 km, 1 öğrenci → tavan 8 (bkz. `migrations/0002_seed_hour_rules.sql`).
    expect_committed(commit(&pool, request(None, ChangeCommand::SetCompanyHours { rows: vec![hours_row(from, 8)] }), today).await);
    expect_committed(commit(&pool, request(None, ChangeCommand::SetCompanyHours { rows: vec![hours_row(into, 8)] }), today).await);

    (dir, Pair { pool, from, into, student_from, student_into })
}

async fn is_active(pool: &SqlitePool, company_id: i64) -> bool {
    let value: i64 = sqlx::query_scalar("SELECT is_active FROM companies WHERE id = ?1").bind(company_id).fetch_one(pool).await.unwrap();
    value != 0
}

async fn change_set_count(pool: &SqlitePool, kind: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM change_sets WHERE kind = ?1").bind(kind).fetch_one(pool).await.unwrap()
}

// ---------------------------------------------------------------- gerçek senaryo

/// Brief'in "gerçek senaryo"su: birleştirme sonrası hedefte 2 öğrenci,
/// kaynakta 0, kaynak pasif, kaynağın saati 0, tarihçede `transfer_student`
/// ve `set_company_hours` change set'leri var.
#[tokio::test]
async fn merging_two_companies_moves_students_clears_hours_and_deactivates_source() {
    let (_dir, p) = pair_with_students_and_hours().await;
    let transfers_before = change_set_count(&p.pool, "transfer_student").await;
    let hours_before = change_set_count(&p.pool, "set_company_hours").await;

    let summary = apply_company_merge(&p.pool, p.from, p.into, merge_date(), "mükerrer kayıt birleştirmesi".to_string(), planning_today()).await.unwrap();

    assert_eq!(summary.moved_students, 1);
    assert_eq!(summary.cleared_hours, 8);
    assert!(!summary.ended_coordination);

    assert_eq!(open_placement(&p.pool, p.student_from).await, Some(p.into), "kaynaktaki öğrenci hedefe taşınmalı");
    assert_eq!(open_placement(&p.pool, p.student_into).await, Some(p.into), "hedefteki öğrenci yerinde kalmalı");
    assert_eq!(awarded_hours_on(&p.pool, p.from, ymd(2026, 9, 15)).await, Some(0), "kaynağın saati 0'a çekilmeli");
    assert!(!is_active(&p.pool, p.from).await, "kaynak pasifleşmeli, silinmemeli");
    assert!(is_active(&p.pool, p.into).await, "hedef aktif kalmalı");

    assert_eq!(change_set_count(&p.pool, "transfer_student").await, transfers_before + 1);
    assert_eq!(change_set_count(&p.pool, "set_company_hours").await, hours_before + 1);
}

// ---------------------------------------------------------------- koordinasyon

#[tokio::test]
async fn merging_a_company_with_a_coordinator_ends_the_coordination() {
    let (_dir, p) = pair_with_students_and_hours().await;
    let teacher = add_teacher(&p.pool, "Deniz", planning_today()).await;
    expect_committed(commit(&p.pool, request(None, ChangeCommand::AssignCoordinators { rows: vec![coordinator_row(p.from, teacher)] }), planning_today()).await);
    let ended_before = change_set_count(&p.pool, "end_coordination").await;

    let summary = apply_company_merge(&p.pool, p.from, p.into, merge_date(), "test".to_string(), planning_today()).await.unwrap();

    assert!(summary.ended_coordination);
    assert_eq!(change_set_count(&p.pool, "end_coordination").await, ended_before + 1);
}

#[tokio::test]
async fn merging_a_company_without_a_coordinator_does_not_end_coordination() {
    let (_dir, p) = pair_with_students_and_hours().await;
    let ended_before = change_set_count(&p.pool, "end_coordination").await;

    let summary = apply_company_merge(&p.pool, p.from, p.into, merge_date(), "test".to_string(), planning_today()).await.unwrap();

    assert!(!summary.ended_coordination);
    assert_eq!(change_set_count(&p.pool, "end_coordination").await, ended_before, "koordinatörü olmayan işletme için olay üretilmemeli");
}

// ---------------------------------------------------------------- atomiklik

/// Kapı bir adımı reddederse (dönem başlamış, tarih verilmemiş) HİÇBİR ŞEY
/// yazılmaz: kaynak hâlâ aktif, öğrenci taşınmamış. İçe aktarmadaki
/// "atla ve devam et" davranışından BİLİNÇLİ farkı budur.
#[tokio::test]
async fn rejected_step_writes_nothing_and_keeps_the_source_active() {
    let (_dir, p) = pair_with_students_and_hours().await;
    let before = table_counts(&p.pool).await;

    let result = apply_company_merge(&p.pool, p.from, p.into, None, "test".to_string(), november_today()).await;

    assert!(matches!(result, Err(AppError::Validation(_))), "Validation beklenirdi: {result:?}");
    assert_eq!(table_counts(&p.pool).await, before, "hiçbir tablo değişmemeli");
    assert_eq!(open_placement(&p.pool, p.student_from).await, Some(p.from), "öğrenci taşınmamış olmalı");
    assert!(is_active(&p.pool, p.from).await, "kaynak hâlâ aktif olmalı");
}

// ---------------------------------------------------------------- sınırda doğrulama

#[tokio::test]
async fn merging_a_company_into_itself_is_a_validation_error() {
    let (_dir, p) = pair_with_students_and_hours().await;
    let result = apply_company_merge(&p.pool, p.from, p.from, None, "test".to_string(), planning_today()).await;
    assert!(matches!(result, Err(AppError::Validation(_))));
}

#[tokio::test]
async fn merging_a_missing_company_is_a_validation_error() {
    let (_dir, p) = pair_with_students_and_hours().await;

    let missing_source = apply_company_merge(&p.pool, 999, p.into, None, "test".to_string(), planning_today()).await;
    assert!(matches!(missing_source, Err(AppError::Validation(_))));

    let missing_target = apply_company_merge(&p.pool, p.from, 999, None, "test".to_string(), planning_today()).await;
    assert!(matches!(missing_target, Err(AppError::Validation(_))));
}

#[tokio::test]
async fn merging_into_an_inactive_target_is_a_validation_error() {
    let (_dir, p) = pair_with_students_and_hours().await;
    let mut conn = p.pool.acquire().await.unwrap();
    crate::db::companies::set_active_in(&mut conn, p.into, false).await.unwrap();
    drop(conn);

    let result = apply_company_merge(&p.pool, p.from, p.into, None, "test".to_string(), planning_today()).await;
    assert!(matches!(result, Err(AppError::Validation(_))));
}

/// İkinci birleştirme (kaynak zaten pasif) anlamlı bir hata vermeli;
/// sessizce "hiçbir şey taşınmadı, başarılı" dönmemeli.
#[tokio::test]
async fn merging_the_same_pair_twice_fails_clearly_the_second_time() {
    let (_dir, p) = pair_with_students_and_hours().await;
    apply_company_merge(&p.pool, p.from, p.into, merge_date(), "test".to_string(), planning_today()).await.unwrap();

    let result = apply_company_merge(&p.pool, p.from, p.into, None, "test".to_string(), planning_today()).await;

    let err = result.expect_err("ikinci birleştirme başarısız olmalı");
    assert!(matches!(err, AppError::Validation(_)));
    assert!(err.to_string().contains("pasif"), "hata kaynağın zaten pasif olduğunu söylemeli: {err}");
}

// ---------------------------------------------------------------- önizleme

#[tokio::test]
async fn preview_does_not_write_anything() {
    let (_dir, p) = pair_with_students_and_hours().await;
    let before = table_counts(&p.pool).await;

    let preview = preview_company_merge(&p.pool, p.from, p.into).await.unwrap();

    assert_eq!(preview.students.len(), 1);
    assert_eq!(preview.students[0].student_id, p.student_from);
    assert_eq!(preview.awarded_hours_to_clear, 8);
    assert!(!preview.ends_coordination);
    assert_eq!(table_counts(&p.pool).await, before, "önizleme hiçbir tabloyu değiştirmemeli");
}
