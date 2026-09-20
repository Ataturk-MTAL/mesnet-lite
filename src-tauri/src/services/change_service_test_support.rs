//! `change_service` ve `history_service` testleri için ortak fixture'lar.
//! Yalnız `cfg(test)`'te derlenir; üretim kodunda çağıranı YOKTUR ve olmamalıdır.

use chrono::NaiveDate;
use sqlx::SqlitePool;
use tempfile::TempDir;

use super::change_service::{execute_change, ChangeMode, ChangeOutcome};
use crate::db::{companies, init_pool};
use crate::domain::history::decide::{
    ChangeCommand, ChangeRequest, CompanyHoursRow, CoordinatorRow, ImpactSummary, NewStudentInput, NewTeacherProfile,
};
use crate::domain::history::events::TeacherLoad;
use crate::domain::models::{ChiefType, EmploymentType, NewCompany};

/// `init_pool`'un tohumladığı dönem: 2026-09-01 – 2027-01-31.
pub(super) const TERM: &str = "2026-2027/1";

pub(super) fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

/// Dönem 1 Eylül'de başlar; bu gün planlama evresindedir (tarih zorunlu değil).
pub(super) fn planning_today() -> NaiveDate {
    ymd(2026, 8, 15)
}

/// Dönem başladı; ay penceresi 1 Kasım'dan açılır (spec §5.1).
pub(super) fn november_today() -> NaiveDate {
    ymd(2026, 11, 10)
}

pub(super) async fn test_pool() -> (TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
    (dir, pool)
}

pub(super) fn standard_load() -> TeacherLoad {
    TeacherLoad { base_hours: 15, max_extra_hours: 24, other_extra_hours: 0, chief_type: ChiefType::None, employment_type: EmploymentType::Tenured }
}

pub(super) fn new_company(name: &str, one_way_km: Option<f64>) -> NewCompany {
    NewCompany {
        name: name.to_string(),
        contact_first_name: String::new(),
        contact_last_name: String::new(),
        phone: String::new(),
        email: String::new(),
        address_text: "Örnek Mah. Örnek Sok.".to_string(),
        latitude: None,
        longitude: None,
        one_way_distance_km: one_way_km,
        notes: String::new(),
    }
}

pub(super) fn request(effective: Option<NaiveDate>, command: ChangeCommand) -> ChangeRequest {
    ChangeRequest { term: TERM.to_string(), effective_date: effective, document_date: None, reason: "test".to_string(), command }
}

pub(super) async fn commit(pool: &SqlitePool, req: ChangeRequest, today: NaiveDate) -> ChangeOutcome {
    execute_change(pool, req, ChangeMode::Commit { expected_high_water: None }, today).await.unwrap()
}

pub(super) async fn preview(pool: &SqlitePool, req: ChangeRequest, today: NaiveDate) -> ChangeOutcome {
    execute_change(pool, req, ChangeMode::Preview, today).await.unwrap()
}

/// `Committed` beklenir; başka bir sonuç testin senaryosunun bozulduğunu
/// gösterir ve sonucu mesajda döker.
pub(super) fn expect_committed(outcome: ChangeOutcome) -> (i64, ImpactSummary) {
    match outcome {
        ChangeOutcome::Committed { change_set_id, impact } => (change_set_id, impact),
        other => panic!("Committed beklenirdi, gelen: {other:?}"),
    }
}

pub(super) async fn add_company(pool: &SqlitePool, name: &str, one_way_km: f64) -> i64 {
    companies::create(pool, &new_company(name, Some(one_way_km))).await.unwrap().id
}

async fn id_by_first_name(pool: &SqlitePool, table: &str, first_name: &str) -> i64 {
    let sql = format!("SELECT id FROM {table} WHERE first_name = ?1");
    sqlx::query_scalar(&sql).bind(first_name).fetch_one(pool).await.unwrap()
}

pub(super) fn student_input(first_name: &str) -> NewStudentInput {
    NewStudentInput {
        first_name: first_name.to_string(),
        last_name: "Öğrenci".to_string(),
        student_no: None,
        grade: "12/C".to_string(),
        branch: "Elektronik Haberleşme".to_string(),
        submitted_at: None,
    }
}

pub(super) fn teacher_profile(first_name: &str) -> NewTeacherProfile {
    NewTeacherProfile {
        first_name: first_name.to_string(),
        last_name: "Öğretmen".to_string(),
        registry_no: "1".to_string(),
        field: "Elektrik-Elektronik Teknolojisi".to_string(),
        branches: vec!["Elektronik Haberleşme".to_string()],
        is_active: true,
    }
}

