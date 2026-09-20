//! Havuz (şeflik saati) testleri için ortak fixture'lar. Yalnız `cfg(test)`'te
//! derlenir; üretim kodunda çağıranı YOKTUR ve olmamalıdır.
//!
//! Şeflik verisi doğrudan tabloya yazılmaz: gerçek yazma yolu olan
//! `execute_change` kullanılır. Böylece test, `teacher_load_periods`
//! projeksiyonunu üretimde dolduğu biçimde görür.

use chrono::NaiveDate;
use sqlx::SqlitePool;

use crate::domain::history::decide::{ChangeCommand, ChangeRequest, NewTeacherProfile};
use crate::domain::history::events::TeacherLoad;
use crate::domain::models::{ChiefType, EmploymentType};
use crate::services::change_service::{execute_change, ChangeMode, ChangeOutcome};

/// `init_pool`'un tohumladığı dönem: 2026-09-01 – 2027-01-31.
pub(crate) const TERM: &str = "2026-2027/1";

pub(crate) fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

/// Dönem başlamadan önceki gün: tarih vermeden yapılan giriş dönem başına
/// (2026-09-01) yürürlüğe girer.
fn planning_today() -> NaiveDate {
    ymd(2026, 8, 15)
}

/// Dönem başladıktan sonraki bir gün: ay penceresi 1 Kasım'dan açıktır.
fn november_today() -> NaiveDate {
    ymd(2026, 11, 10)
}

fn load_with(chief_type: ChiefType) -> TeacherLoad {
    TeacherLoad {
        base_hours: 15,
        max_extra_hours: 24,
        other_extra_hours: 0,
        chief_type,
        employment_type: EmploymentType::Tenured,
    }
}

async fn commit(pool: &SqlitePool, req: ChangeRequest, today: NaiveDate) {
    let outcome = execute_change(pool, req, ChangeMode::Commit { expected_high_water: None }, today)
        .await
        .unwrap();
    assert!(matches!(outcome, ChangeOutcome::Committed { .. }), "Committed beklenirdi: {outcome:?}");
}

/// Dönem başından itibaren verilen şeflik türüyle yeni bir öğretmen açar.
/// `first_name` benzersiz olmalıdır (kimlik ad üzerinden bulunur).
pub(crate) async fn seed_teacher(pool: &SqlitePool, first_name: &str, chief_type: ChiefType) -> i64 {
    let teacher = NewTeacherProfile {
        first_name: first_name.to_string(),
        last_name: "Öğretmen".to_string(),
        registry_no: "1".to_string(),
        field: "Elektrik-Elektronik Teknolojisi".to_string(),
        branches: vec!["Elektronik Haberleşme".to_string()],
        is_active: true,
    };
    let command = ChangeCommand::CreateTeacher { teacher, load: load_with(chief_type) };
    let req = ChangeRequest {
        term: TERM.to_string(),
        effective_date: None,
        document_date: None,
        reason: "test".to_string(),
        command,
    };
    commit(pool, req, planning_today()).await;
    sqlx::query_scalar("SELECT id FROM teachers WHERE first_name = ?1")
        .bind(first_name)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Öğretmenin şeflik türünü `date` gününden itibaren değiştirir; önceki
/// aralık o gün kapanır (`valid_to = date`).
pub(crate) async fn change_chief_type(pool: &SqlitePool, teacher_id: i64, chief_type: ChiefType, date: NaiveDate) {
    let req = ChangeRequest {
        term: TERM.to_string(),
        effective_date: Some(date),
        document_date: None,
        reason: "test".to_string(),
        command: ChangeCommand::SetTeacherLoad { teacher_id, load: load_with(chief_type) },
    };
    commit(pool, req, november_today()).await;
}

pub(crate) async fn deactivate_teacher(pool: &SqlitePool, teacher_id: i64) {
    sqlx::query("UPDATE teachers SET is_active = 0 WHERE id = ?1")
        .bind(teacher_id)
        .execute(pool)
        .await
        .unwrap();
}
