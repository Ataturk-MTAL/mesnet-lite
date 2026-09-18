use crate::db::{students, AppState};
use crate::domain::models::{NewStudent, Student};
use crate::error::{AppError, AppResult};
use tauri::State;

#[tauri::command]
pub async fn list_students(state: State<'_, AppState>) -> AppResult<Vec<Student>> {
    students::list(&state.pool).await
}

#[tauri::command]
pub async fn create_student(state: State<'_, AppState>, input: NewStudent) -> AppResult<Student> {
    validate(&input)?;
    students::create(&state.pool, &input).await
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
