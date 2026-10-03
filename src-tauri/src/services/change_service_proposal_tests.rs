//! `ApplyProposal` (Issue #43) ve atama kaynağı: motor önerisi saat
//! değişikliği + atama + atama sonlandırmayı TEK değişiklik kümesinde yazar,
//! tek `Revoke` hepsini geri alır. Yalnız kurgusal veri.

use chrono::NaiveDate;
use sqlx::SqlitePool;

use super::super::change_service_test_support::*;
use super::{assigned_companies, cell, company_with_hours, expect_rejected};
use crate::db::assignments;
use crate::db::read_at::ReadAt;
use crate::domain::history::decide::{ChangeCommand, CoordinatorRow};
use crate::domain::history::events::AssignmentSource;
use crate::domain::history::rejection::RejectionCode;

fn apply(hours: Vec<crate::domain::history::decide::CompanyHoursRow>, assign: Vec<CoordinatorRow>, release: Vec<i64>) -> ChangeCommand {
    ChangeCommand::ApplyProposal { hours, assign, release }
}

fn term_start() -> NaiveDate {
    ymd(2026, 9, 1)
}

/// Sonlandırma/saat değişikliği dönem başladıktan sonra, atamalardan SONRAKİ
/// bir tarihte istenir: `end_coordination` bir önceki durumu (d⁻) arar, aynı
/// gün başlayan atama henüz yoktur (bkz. `assignment_commands_tests.rs::
/// unassign_company_ends_an_existing_open_assignment`).
fn proposal_date() -> NaiveDate {
    ymd(2026, 11, 5)
}

async fn source_of(pool: &SqlitePool, company_id: i64) -> Option<String> {
    let row = assignments::get_for_company(pool, company_id, TERM).await.unwrap();
    row.map(|a| a.source)
}

async fn teacher_of(pool: &SqlitePool, company_id: i64) -> Option<i64> {
    assignments::get_for_company(pool, company_id, TERM).await.unwrap().map(|a| a.teacher_id)
}

async fn assign_manually(pool: &SqlitePool, row: CoordinatorRow) {
    let command = ChangeCommand::AssignCoordinators { rows: vec![row] };
    expect_committed(commit(pool, request(None, command), planning_today()).await);
}

async fn set_hours(pool: &SqlitePool, company_id: i64, hours: i64) {
    let command = ChangeCommand::SetCompanyHours { rows: vec![hours_row(company_id, hours)] };
    expect_committed(commit(pool, request(None, command), planning_today()).await);
}

/// Elle atama `manual`, `ApplyProposal` ile gelen `proposal` yazılır; satırın
/// kendi `source` alanı `Manual` gelse bile komut `Proposal` yazar.
#[tokio::test]
async fn source_is_manual_for_hand_assignment_and_proposal_for_apply_proposal() {
    let w = world().await;
    let teacher = add_teacher(&w.pool, "Ece", planning_today()).await;
    let by_hand = company_with_hours(&w, "Elle").await;
    let by_engine = company_with_hours(&w, "Motor").await;
    assign_manually(&w.pool, cell(by_hand, teacher, 2, 1)).await;

    let mut engine_row = cell(by_engine, teacher, 3, 1);
    engine_row.source = AssignmentSource::Manual;
    expect_committed(commit(&w.pool, request(None, apply(vec![], vec![engine_row], vec![])), planning_today()).await);

    assert_eq!(source_of(&w.pool, by_hand).await.as_deref(), Some("manual"));
    assert_eq!(source_of(&w.pool, by_engine).await.as_deref(), Some("proposal"));
}

/// Revoke projeksiyonu olaylardan yeniden kurar: öneri kümesi geri alınınca
/// işletme ESKİ elle atamasına (öğretmen VE kaynak) döner.
#[tokio::test]
async fn revoking_a_proposal_restores_the_previous_manual_source() {
    let w = world().await;
    let first = add_teacher(&w.pool, "Ece", planning_today()).await;
    let second = add_teacher(&w.pool, "Ford", planning_today()).await;
    let company = company_with_hours(&w, "Tasinan").await;
    assign_manually(&w.pool, cell(company, first, 2, 1)).await;

    let command = apply(vec![], vec![cell(company, second, 4, 1)], vec![]);
    let (set_id, _) = expect_committed(commit(&w.pool, request(None, command), planning_today()).await);
    assert_eq!((teacher_of(&w.pool, company).await, source_of(&w.pool, company).await.as_deref()), (Some(second), Some("proposal")));

    expect_committed(commit(&w.pool, request(None, ChangeCommand::Revoke { change_set_id: set_id }), planning_today()).await);

    assert_eq!((teacher_of(&w.pool, company).await, source_of(&w.pool, company).await.as_deref()), (Some(first), Some("manual")));
}

