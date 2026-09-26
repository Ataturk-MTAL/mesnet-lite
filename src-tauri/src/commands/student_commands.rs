use crate::db::read_at::ReadAt;
use crate::db::{settings, students, AppState};
use crate::domain::models::{NewStudent, Student};
use crate::error::{AppError, AppResult};
use tauri::State;

/// Aktif eğitim-öğretim yılındaki öğrenciler. `asOf` eksikse `ReadAt::Latest`
/// (bugünkü davranışın birebir aynısı); verilirse aktif dönemin aralığında
/// olması zorunludur (spec §6).
#[tauri::command]
pub async fn list_students(state: State<'_, AppState>, as_of: Option<String>) -> AppResult<Vec<Student>> {
    let term = settings::get_active_term(&state.pool).await?;
    let read_at = ReadAt::resolve(&state.pool, &term, as_of).await?;
    students::list_by_term(&state.pool, &term, &read_at).await
}

/// Veritabanındaki tüm eğitim-öğretim yılları, en yeniden eskiye.
#[tauri::command]
pub async fn list_terms(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    students::list_terms(&state.pool).await
}

#[tauri::command]
pub async fn create_student(
    state: State<'_, AppState>,
    input: NewStudent,
) -> AppResult<Student> {
    validate(&input)?;
    // Dönem boş gelirse aktif döneme yazılır; öğrenci dönemsiz kalamaz.
    let mut to_create = input;
    if to_create.term.trim().is_empty() {
        to_create.term = settings::get_active_term(&state.pool).await?;
    }
    students::create(&state.pool, &to_create).await
}

#[tauri::command]
pub async fn update_student(
    state: State<'_, AppState>,
    id: i64,
    input: NewStudent,
) -> AppResult<Student> {
    validate(&input)?;
    students::update(&state.pool, id, &input).await
}

#[tauri::command]
pub async fn delete_student(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    students::remove(&state.pool, id).await
}

fn validate(input: &NewStudent) -> AppResult<()> {
    if input.first_name.trim().is_empty() || input.last_name.trim().is_empty() {
        return Err(AppError::Validation("Ad ve soyad boş olamaz".into()));
    }
    if input.grade.trim().is_empty() {
        return Err(AppError::Validation("Sınıf boş olamaz".into()));
    }
    if input.branch.trim().is_empty() {
        return Err(AppError::Validation("Dal boş olamaz".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> NewStudent {
        NewStudent {
            first_name: "Ahmet".into(),
            last_name: "Yilmaz".into(),
            student_no: None,
            grade: "12/C".into(),
            branch: "Elektronik Haberleşme".into(),
            submitted_at: None,
            term: "2026-2027/1".into(),
        }
    }

    #[test]
    fn rejects_blank_names() {
        let mut input = valid();
        input.first_name = "  ".into();
        assert!(validate(&input).is_err());
    }

    #[test]
    fn rejects_blank_grade_and_branch() {
        let mut input = valid();
        input.grade = String::new();
        assert!(validate(&input).is_err());

        let mut input = valid();
        input.branch = String::new();
        assert!(validate(&input).is_err());
    }

    #[test]
    fn accepts_valid_student() {
        assert!(validate(&valid()).is_ok());
    }

    // --- R5 spec §6: `list_students` tarih itibarıyla okur ---

    use crate::db::companies;
    use crate::db::init_pool;
    use crate::domain::history::decide::{ChangeCommand, ChangeRequest, NewStudentInput, TransferTarget};
    use crate::domain::models::NewCompany;
    use crate::services::change_service::{execute_change, ChangeMode, ChangeOutcome};
    use chrono::NaiveDate;
    use sqlx::SqlitePool;

    const TERM: &str = "2026-2027/1";

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
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

    async fn commit(pool: &SqlitePool, req: ChangeRequest, today: NaiveDate) {
        let outcome = execute_change(pool, req, ChangeMode::Commit { expected_high_water: None }, today)
            .await
            .unwrap();
        assert!(matches!(outcome, ChangeOutcome::Committed { .. }), "Committed beklenirdi");
    }

    /// Öğrenci dönem başında (planlamada) İşletme A'ya yerleşir, dönem
    /// başladıktan sonraki bir tarihte (2026-11-05) İşletme B'ye nakledilir.
    /// `AsOf` iki durumu da doğru gösterir; `Latest` her zaman güncel açık
    /// yerleşimi görür — ikisi de gerçek takvim gününden BAĞIMSIZDIR çünkü
    /// `student_placements` "AÇIK satır" deseniyle okunur.
    #[tokio::test]
    async fn list_by_term_as_of_reads_the_placement_at_the_given_date() {
        let (_dir, pool) = test_pool().await;
        let company_a = a_company(&pool, "İşletme A").await;
        let company_b = a_company(&pool, "İşletme B").await;

        commit(
            &pool,
            ChangeRequest {
                term: TERM.to_string(),
                effective_date: None,
                document_date: None,
                reason: "test".into(),
                command: ChangeCommand::CreateStudent {
                    student: NewStudentInput {
                        first_name: "Ahmet".into(),
                        last_name: "Yilmaz".into(),
                        student_no: None,
                        grade: "12/C".into(),
                        branch: "Elektronik Haberleşme".into(),
                        submitted_at: None,
                    },
                    company_id: Some(company_a),
                },
            },
            ymd(2026, 8, 15),
        )
        .await;

        let student_id: i64 = sqlx::query_scalar("SELECT id FROM students WHERE first_name = 'Ahmet'")
            .fetch_one(&pool)
            .await
            .unwrap();

        commit(
            &pool,
            ChangeRequest {
                term: TERM.to_string(),
                effective_date: Some(ymd(2026, 11, 5)),
                document_date: None,
                reason: "nakil".into(),
                command: ChangeCommand::TransferStudent {
                    student_id,
                    from_company_id: company_a,
                    to: TransferTarget::Existing { company_id: company_b },
                },
            },
            ymd(2026, 11, 10),
        )
        .await;

        let before = ReadAt::resolve(&pool, TERM, Some("2026-10-01".into())).await.unwrap();
        let before_student = students::list_by_term(&pool, TERM, &before)
            .await
            .unwrap()
            .into_iter()
            .find(|s| s.id == student_id)
            .unwrap();
        assert_eq!(before_student.company_id, Some(company_a), "10-01'de öğrenci hâlâ A'daydı");

        let after = ReadAt::resolve(&pool, TERM, Some("2026-11-06".into())).await.unwrap();
        let after_student = students::list_by_term(&pool, TERM, &after)
            .await
            .unwrap()
            .into_iter()
            .find(|s| s.id == student_id)
            .unwrap();
        assert_eq!(after_student.company_id, Some(company_b), "11-06'da öğrenci B'ye nakledilmişti");

        let latest_student = students::list_by_term(&pool, TERM, &ReadAt::Latest)
            .await
            .unwrap()
            .into_iter()
            .find(|s| s.id == student_id)
            .unwrap();
        assert_eq!(latest_student.company_id, Some(company_b), "Latest her zaman güncel açık yerleşimi görür");
    }

    /// Bozuk biçimli bir tarih `Validation` ile reddedilir.
    #[tokio::test]
    async fn list_students_rejects_a_malformed_date() {
        let (_dir, pool) = test_pool().await;
        let err = ReadAt::resolve(&pool, TERM, Some("05-11-2026".into())).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }
}
