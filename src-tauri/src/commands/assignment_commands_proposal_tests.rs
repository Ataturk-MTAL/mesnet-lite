//! `propose_assignments` (Issue #43, birim 3): komut girdisini kurar, yeni
//! motoru çağırır. Yalnız kurgusal veri; her sahne `init_pool` tohumlu
//! dönemde (2026-09-01 – 2027-01-31) kurulur.

use super::proposal_input::{ceiling_for, validate_input, ProposalInput};
use super::*;
use crate::db::init_pool;
use crate::db::teaching_load_test_support::{seed_teacher, ymd, TERM};
use crate::domain::history::decide::{ChangeCommand, ChangeRequest, CompanyHoursRow, CoordinatorRow, NewStudentInput};
use crate::domain::models::{ChiefType, NewCompany};
use crate::domain::optimizer::{CompanyInput, CurrentPlacement, PlacementSource, ProposalMode, TeacherInput};
use crate::error::AppError;
use sqlx::SqlitePool;

/// Dönem başlamadan önce: "Baştan dağıt" serbest.
fn planning_today() -> NaiveDate {
    ymd(2026, 8, 15)
}

/// Dönem başladı: "Baştan dağıt" reddedilir.
fn running_today() -> NaiveDate {
    ymd(2026, 11, 10)
}

async fn test_state() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
    class_days::replace_for_grade(&pool, "12/C", TERM, &[1, 2, 3, 4, 5]).await.unwrap();
    (dir, AppState { pool })
}

async fn commit(pool: &SqlitePool, command: ChangeCommand) {
    let request = ChangeRequest {
        term: TERM.to_string(),
        effective_date: None,
        document_date: None,
        reason: "test".to_string(),
        command,
    };
    commit_legacy_change(pool, request, planning_today()).await.unwrap();
}

/// Haftanın her gün, 1–9. saatleri boş olan öğretmen.
async fn free_teacher(pool: &SqlitePool, first_name: &str) -> i64 {
    let teacher_id = seed_teacher(pool, first_name, ChiefType::None).await;
    let slots = (1..=5).flat_map(|day| (1..=9).map(move |hour| Slot::new(day, hour))).collect();
    commit(pool, ChangeCommand::SetTeacherSchedule { teacher_id, slots }).await;
    teacher_id
}

/// `student_count` öğrencili işletme. `one_way_km` gidiş mesafesi; saat
/// satırı yalnız `hours` verilmişse yazılır (kapı tavanı kendisi hesaplar).
async fn company(pool: &SqlitePool, name: &str, one_way_km: Option<f64>, student_count: usize, hours: Option<i64>) -> i64 {
    let new = NewCompany {
        name: name.into(),
        contact_first_name: String::new(),
        contact_last_name: String::new(),
        phone: String::new(),
        email: String::new(),
        address_text: "Örnek adres".into(),
        latitude: None,
        longitude: None,
        one_way_distance_km: one_way_km,
        district: String::new(),
        neighborhood: String::new(),
        notes: String::new(),
    };
    let company_id = companies::create(pool, &new).await.unwrap().id;
    for index in 0..student_count {
        let student = NewStudentInput {
            first_name: format!("{name}-{index}"),
            last_name: "Öğrenci".into(),
            student_no: None,
            grade: "12/C".into(),
            branch: "Elektronik Haberleşme".into(),
            submitted_at: None,
        };
        commit(pool, ChangeCommand::CreateStudent { student, company_id: Some(company_id) }).await;
    }
    if let Some(awarded_hours) = hours {
        let row = CompanyHoursRow { company_id, awarded_hours, is_honorary: false, is_locked: false, notes: String::new() };
        commit(pool, ChangeCommand::SetCompanyHours { rows: vec![row] }).await;
    }
    company_id
}

fn cell(company_id: i64, teacher_id: i64, visit_day: i64, visit_hour: i64) -> CoordinatorRow {
    CoordinatorRow { company_id, teacher_id, visit_day, visit_hour, is_forced: false, force_reason: None, source: Default::default() }
}

async fn assign_by_hand(pool: &SqlitePool, row: CoordinatorRow) {
    commit(pool, ChangeCommand::AssignCoordinators { rows: vec![row] }).await;
}

