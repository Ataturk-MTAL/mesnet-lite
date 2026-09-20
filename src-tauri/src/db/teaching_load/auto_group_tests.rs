//! Otomatik grup sayısı (Norm Kadro Yön. MADDE 22/1-ç, migration 0007)
//! testleri. `teaching_load.rs` 800 satırı aştığı için ayrı dosyada durur;
//! ortak fixture'lar `tests` modülünden gelir.

use super::tests::{a_student, breakdown, row, test_pool, TERM};
use super::*;
use crate::db::teaching_load_test_support::{seed_teacher, ymd};
use crate::domain::models::ChiefType;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

/// Otomatik satır: istemcinin `group_count` değeri anlamsızdır (99 gönderilir).
fn auto_row(grade: &str, branch: &str, weekly: i64) -> TermBranchHoursInput {
    TermBranchHoursInput {
        grade: grade.into(),
        branch: branch.into(),
        weekly_hours: weekly,
        group_count: 99,
        is_group_manual: false,
    }
}

async fn n_students(pool: &SqlitePool, grade: &str, branch: &str, count: usize) {
    for _ in 0..count {
        a_student(pool, grade, branch, TERM).await;
    }
}

/// Gerçek veri: 12/C Elektronik Haberleşme 16 öğrenci ⇒ 1 grup (elle
/// girilmiş 2 değil); 12/D'de iki dal 7 ve 9 öğrenci ⇒ 1'er grup.
/// Ders saati 24: Σ = 24×3 = 72; alan şefi olmayan atölye şefi 6 ile 78.
#[tokio::test]
async fn real_scenario_automatic_groups_give_72_branch_hours_and_78_in_total() {
    let (_dir, pool) = test_pool().await;
    n_students(&pool, "12/C", "Elektronik Haberleşme", 16).await;
    n_students(&pool, "12/D", "Endüstriyel Bakım Onarım", 7).await;
    n_students(&pool, "12/D", "Bilişim Teknolojileri", 9).await;
    replace_for_term(
        &pool,
        TERM,
        &[
            auto_row("12/C", "Elektronik Haberleşme", 24),
            auto_row("12/D", "Endüstriyel Bakım Onarım", 24),
            auto_row("12/D", "Bilişim Teknolojileri", 24),
        ],
    )
    .await
    .unwrap();
    seed_teacher(&pool, "Atolye", ChiefType::WorkshopLab).await;

    let as_of = ymd(2026, 10, 1);
    assert_eq!(pool_breakdown(&pool, TERM, as_of).await.unwrap(), breakdown(72, 6));
    assert_eq!(total_pool_hours(&pool, TERM, as_of).await.unwrap(), 78);
}

/// Otomatik satırda öğrenci eklenince/çıkınca havuz kendiliğinden değişir;
/// satır yeniden kaydedilmez.
#[tokio::test]
async fn automatic_row_follows_the_student_count_without_being_saved_again() {
    let (_dir, pool) = test_pool().await;
    replace_for_term(&pool, TERM, &[auto_row("12/C", "Dal A", 24)]).await.unwrap();
    let as_of = ymd(2026, 10, 1);
    let branch_hours = || async { pool_breakdown(&pool, TERM, as_of).await.unwrap().branch_hours };

    assert_eq!(branch_hours().await, 0, "öğrencisiz otomatik satır havuza katkı vermez");

    n_students(&pool, "12/C", "Dal A", 16).await;
    assert_eq!(branch_hours().await, 24, "16 öğrenci ⇒ 1 grup");

    n_students(&pool, "12/C", "Dal A", 1).await;
    assert_eq!(branch_hours().await, 48, "17 öğrenci ⇒ 2 grup");

    sqlx::query("DELETE FROM students WHERE id IN (SELECT id FROM students LIMIT 10)")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(branch_hours().await, 24, "7 öğrenci ⇒ 1 grup");
}

/// Elle satır saklanan değeri korur; öğrenci sayısı değişse de değişmez.
#[tokio::test]
async fn manual_row_keeps_the_stored_group_count_whatever_the_student_count() {
    let (_dir, pool) = test_pool().await;
    n_students(&pool, "12/C", "Dal A", 16).await;
    replace_for_term(&pool, TERM, &[row("12/C", "Dal A", 24, 3)]).await.unwrap();
    let as_of = ymd(2026, 10, 1);

    assert_eq!(pool_breakdown(&pool, TERM, as_of).await.unwrap().branch_hours, 72);

    n_students(&pool, "12/C", "Dal A", 20).await;
    assert_eq!(pool_breakdown(&pool, TERM, as_of).await.unwrap().branch_hours, 72);
}

/// Otomatik satırda istemcinin `group_count`'u yok sayılır; saklanan değer
/// hesaplanandır (öğrencisiz satırda önbellek en az 1).
#[tokio::test]
async fn automatic_row_ignores_the_client_group_count_and_caches_the_computed_one() {
    let (_dir, pool) = test_pool().await;
    n_students(&pool, "12/C", "Dal A", 20).await;
    replace_for_term(
        &pool,
        TERM,
        &[auto_row("12/C", "Dal A", 24), auto_row("12/D", "Dal B", 24)],
    )
    .await
    .unwrap();

    let rows = list_for_term(&pool, TERM).await.unwrap();
    assert!(!rows[0].is_group_manual && !rows[1].is_group_manual);
    assert_eq!(rows[0].group_count, 2, "20 öğrenci ⇒ 2 grup, istemcinin 99'u değil");
    assert_eq!(rows[1].group_count, 1, "öğrencisiz satırda önbellek max(0, 1)");
}

