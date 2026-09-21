//! Şeflik saatleri havuzu (MADDE 6/4, OÖKY MADDE 88/2-ç) testleri.
//! `teaching_load.rs` 800 satırı aştığı için ayrı dosyada durur; ortak
//! fixture'lar `tests` modülünden gelir (bkz. `auto_group_tests.rs`'in aynı
//! deseni).

use super::tests::{breakdown, row, test_pool, TERM};
use super::*;
use crate::db::teaching_load_test_support::{change_chief_type, deactivate_teacher, seed_teacher, ymd};
use crate::domain::models::ChiefType;

/// Alan şefi 10 + atölye/lab şefi 6 saat, Σ(saat × grup)'a EKLENİR.
#[tokio::test]
async fn pool_adds_department_and_workshop_lab_chief_hours_to_branch_hours() {
    let (_dir, pool) = test_pool().await;
    replace_for_term(&pool, TERM, &[row("12/C", "Dal A", 24, 2), row("12/D", "Dal B", 24, 1)])
        .await
        .unwrap();
    seed_teacher(&pool, "Alan", ChiefType::Department).await;
    seed_teacher(&pool, "Atolye", ChiefType::WorkshopLab).await;
    seed_teacher(&pool, "Siradan", ChiefType::None).await;

    let as_of = ymd(2026, 10, 1);

    assert_eq!(chief_planning_hours(&pool, TERM, as_of).await.unwrap(), 10 + 6);
    assert_eq!(pool_breakdown(&pool, TERM, as_of).await.unwrap(), breakdown(24 * 2 + 24, 16));
    assert_eq!(total_pool_hours(&pool, TERM, as_of).await.unwrap(), 24 * 2 + 24 + 16);
}

#[tokio::test]
async fn pool_is_only_the_branch_sum_when_nobody_is_chief() {
    let (_dir, pool) = test_pool().await;
    replace_for_term(&pool, TERM, &[row("12/C", "Dal A", 24, 2)]).await.unwrap();
    seed_teacher(&pool, "Siradan", ChiefType::None).await;

    let as_of = ymd(2026, 10, 1);
    assert_eq!(chief_planning_hours(&pool, TERM, as_of).await.unwrap(), 0);
    assert_eq!(total_pool_hours(&pool, TERM, as_of).await.unwrap(), 48);
}

#[tokio::test]
async fn inactive_teachers_chief_hours_are_not_counted() {
    let (_dir, pool) = test_pool().await;
    seed_teacher(&pool, "Alan", ChiefType::Department).await;
    let atolye = seed_teacher(&pool, "Atolye", ChiefType::WorkshopLab).await;
    deactivate_teacher(&pool, atolye).await;

    assert_eq!(chief_planning_hours(&pool, TERM, ymd(2026, 10, 1)).await.unwrap(), 10);
}

/// `valid_from <= as_of < valid_to`: aralığın başlangıç günü dahil, bitiş
/// günü hariçtir.
#[tokio::test]
async fn chief_hours_outside_the_validity_range_are_not_counted() {
    let (_dir, pool) = test_pool().await;
    let alan = seed_teacher(&pool, "Alan", ChiefType::Department).await; // 2026-09-01'den
    change_chief_type(&pool, alan, ChiefType::None, ymd(2026, 11, 5)).await;

    let hours = |d| chief_planning_hours(&pool, TERM, d);
    assert_eq!(hours(ymd(2026, 8, 31)).await.unwrap(), 0, "aralıktan önce");
    assert_eq!(hours(ymd(2026, 9, 1)).await.unwrap(), 10, "başlangıç günü dahil");
    assert_eq!(hours(ymd(2026, 11, 4)).await.unwrap(), 10, "bitişten önceki gün");
    assert_eq!(hours(ymd(2026, 11, 5)).await.unwrap(), 0, "bitiş günü hariç");
}

/// Şef değişince iki aralık ardışık olur; her gün doğru aralığı okumalı.
#[tokio::test]
async fn chief_change_gives_the_right_total_on_each_side_of_the_change_day() {
    let (_dir, pool) = test_pool().await;
    let alan = seed_teacher(&pool, "Alan", ChiefType::Department).await;
    change_chief_type(&pool, alan, ChiefType::WorkshopLab, ymd(2026, 11, 5)).await;

    let hours = |d| chief_planning_hours(&pool, TERM, d);
    assert_eq!(hours(ymd(2026, 10, 1)).await.unwrap(), 10);
    assert_eq!(hours(ymd(2026, 11, 5)).await.unwrap(), 6);
    assert_eq!(hours(ymd(2027, 1, 31)).await.unwrap(), 6, "açık aralık (valid_to boş) dönem sonuna dek");
}

#[tokio::test]
async fn chief_hours_are_scoped_to_the_term() {
    let (_dir, pool) = test_pool().await;
    seed_teacher(&pool, "Alan", ChiefType::Department).await;

    assert_eq!(chief_planning_hours(&pool, "2027-2028/1", ymd(2027, 10, 1)).await.unwrap(), 0);
}

#[test]
fn breakdown_total_is_the_sum_of_both_parts() {
    assert_eq!(breakdown(72, 16).total(), 88);
}

// --- `_in` (transaction-içi) varyantları: `db/history_context.rs`'in havuzu
// TEK noktadan okuduğunu kanıtlar; pool ve conn sürümleri AYNI sonucu vermeli.

#[tokio::test]
async fn total_pool_hours_in_matches_the_pool_based_version() {
    let (_dir, pool) = test_pool().await;
    replace_for_term(&pool, TERM, &[row("12/C", "Dal A", 24, 2), row("12/D", "Dal B", 24, 1)])
        .await
        .unwrap();
    seed_teacher(&pool, "Atolye", ChiefType::WorkshopLab).await;
    let as_of = ymd(2026, 10, 1);

    let via_pool = total_pool_hours(&pool, TERM, as_of).await.unwrap();
    let mut conn = pool.acquire().await.unwrap();
    let via_conn = total_pool_hours_in(&mut conn, TERM, as_of).await.unwrap();

    assert_eq!(via_pool, via_conn);
    assert_eq!(via_conn, 24 * 2 + 24 + 6);
}