/// Elle yeniden atama, öneri kaynaklı atamanın kaynağını `manual` yapar.
#[tokio::test]
async fn a_hand_assignment_over_a_proposal_becomes_manual() {
    let w = world().await;
    let teacher = add_teacher(&w.pool, "Ece", planning_today()).await;
    let company = company_with_hours(&w, "Donen").await;
    expect_committed(commit(&w.pool, request(None, apply(vec![], vec![cell(company, teacher, 2, 1)], vec![])), planning_today()).await);
    assert_eq!(source_of(&w.pool, company).await.as_deref(), Some("proposal"));

    assign_manually(&w.pool, cell(company, teacher, 2, 4)).await;

    assert_eq!(source_of(&w.pool, company).await.as_deref(), Some("manual"));
}

/// Kullanıcı kuralı: saat küçültme önce uygulanır. X 6 saatle öğretmenin
/// 1-6. saatlerinde; Y'nin 3. saate paketlenmesi tek başına çakışmadır.
/// Aynı kümede X 2 saate düşerken Y yanına paketlenebilmelidir — saatler
/// atamalardan ÖNCE yazıldığı için.
#[tokio::test]
async fn shrinking_hours_and_packing_next_to_it_is_accepted_in_one_set() {
    let w = world().await;
    let teacher = add_teacher(&w.pool, "Ece", planning_today()).await;
    let shrunk = company_with_hours(&w, "Kuculen").await;
    let packed = company_with_hours(&w, "Paketlenen").await;
    set_hours(&w.pool, shrunk, 6).await;
    assign_manually(&w.pool, cell(shrunk, teacher, 2, 1)).await;

    let alone = ChangeCommand::AssignCoordinators { rows: vec![cell(packed, teacher, 2, 3)] };
    let (code, _) = expect_rejected(commit(&w.pool, request(None, alone), planning_today()).await);
    assert_eq!(code, RejectionCode::BlockOverlap, "ön koşul: saat küçülmeden paketleme çakışır");

    let command = apply(vec![hours_row(shrunk, 2)], vec![cell(packed, teacher, 2, 3)], vec![]);
    expect_committed(commit(&w.pool, request(None, command), planning_today()).await);

    assert_eq!(awarded_hours_on(&w.pool, shrunk, term_start()).await, Some(2));
    assert_eq!(teacher_of(&w.pool, packed).await, Some(teacher));
}

/// `release` işletmenin atamasını sonlandırır; diğerlerine dokunmaz.
#[tokio::test]
async fn release_ends_the_assignment() {
    let w = world().await;
    let teacher = add_teacher(&w.pool, "Ece", planning_today()).await;
    let released = company_with_hours(&w, "Birakilan").await;
    let kept = company_with_hours(&w, "Kalan").await;
    assign_manually(&w.pool, cell(released, teacher, 2, 1)).await;
    assign_manually(&w.pool, cell(kept, teacher, 3, 1)).await;

    expect_committed(commit(&w.pool, request(Some(proposal_date()), apply(vec![], vec![], vec![released])), november_today()).await);

    let ids = assigned_companies(&w.pool).await;
    assert!(!ids.contains(&released) && ids.contains(&kept));
}

/// Aynı işletme hem `assign` hem `release` içinde olamaz; hiçbir şey yazılmaz.
#[tokio::test]
async fn the_same_company_in_assign_and_release_is_rejected() {
    let w = world().await;
    let teacher = add_teacher(&w.pool, "Ece", planning_today()).await;
    let company = company_with_hours(&w, "Cakisan").await;
    assign_manually(&w.pool, cell(company, teacher, 2, 1)).await;
    let before = table_counts(&w.pool).await;

    let command = apply(vec![], vec![cell(company, teacher, 3, 1)], vec![company]);
    let (code, _) = expect_rejected(commit(&w.pool, request(None, command), planning_today()).await);

    assert_eq!(code, RejectionCode::InvalidRequest);
    assert_eq!(table_counts(&w.pool).await, before);
}

/// Aynı işletmeyi iki kez bırakmak belirsizdir; kapı bunu açıkça reddeder.
#[tokio::test]
async fn releasing_the_same_company_twice_is_rejected() {
    let w = world().await;
    let (code, _) = expect_rejected(commit(&w.pool, request(None, apply(vec![], vec![], vec![w.company_a, w.company_a])), planning_today()).await);
    assert_eq!(code, RejectionCode::InvalidRequest);
}