async fn assign_by_proposal(pool: &SqlitePool, row: CoordinatorRow) {
    commit(pool, ChangeCommand::ApplyProposal { hours: vec![], assign: vec![row], release: vec![] }).await;
}

/// Her biri 1 öğrencili, 6 km gidiş-dönüşlü (tavan 8), 4 saatlik üç işletme
/// A öğretmeninde yığılı: ikisi elle, biri öneri kaynaklı. B öğretmeni boş.
struct Crowded {
    state: AppState,
    _dir: tempfile::TempDir,
    teacher_a: i64,
    teacher_b: i64,
    manual_1: i64,
    manual_2: i64,
    proposed: i64,
}

async fn crowded_world() -> Crowded {
    let (dir, state) = test_state().await;
    let pool = &state.pool;
    let teacher_a = free_teacher(pool, "Ada").await;
    let teacher_b = free_teacher(pool, "Bora").await;
    let manual_1 = company(pool, "Elle Bir", Some(3.0), 1, Some(4)).await;
    let manual_2 = company(pool, "Elle Iki", Some(3.0), 1, Some(4)).await;
    let proposed = company(pool, "Oneri", Some(3.0), 1, Some(4)).await;
    assign_by_hand(pool, cell(manual_1, teacher_a, 1, 1)).await;
    assign_by_hand(pool, cell(manual_2, teacher_a, 2, 1)).await;
    assign_by_proposal(pool, cell(proposed, teacher_a, 3, 1)).await;
    Crowded { state, _dir: dir, teacher_a, teacher_b, manual_1, manual_2, proposed }
}

async fn load_input(state: &AppState) -> ProposalInput {
    let board = load_board(state, &ReadAt::Latest).await.unwrap();
    ProposalInput::load(state, &board).await.unwrap()
}

fn input_company(input: &ProposalInput, id: i64) -> &CompanyInput {
    input.companies.iter().find(|c| c.id == id).unwrap()
}

fn input_teacher(input: &ProposalInput, id: i64) -> &TeacherInput {
    input.teachers.iter().find(|t| t.id == id).unwrap()
}

// --- Komut: kip ve evre ---

/// fillGaps yalnız atanmamışı yerleştirir; öneri kaynaklı olsa bile mevcut
/// atamaya dokunmaz (dengesiz olsa da — redistribute'un aksine).
#[tokio::test]
async fn fill_gaps_places_unassigned_companies_and_leaves_existing_assignments_alone() {
    let w = crowded_world().await;
    let open = company(&w.state.pool, "Bos", Some(3.0), 1, Some(4)).await;

    let proposal = propose_assignments_for(&w.state, "fillGaps", planning_today()).await.unwrap();

    assert_eq!(proposal.mode, ProposalMode::FillGaps);
    let proposed_ids: Vec<i64> = proposal.assignments.iter().map(|a| a.company_id).collect();
    assert_eq!(proposed_ids, vec![open], "yalnız atanmamış işletme önerilmeli");
    assert!(proposal.released.is_empty());
    let kept_ids: BTreeSet<i64> = proposal.kept.iter().map(|k| k.company_id).collect();
    assert_eq!(kept_ids, BTreeSet::from([w.manual_1, w.manual_2, w.proposed]));
    assert!(proposal.kept.iter().all(|k| k.teacher_id == w.teacher_a), "mevcut atamalar yerinde");
}

/// redistribute planlamada: elle atamalar yerinde kalır, öneri kaynaklı
/// atama (yük A'da yığılı, B boşken) B'ye taşınabilir.
#[tokio::test]
async fn redistribute_in_planning_keeps_manual_assignments_and_may_move_a_proposed_one() {
    let w = crowded_world().await;

    let proposal = propose_assignments_for(&w.state, "redistribute", planning_today()).await.unwrap();

    assert_eq!(proposal.mode, ProposalMode::Redistribute);
    assert_eq!(proposal.assignments.len(), 1, "yalnız öneri kaynaklı atama değişmeli: {:?}", proposal.assignments);
    let moved = &proposal.assignments[0];
    assert_eq!((moved.company_id, moved.teacher_id), (w.proposed, w.teacher_b));
    assert!(moved.previous.is_some(), "taşınan işletmenin önceki yeri raporlanmalı");
    let kept_ids: BTreeSet<i64> = proposal.kept.iter().map(|k| k.company_id).collect();
    assert!(kept_ids.contains(&w.manual_1) && kept_ids.contains(&w.manual_2));
    assert!(proposal.kept.iter().all(|k| k.teacher_id == w.teacher_a), "elle atamalar A'da kalmalı");
    assert!(proposal.released.is_empty());
}