/// Yeni öğrenci; `first_name` benzersiz olmalıdır (kimlik ad üzerinden bulunur).
pub(super) async fn add_student(pool: &SqlitePool, first_name: &str, company_id: Option<i64>, today: NaiveDate) -> i64 {
    let command = ChangeCommand::CreateStudent { student: student_input(first_name), company_id };
    expect_committed(commit(pool, request(None, command), today).await);
    id_by_first_name(pool, "students", first_name).await
}

pub(super) async fn add_teacher(pool: &SqlitePool, first_name: &str, today: NaiveDate) -> i64 {
    let command = ChangeCommand::CreateTeacher { teacher: teacher_profile(first_name), load: standard_load() };
    expect_committed(commit(pool, request(None, command), today).await);
    id_by_first_name(pool, "teachers", first_name).await
}

pub(super) fn hours_row(company_id: i64, awarded_hours: i64) -> CompanyHoursRow {
    CompanyHoursRow { company_id, awarded_hours, is_honorary: false, is_locked: false, notes: String::new() }
}

pub(super) fn coordinator_row(company_id: i64, teacher_id: i64) -> CoordinatorRow {
    CoordinatorRow { company_id, teacher_id, visit_day: 1, visit_hour: 3, is_forced: false, force_reason: None }
}

/// Şema tablolarının hepsindeki satır sayısı (`_sqlx_migrations` ve SQLite'ın
/// iç tabloları hariç). "Hiçbir şey yazılmadı" iddiasını tek tablo değil,
/// TÜM tablolar üzerinden doğrular.
pub(super) async fn table_counts(pool: &SqlitePool) -> Vec<(String, i64)> {
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name <> '_sqlx_migrations'
         ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .unwrap();

    let mut counts = Vec::new();
    for name in names {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {name}")).fetch_one(pool).await.unwrap();
        counts.push((name, count));
    }
    counts
}

pub(super) async fn count_of(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}")).fetch_one(pool).await.unwrap()
}

pub(super) async fn open_placement(pool: &SqlitePool, student_id: i64) -> Option<i64> {
    sqlx::query_scalar("SELECT company_id FROM student_placements WHERE student_id = ?1 AND term = ?2 AND valid_to IS NULL")
        .bind(student_id)
        .bind(TERM)
        .fetch_optional(pool)
        .await
        .unwrap()
}

/// Bir işletmenin `date` günündeki takdir edilen saati (projeksiyondan).
pub(super) async fn awarded_hours_on(pool: &SqlitePool, company_id: i64, date: NaiveDate) -> Option<i64> {
    sqlx::query_scalar(
        "SELECT awarded_hours FROM company_hour_periods
         WHERE company_id = ?1 AND term = ?2 AND valid_from <= ?3 AND (valid_to IS NULL OR ?3 < valid_to)",
    )
    .bind(company_id)
    .bind(TERM)
    .bind(date)
    .fetch_optional(pool)
    .await
    .unwrap()
}

/// Ortak sahne. Dönem planlamadayken (`planning_today`) kurulur:
/// A ve B işletmeleri (gidiş-dönüş 6 km, yani 5+ km bandı), A'da üç öğrenci,
/// A'ya 9 saat takdir (3 öğrenci → tavan 9), A'nın koordinatörü `teacher`.
pub(super) struct World {
    /// Geçici dizini yaşatır; düşerse veritabanı dosyası silinir.
    pub _dir: TempDir,
    pub pool: SqlitePool,
    pub company_a: i64,
    pub company_b: i64,
    pub students: Vec<i64>,
    pub teacher: i64,
}

pub(super) async fn world() -> World {
    let (dir, pool) = test_pool().await;
    let today = planning_today();
    let company_a = add_company(&pool, "İşletme A", 3.0).await;
    let company_b = add_company(&pool, "İşletme B", 3.0).await;

    let mut students = Vec::new();
    for name in ["Ada", "Bora", "Ceren"] {
        students.push(add_student(&pool, name, Some(company_a), today).await);
    }
    let teacher = add_teacher(&pool, "Deniz", today).await;

    expect_committed(commit(&pool, request(None, ChangeCommand::SetCompanyHours { rows: vec![hours_row(company_a, 9)] }), today).await);
    expect_committed(commit(&pool, request(None, ChangeCommand::AssignCoordinators { rows: vec![coordinator_row(company_a, teacher)] }), today).await);

    World { _dir: dir, pool, company_a, company_b, students, teacher }
}