/// Üç liste de boşsa yazılacak bir şey yoktur.
#[tokio::test]
async fn an_empty_proposal_is_rejected() {
    let w = world().await;
    let (code, _) = expect_rejected(commit(&w.pool, request(None, apply(vec![], vec![], vec![])), planning_today()).await);
    assert_eq!(code, RejectionCode::InvalidRequest);
}

/// Hep ya da hiç: geçerli saat satırı, bir sonraki parçanın (çakışan atama)
/// reddi yüzünden de yazılmaz.
#[tokio::test]
async fn a_rejected_part_writes_nothing_even_when_the_hours_part_is_valid() {
    let w = world().await;
    let teacher = add_teacher(&w.pool, "Ece", planning_today()).await;
    let first = company_with_hours(&w, "Birinci").await;
    let second = company_with_hours(&w, "Ikinci").await;
    let third = company_with_hours(&w, "Ucuncu").await;
    let before = table_counts(&w.pool).await;

    let command = apply(
        vec![hours_row(third, 2)],
        vec![cell(first, teacher, 2, 1), cell(second, teacher, 2, 1)],
        vec![],
    );
    let (code, _) = expect_rejected(commit(&w.pool, request(None, command), planning_today()).await);

    assert_eq!(code, RejectionCode::BlockOverlap);
    assert_eq!(table_counts(&w.pool).await, before, "hiçbir tabloya yazılmamalı");
    assert_eq!(awarded_hours_on(&w.pool, third, term_start()).await, Some(3));
}

/// Tek `Revoke` kümenin üç parçasını da geri alır: saat, yeni atama ve
/// sonlandırılan atama.
#[tokio::test]
async fn one_revoke_undoes_hours_assign_and_release_together() {
    let w = world().await;
    let teacher = add_teacher(&w.pool, "Ece", planning_today()).await;
    let shrunk = company_with_hours(&w, "Kuculen").await;
    let packed = company_with_hours(&w, "Paketlenen").await;
    let released = company_with_hours(&w, "Birakilan").await;
    set_hours(&w.pool, shrunk, 6).await;
    assign_manually(&w.pool, cell(shrunk, teacher, 2, 1)).await;
    assign_manually(&w.pool, cell(released, teacher, 4, 1)).await;
    let command = apply(vec![hours_row(shrunk, 2)], vec![cell(packed, teacher, 2, 3)], vec![released]);
    let (set_id, _) = expect_committed(commit(&w.pool, request(Some(proposal_date()), command), november_today()).await);
    let events_in_set: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_events WHERE change_set_id = ?1").bind(set_id).fetch_one(&w.pool).await.unwrap();
    assert_eq!(events_in_set, 3, "ön koşul: saat + atama + sonlandırma TEK kümede");

    assert_eq!(awarded_hours_on(&w.pool, shrunk, proposal_date()).await, Some(2), "ön koşul: saat küçüldü");

    expect_committed(commit(&w.pool, request(Some(proposal_date()), ChangeCommand::Revoke { change_set_id: set_id }), november_today()).await);

    assert_eq!(awarded_hours_on(&w.pool, shrunk, proposal_date()).await, Some(6), "saat eski değerine döner");
    assert_eq!(teacher_of(&w.pool, packed).await, None, "yeni atama kalkar");
    assert_eq!(teacher_of(&w.pool, released).await, Some(teacher), "sonlandırılan atama geri gelir");
    let listed = assignments::list(&w.pool, TERM, &ReadAt::Latest).await.unwrap();
    assert!(listed.iter().all(|a| a.source == "manual"), "geri yüklenen atamalar elle kaynaklıdır");
}

/// Sınırda doğrulama yeni komutu da kapsar: ızgara dışı saat günlüğe
/// girmeden reddedilir (bkz. `change_input::validate_command`).
#[tokio::test]
async fn out_of_grid_visit_hour_in_a_proposal_is_a_validation_error() {
    let w = world().await;
    let teacher = add_teacher(&w.pool, "Ece", planning_today()).await;
    let company = company_with_hours(&w, "Disarida").await;
    let before = table_counts(&w.pool).await;

    let command = apply(vec![], vec![cell(company, teacher, 2, 99)], vec![]);
    let result = super::execute_change(&w.pool, request(None, command), super::ChangeMode::Commit { expected_high_water: None }, planning_today()).await;

    assert!(matches!(result, Err(crate::error::AppError::Validation(_))), "Validation beklenirdi: {result:?}");
    assert_eq!(table_counts(&w.pool).await, before);
}