/// Dönem başladıysa yürürlükteki dağılım ek ders puantajına işlemiştir;
/// baştan kurmak reddedilir, yalnız boşlar doldurulur.
#[tokio::test]
async fn redistribute_is_rejected_once_the_term_has_started_but_fill_gaps_is_allowed() {
    let w = crowded_world().await;

    let err = propose_assignments_for(&w.state, "redistribute", running_today()).await.unwrap_err();
    match err {
        AppError::Validation(message) => assert!(message.contains("Dönem başladı"), "mesaj dönemi anmalı: {message}"),
        other => panic!("Validation beklenirdi: {other:?}"),
    }

    assert!(propose_assignments_for(&w.state, "fillGaps", running_today()).await.is_ok());
}

#[tokio::test]
async fn an_unknown_mode_is_rejected() {
    let (_dir, state) = test_state().await;
    for bad in ["", "bogus", "FillGaps", "fill_gaps", "REDISTRIBUTE"] {
        let err = propose_assignments_for(&state, bad, planning_today()).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)), "{bad:?} reddedilmeliydi");
    }
}

// --- Girdi: tavan ---

#[test]
fn ceiling_is_the_smaller_of_snapshot_and_live_cap() {
    assert_eq!(ceiling_for(Some(99), Some(2)), 2, "snapshot büyükken canlı sınır geçerli");
    assert_eq!(ceiling_for(Some(3), Some(8)), 3, "canlı büyükken snapshot geçerli");
    assert_eq!(ceiling_for(Some(5), None), 5, "canlı tavan bilinmiyorsa snapshot");
    assert_eq!(ceiling_for(None, Some(7)), 7, "saat kaydı yoksa canlı tavan");
    assert_eq!(ceiling_for(None, None), 0, "ikisi de yoksa 0");
}

/// Uçtan uca: saat kaydındaki snapshot canlı kuraldan büyükse (kural sonradan
/// sıkılaştı / öğrenci azaldı) motora CANLI tavan gider.
#[tokio::test]
async fn input_ceiling_follows_the_live_rule_when_the_snapshot_is_larger() {
    let (_dir, state) = test_state().await;
    // 1 öğrenci, gidiş-dönüş 0,5 km → 0–1 km bandı, 1–2 öğrenci: tavan 2.
    let tight = company(&state.pool, "Yakin", Some(0.25), 1, Some(2)).await;
    sqlx::query("UPDATE company_hour_periods SET max_hours_snapshot = 99 WHERE company_id = ?1")
        .bind(tight)
        .execute(&state.pool)
        .await
        .unwrap();

    let input = load_input(&state).await;

    assert_eq!(input_company(&input, tight).max_hours, 2);
    assert_eq!(input_company(&input, tight).awarded_hours, 2);
}

#[tokio::test]
async fn input_ceiling_is_the_snapshot_when_it_is_smaller_than_the_live_cap() {
    let (_dir, state) = test_state().await;
    // Gidiş-dönüş 6 km, 1 öğrenci: canlı tavan 8.
    let id = company(&state.pool, "Uzak", Some(3.0), 1, Some(5)).await;
    sqlx::query("UPDATE company_hour_periods SET max_hours_snapshot = 3 WHERE company_id = ?1")
        .bind(id)
        .execute(&state.pool)
        .await
        .unwrap();

    let input = load_input(&state).await;

    assert_eq!(input_company(&input, id).max_hours, 3);
}

/// Saat kaydı hiç yoksa tavan canlı kuraldan; kural da yoksa (mesafe yok) 0.
#[tokio::test]
async fn input_ceiling_without_an_hours_row_uses_the_live_cap_or_zero() {
    let (_dir, state) = test_state().await;
    let measured = company(&state.pool, "Olculu", Some(3.0), 1, None).await;
    let unmeasured = company(&state.pool, "Olcusuz", None, 1, None).await;

    let input = load_input(&state).await;

    assert_eq!(input_company(&input, measured).max_hours, 8);
    assert_eq!(input_company(&input, unmeasured).max_hours, 0);
    assert_eq!(input_company(&input, measured).awarded_hours, 0);
}

