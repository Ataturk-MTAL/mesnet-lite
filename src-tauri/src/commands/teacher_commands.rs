use crate::db::{teachers, AppState};
use crate::domain::models::{NewTeacher, Teacher};
use crate::error::{AppError, AppResult};
use tauri::State;

#[tauri::command]
pub async fn list_teachers(state: State<'_, AppState>) -> AppResult<Vec<Teacher>> {
    teachers::list(&state.pool).await
}

#[tauri::command]
pub async fn create_teacher(state: State<'_, AppState>, input: NewTeacher) -> AppResult<Teacher> {
    validate(&input)?;
    teachers::create(&state.pool, &input).await
}

#[tauri::command]
pub async fn update_teacher(
    state: State<'_, AppState>,
    id: i64,
    input: NewTeacher,
) -> AppResult<Teacher> {
    validate(&input)?;
    teachers::update(&state.pool, id, &input).await
}

#[tauri::command]
pub async fn delete_teacher(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    teachers::remove(&state.pool, id).await
}

fn validate(input: &NewTeacher) -> AppResult<()> {
    if input.first_name.trim().is_empty() || input.last_name.trim().is_empty() {
        return Err(AppError::Validation("Ad ve soyad boş olamaz".into()));
    }
    if input.field.trim().is_empty() {
        return Err(AppError::Validation("Alan boş olamaz".into()));
    }
    // MADDE 6/1-c: azamî ek ders tavanı; şeflik saati bu tavanın İÇİNDE verilir,
    // bu yüzden şeflik saatinden küçük bir tavan tutarsızdır.
    let chief_hours = teachers::parse_chief_type(&input.chief_type).weekly_hours();
    if input.max_extra_hours < chief_hours {
        return Err(AppError::Validation(format!(
            "Azami ek ders ({}) şeflik saatinden ({}) küçük olamaz",
            input.max_extra_hours, chief_hours
        )));
    }
    if input.other_extra_hours < 0 || input.base_hours < 0 {
        return Err(AppError::Validation("Saatler negatif olamaz".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> NewTeacher {
        NewTeacher {
            first_name: "Test".into(),
            last_name: "Ogretmen".into(),
            registry_no: String::new(),
            field: "Elektrik-Elektronik Teknolojisi".into(),
            branches: vec!["Elektronik Haberleşme".into()],
            employment_type: "tenured".into(),
            base_hours: 20,
            max_extra_hours: 24,
            other_extra_hours: 0,
            chief_type: "none".into(),
            is_active: true,
        }
    }

    #[test]
    fn rejects_blank_names_and_field() {
        let mut input = valid();
        input.first_name = " ".into();
        assert!(validate(&input).is_err());

        let mut input = valid();
        input.field = String::new();
        assert!(validate(&input).is_err());
    }

    /// Bölüm şefi 10 saat alır; azami ek ders 10'un altındaysa tutarsızdır.
    #[test]
    fn rejects_max_extra_hours_below_chief_hours() {
        let mut input = valid();
        input.chief_type = "department".into();
        input.max_extra_hours = 8;
        assert!(validate(&input).is_err());
    }

    #[test]
    fn accepts_department_chief_with_default_cap() {
        let mut input = valid();
        input.chief_type = "department".into();
        assert!(validate(&input).is_ok());
    }

    #[test]
    fn accepts_valid_teacher() {
        assert!(validate(&valid()).is_ok());
    }
}
