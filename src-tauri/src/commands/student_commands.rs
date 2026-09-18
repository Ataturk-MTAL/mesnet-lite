use crate::db::{settings, students, AppState};
use crate::domain::models::{NewStudent, Student};
use crate::error::{AppError, AppResult};
use tauri::State;

/// Aktif eğitim-öğretim yılındaki öğrenciler.
#[tauri::command]
pub async fn list_students(state: State<'_, AppState>) -> AppResult<Vec<Student>> {
    let term = settings::get_active_term(&state.pool).await?;
    students::list_by_term(&state.pool, &term).await
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
            company_id: None,
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
}