/// Otomatik satırda 0 grup geçerlidir (öğrencisiz şube); elle satırda ≥ 1.
#[tokio::test]
async fn zero_groups_is_valid_for_an_automatic_row_but_not_for_a_manual_one() {
    let (_dir, pool) = test_pool().await;
    let mut auto_zero = auto_row("12/C", "Dal A", 24);
    auto_zero.group_count = 0;
    replace_for_term(&pool, TERM, &[auto_zero]).await.unwrap();

    let err = replace_for_term(&pool, TERM, &[row("12/C", "Dal A", 24, 0)]).await.unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
    assert_eq!(
        list_for_term(&pool, TERM).await.unwrap().len(),
        1,
        "reddedilen kayıt eski listeyi silmemeli"
    );
}

/// `isGroupManual` göndermeyen istemci otomatik satır sayılır.
#[test]
fn input_without_is_group_manual_defaults_to_automatic() {
    let input: TermBranchHoursInput = serde_json::from_str(
        r#"{"grade":"12/C","branch":"Dal A","weeklyHours":24,"groupCount":2}"#,
    )
    .unwrap();
    assert!(!input.is_group_manual);
}

/// Kopyalama modu da taşır: elle satır yeni dönemde de elle kalır.
#[tokio::test]
async fn copy_term_keeps_the_group_count_mode() {
    let (_dir, pool) = test_pool().await;
    replace_for_term(
        &pool,
        TERM,
        &[row("12/C", "Dal A", 24, 3), auto_row("12/D", "Dal B", 20)],
    )
    .await
    .unwrap();

    assert!(copy_term(&pool, TERM, "2027-2028/1").await.unwrap());

    let copied = list_for_term(&pool, "2027-2028/1").await.unwrap();
    assert_eq!(copied.len(), 2);
    assert!(copied[0].is_group_manual, "12/C elle kalmalı");
    assert_eq!(copied[0].group_count, 3);
    assert!(!copied[1].is_group_manual, "12/D otomatik kalmalı");
}

// Silinen `pool_hours_for_term` testlerinin kuralları, etkin hesap için:

/// Σ (haftalık ders saati × grup sayısı), elle satırlarla. (Eski
/// `pool_hours_is_the_sum_of_weekly_hours_times_group_counts`.)
#[tokio::test]
async fn effective_branch_hours_sums_weekly_hours_times_manual_group_counts() {
    let (_dir, pool) = test_pool().await;
    replace_for_term(
        &pool,
        TERM,
        &[row("12/C", "Dal A", 24, 2), row("12/D", "Dal B", 24, 1)],
    )
    .await
    .unwrap();

    assert_eq!(effective_branch_hours(&pool, TERM).await.unwrap(), 24 * 2 + 24);
}

/// Satırsız dönem 0'dır. (Eski `pool_hours_is_zero_when_term_has_no_rows`.)
#[tokio::test]
async fn effective_branch_hours_is_zero_when_term_has_no_rows() {
    let (_dir, pool) = test_pool().await;
    assert_eq!(effective_branch_hours(&pool, TERM).await.unwrap(), 0);
}

/// Şef saati Σ'ya karışmaz; kırılımda ayrı kalem olarak durur. (Eski
/// `branch_hours_sum_ignores_chiefs`.)
#[tokio::test]
async fn effective_branch_hours_ignores_chiefs_and_the_breakdown_keeps_them_apart() {
    let (_dir, pool) = test_pool().await;
    replace_for_term(&pool, TERM, &[row("12/C", "Dal A", 24, 2)]).await.unwrap();
    seed_teacher(&pool, "Alan", ChiefType::Department).await;

    assert_eq!(effective_branch_hours(&pool, TERM).await.unwrap(), 48);
    assert_eq!(
        pool_breakdown(&pool, TERM, ymd(2026, 10, 1)).await.unwrap(),
        breakdown(48, 10)
    );
}

/// Migration 0007: mevcut satırlar otomatik moda geçer, değerler bozulmaz.
/// Test 0001-0006'yı uygular, eski (elle girilmiş) satırı yazar, sonra tüm
/// göçleri çalıştırır.
#[tokio::test]
async fn migration_0007_switches_existing_rows_to_automatic_and_keeps_their_values() {
    use std::borrow::Cow;

    const LAST_LEGACY_VERSION: i64 = 6;

    let dir = tempfile::tempdir().unwrap();
    let options = SqliteConnectOptions::new()
        .filename(dir.path().join("legacy.db"))
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();

    let mut legacy = sqlx::migrate!("./migrations");
    let legacy_steps: Vec<_> =
        legacy.migrations.iter().filter(|m| m.version <= LAST_LEGACY_VERSION).cloned().collect();
    legacy.migrations = Cow::Owned(legacy_steps);
    legacy.run(&pool).await.unwrap();

    sqlx::query(
        "INSERT INTO term_branch_hours
            (term, grade, branch, weekly_hours, group_count, created_at, updated_at)
         VALUES (?1, '12/C', 'Elektronik Haberleşme', 24, 2, 'x', 'x')",
    )
    .bind(TERM)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    let rows = list_for_term(&pool, TERM).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].is_group_manual, "mevcut satır otomatik moda geçmeli");
    assert_eq!(rows[0].weekly_hours, 24);
    assert_eq!(rows[0].group_count, 2, "saklanan değer bozulmamalı");

    let invalid = sqlx::query("UPDATE term_branch_hours SET is_group_manual = 2").execute(&pool).await;
    assert!(invalid.is_err(), "CHECK yalnız 0 ve 1'e izin vermeli");
}