/// Pano `assignmentSource`'u öneri kaynaklı atama için `proposal` döner.
#[tokio::test]
async fn proposal_assignment_is_listed_with_source_proposal() {
    let w = world().await;
    let teacher = add_teacher(&w.pool, "Ece", planning_today()).await;
    let company = company_with_hours(&w, "Onerilen").await;
    expect_committed(commit(&w.pool, request(None, apply(vec![], vec![cell(company, teacher, 2, 1)], vec![])), planning_today()).await);

    let listed = assignments::list(&w.pool, TERM, &ReadAt::Latest).await.unwrap();

    let sources: Vec<(i64, &str)> = listed.iter().map(|a| (a.company_id, a.source.as_str())).collect();
    assert!(sources.contains(&(company, "proposal")) && sources.contains(&(w.company_a, "manual")), "{sources:?}");
}

/// Planlamada her atama dönem başı tarihlidir; aynı tarihli `release` onu
/// hiç yürürlüğe girmemiş sayar: açık satır kalmaz. Revoke atamayı geri getirir.
#[tokio::test]
async fn release_in_planning_removes_a_same_day_assignment_and_revoke_restores_it() {
    let w = world().await;
    let teacher = add_teacher(&w.pool, "Ece", planning_today()).await;
    let company = company_with_hours(&w, "Ayni-Gun").await;
    assign_manually(&w.pool, cell(company, teacher, 2, 1)).await;

    let (set_id, _) = expect_committed(commit(&w.pool, request(None, apply(vec![], vec![], vec![company])), planning_today()).await);

    assert_eq!(teacher_of(&w.pool, company).await, None);
    let periods: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM coordination_periods WHERE company_id = ?1").bind(company).fetch_one(&w.pool).await.unwrap();
    assert_eq!(periods, 0, "boş dönem satırı kalmamalı");

    expect_committed(commit(&w.pool, request(None, ChangeCommand::Revoke { change_set_id: set_id }), planning_today()).await);
    assert_eq!(teacher_of(&w.pool, company).await, Some(teacher));
}

/// Saat büyüyüp işletme başka hücreye taşınırken ESKİ hücre + yeni saat
/// B'nin bloğuna taşardı; eski blok yürürlükten kalktığı için kabul edilir.
#[tokio::test]
async fn growing_and_moving_in_one_proposal_ignores_the_old_cell() {
    let w = world().await;
    let teacher = add_teacher(&w.pool, "Ece", planning_today()).await;
    let moved = company_with_hours(&w, "Tasinan").await;
    let neighbour = company_with_hours(&w, "Komsu").await;
    set_hours(&w.pool, moved, 2).await;
    assign_manually(&w.pool, cell(moved, teacher, 2, 1)).await;
    assign_manually(&w.pool, cell(neighbour, teacher, 2, 5)).await;

    let grow_alone = ChangeCommand::SetCompanyHours { rows: vec![hours_row(moved, 6)] };
    let (code, _) = expect_rejected(commit(&w.pool, request(None, grow_alone), planning_today()).await);
    assert_eq!(code, RejectionCode::BlockOverlap, "ön koşul: yerinde büyümek komşuya taşar");

    let command = apply(vec![hours_row(moved, 6)], vec![cell(moved, teacher, 3, 1)], vec![]);
    expect_committed(commit(&w.pool, request(None, command), planning_today()).await);
    assert_eq!(teacher_of(&w.pool, moved).await, Some(teacher));
}

/// Başka bir işletme, taşınan işletmenin boşalttığı hücreye doğru büyürse de
/// sahte çakışma yok; ama hâlâ yerinde duran bir komşuya taşarsa ret sürer.
#[tokio::test]
async fn growing_into_a_vacated_cell_is_accepted_but_growing_into_a_staying_neighbour_is_not() {
    let w = world().await;
    let teacher = add_teacher(&w.pool, "Ece", planning_today()).await;
    let leaving = company_with_hours(&w, "Giden").await;
    let growing = company_with_hours(&w, "Buyuyen").await;
    let staying = company_with_hours(&w, "Duran").await;
    assign_manually(&w.pool, cell(growing, teacher, 2, 1)).await;
    assign_manually(&w.pool, cell(leaving, teacher, 2, 4)).await;
    assign_manually(&w.pool, cell(staying, teacher, 2, 8)).await;

    let into_staying = apply(vec![hours_row(growing, 8)], vec![cell(leaving, teacher, 4, 1)], vec![]);
    let (code, _) = expect_rejected(commit(&w.pool, request(None, into_staying), planning_today()).await);
    assert_eq!(code, RejectionCode::BlockOverlap, "yerinde duran komşuya taşma reddedilir");

    let into_vacated = apply(vec![hours_row(growing, 6)], vec![cell(leaving, teacher, 4, 1)], vec![]);
    expect_committed(commit(&w.pool, request(None, into_vacated), planning_today()).await);
}