// --- Girdi: işletmeler ---

/// Atanmamış işletmeler de girer; `current` kaynağı tarihçedeki kaynaktan eşlenir.
#[tokio::test]
async fn input_carries_every_active_company_with_the_mapped_placement_source() {
    let w = crowded_world().await;
    let open = company(&w.state.pool, "Bos", Some(3.0), 1, Some(4)).await;

    let input = load_input(&w.state).await;

    assert_eq!(input.companies.len(), 4);
    assert!(input_company(&input, open).current.is_none());
    let source_of = |id: i64| input_company(&input, id).current.as_ref().map(|c: &CurrentPlacement| (c.teacher_id, c.visit_day, c.visit_hour, c.is_forced, c.source));
    assert_eq!(source_of(w.manual_1), Some((w.teacher_a, 1, 1, false, PlacementSource::Manual)));
    assert_eq!(source_of(w.proposed), Some((w.teacher_a, 3, 1, false, PlacementSource::Proposal)));
}

/// Grup anahtarı/adı panonun hesabıdır; ikinci kez türetilmez.
#[tokio::test]
async fn input_group_fields_come_from_the_board() {
    let (_dir, state) = test_state().await;
    let id = company(&state.pool, "Gruplu", Some(3.0), 1, Some(4)).await;
    sqlx::query("UPDATE companies SET district = 'Örnek', neighborhood = 'Deneme' WHERE id = ?1")
        .bind(id)
        .execute(&state.pool)
        .await
        .unwrap();
    let board = load_board(&state, &ReadAt::Latest).await.unwrap();
    let card = board.companies.iter().find(|c| c.company_id == id).unwrap();
    assert!(card.group_key.is_some(), "ön koşul: pano bu işletmeye grup vermeli");

    let input = ProposalInput::load(&state, &board).await.unwrap();

    assert_eq!(input_company(&input, id).group_key, card.group_key);
    assert_eq!(input_company(&input, id).group_label, card.group_label);
}

// --- Girdi: öğretmenler ve taban yük ---

/// Pasif işletmenin ataması taban yüke girer; girdideki işletmenin ataması
/// girmez (motor onu kendisi ekler) — çift sayılırsa A'nın yükü 3+4+4 olurdu.
#[tokio::test]
async fn an_inactive_companys_assignment_is_base_load_and_active_ones_are_not_double_counted() {
    let (_dir, state) = test_state().await;
    let pool = &state.pool;
    let teacher_a = free_teacher(pool, "Ada").await;
    let inactive = company(pool, "Pasif", Some(3.0), 1, Some(3)).await;
    let active = company(pool, "Aktif", Some(3.0), 1, Some(4)).await;
    assign_by_hand(pool, cell(inactive, teacher_a, 1, 1)).await;
    assign_by_hand(pool, cell(active, teacher_a, 2, 1)).await;
    sqlx::query("UPDATE companies SET is_active = 0 WHERE id = ?1").bind(inactive).execute(pool).await.unwrap();

    let input = load_input(&state).await;

    assert!(input.companies.iter().all(|c| c.id != inactive), "pasif işletme girdide olmamalı");
    let teacher = input_teacher(&input, teacher_a);
    assert_eq!(teacher.base_assigned_hours, 3, "yalnız pasif işletmenin saati");
    assert_eq!(teacher.base_used_slots, BTreeSet::from([Slot::new(1, 1), Slot::new(1, 2), Slot::new(1, 3)]));
    assert_eq!(teacher.base_hours_by_day, BTreeMap::from([(1, 3)]));

    let proposal = propose_assignments_for(&state, "fillGaps", planning_today()).await.unwrap();
    let load = proposal.teacher_loads.iter().find(|l| l.teacher_id == teacher_a).unwrap();
    assert_eq!(load.hours, 3 + 4, "taban 3 + sabit 4; çift sayım olmamalı");
}

#[tokio::test]
async fn input_teachers_carry_capacity_and_free_slots_from_the_board() {
    let (_dir, state) = test_state().await;
    let teacher_id = free_teacher(&state.pool, "Ada").await;

    let input = load_input(&state).await;
    let board = load_board(&state, &ReadAt::Latest).await.unwrap();

    let teacher = input_teacher(&input, teacher_id);
    assert_eq!(teacher.capacity, board.teachers[0].capacity);
    assert_eq!(teacher.free_slots.len(), 5 * 9);
    assert!(teacher.free_slots.contains(&Slot::new(5, 9)));
    assert_eq!((teacher.base_assigned_hours, teacher.base_used_slots.len()), (0, 0));
}

