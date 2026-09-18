use crate::db::{settings, teachers, AppState};
use crate::domain::models::{NewTeacher, Teacher};
use crate::domain::workload::{coordinator_capacity, statutory_cap, InstitutionType};
use crate::error::{AppError, AppResult};
use serde::Serialize;
use tauri::State;

/// Öğretmen + mevzuattan türetilen kapasite bilgisi.
/// Kapasite hesabı yalnızca Rust tarafında yapılır; arayüz onu yeniden hesaplamaz.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeacherWithCapacity {
    #[serde(flatten)]
    pub teacher: Teacher,
    /// MADDE 6/4: bölüm şefi 10, atölye/laboratuvar şefi 6, şef değilse 0.
    pub chief_hours: i64,
    /// MADDE 15/2 tavanı (okul tipi ve büyükşehir durumuna göre).
    pub statutory_cap: i64,
    /// Koordinatörlük için kalan haftalık saat.
    pub capacity: i64,
}

#[tauri::command]
pub async fn list_teachers(state: State<'_, AppState>) -> AppResult<Vec<Teacher>> {
    teachers::list(&state.pool).await
}

/// Öğretmenleri kapasiteleriyle birlikte döner. Kapasite ayarlardaki okul
/// tipine ve büyükşehir durumuna bağlı olduğu için burada hesaplanır.
#[tauri::command]
pub async fn list_teachers_with_capacity(
    state: State<'_, AppState>,
) -> AppResult<Vec<TeacherWithCapacity>> {
    let all = settings::get_all(&state.pool).await?;
    let institution_type = InstitutionType::parse(
        all.get("institution_type").map(String::as_str).unwrap_or("other"),
    );
    let is_metropolitan = all
        .get("is_metropolitan_district")
        .map(|v| v == "true")
        .unwrap_or(false);
    let cap = statutory_cap(institution_type, is_metropolitan);

    let rows = teachers::list(&state.pool).await?;
    Ok(rows
        .into_iter()
        .map(|teacher| {
            let chief_hours = teachers::parse_chief_type(&teacher.chief_type).weekly_hours();
            let capacity = coordinator_capacity(
                teacher.max_extra_hours,
                chief_hours,
                teacher.other_extra_hours,
                cap,
            );
            TeacherWithCapacity {
                teacher,
                chief_hours,
                statutory_cap: cap,
                capacity,
            }
        })
        .collect())
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