// --- Girdi: ayarlar ---

#[tokio::test]
async fn input_reads_pool_day_bounds_and_the_balance_gap_from_settings() {
    let (_dir, state) = test_state().await;
    let default_input = load_input(&state).await;
    assert_eq!(default_input.balance_gap_hours, 4, "migration 0019 varsayılanı");
    assert_eq!((default_input.day_start_hour, default_input.day_end_hour), (1, 10));

    settings::set(&state.pool, "allocation_balance_gap_hours", "7").await.unwrap();
    settings::set(&state.pool, "max_daily_lessons", "6").await.unwrap();
    let input = load_input(&state).await;
    assert_eq!(input.balance_gap_hours, 7);
    assert_eq!(input.day_end_hour, 7);

    settings::set(&state.pool, "allocation_balance_gap_hours", "bozuk").await.unwrap();
    assert_eq!(load_input(&state).await.balance_gap_hours, 4, "bozuk değer 4'e düşer");
}

// --- Sınır doğrulaması ---

fn sample_company(id: i64) -> CompanyInput {
    CompanyInput {
        id,
        name: format!("İşletme {id}"),
        branches: vec![],
        student_count: 1,
        workplace_days: BTreeSet::from([1, 2]),
        awarded_hours: 0,
        is_honorary: false,
        max_hours: 4,
        is_locked: false,
        group_key: None,
        group_label: String::new(),
        current: None,
    }
}

fn sample_teacher(id: i64) -> TeacherInput {
    TeacherInput {
        id,
        name: format!("Öğretmen {id}"),
        branches: vec![],
        capacity: 20,
        free_slots: BTreeSet::from([Slot::new(1, 1)]),
        base_assigned_hours: 0,
        base_used_slots: BTreeSet::new(),
        base_hours_by_day: BTreeMap::new(),
    }
}

fn sample_input(companies: Vec<CompanyInput>, teachers: Vec<TeacherInput>) -> ProposalInput {
    ProposalInput { companies, teachers, day_start_hour: 1, day_end_hour: 10, pool_hours: 0, balance_gap_hours: 4 }
}

fn assert_inconsistent(input: &ProposalInput, expected_in_message: &str) {
    match validate_input(input) {
        Err(AppError::Database(message)) => {
            assert!(message.contains(expected_in_message), "{expected_in_message:?} yok: {message}")
        }
        other => panic!("Database hatası beklenirdi: {other:?}"),
    }
}

#[test]
fn a_consistent_input_passes_validation() {
    let input = sample_input(vec![sample_company(1), sample_company(2)], vec![sample_teacher(1), sample_teacher(2)]);
    assert!(validate_input(&input).is_ok());
}

#[test]
fn duplicate_company_or_teacher_ids_are_a_loud_error() {
    assert_inconsistent(&sample_input(vec![sample_company(1), sample_company(1)], vec![]), "işletme");
    assert_inconsistent(&sample_input(vec![], vec![sample_teacher(3), sample_teacher(3)]), "öğretmen");
}

#[test]
fn days_outside_one_to_five_are_a_loud_error() {
    for bad_day in [0, 6, -1] {
        let mut company = sample_company(1);
        company.workplace_days = BTreeSet::from([bad_day]);
        assert_inconsistent(&sample_input(vec![company], vec![]), "gün");

        let mut teacher = sample_teacher(1);
        teacher.free_slots = BTreeSet::from([Slot::new(bad_day, 1)]);
        assert_inconsistent(&sample_input(vec![], vec![teacher]), "gün");

        let mut teacher = sample_teacher(1);
        teacher.base_used_slots = BTreeSet::from([Slot::new(bad_day, 1)]);
        assert_inconsistent(&sample_input(vec![], vec![teacher]), "gün");

        let mut company = sample_company(1);
        company.current = Some(CurrentPlacement { teacher_id: 1, visit_day: bad_day, visit_hour: 1, is_forced: false, source: PlacementSource::Manual });
        assert_inconsistent(&sample_input(vec![company], vec![sample_teacher(1)]), "gün");
    }
}
