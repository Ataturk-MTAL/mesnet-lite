use crate::db::companies::CompanyRemoval;
use crate::db::{companies, AppState};
use crate::domain::models::{Company, NewCompany};
use crate::error::{AppError, AppResult};
use tauri::State;

// Bu katman incedir: yalnızca sınır doğrulaması yapar.
// İş mantığı db/ ve domain/ içindedir.

#[tauri::command]
pub async fn list_companies(state: State<'_, AppState>) -> AppResult<Vec<Company>> {
    companies::list(&state.pool).await
}

#[tauri::command]
pub async fn get_company(state: State<'_, AppState>, id: i64) -> AppResult<Company> {
    companies::get(&state.pool, id).await
}

#[tauri::command]
pub async fn create_company(state: State<'_, AppState>, input: NewCompany) -> AppResult<Company> {
    validate(&input)?;
    companies::create(&state.pool, &input).await
}

#[tauri::command]
pub async fn update_company(
    state: State<'_, AppState>,
    id: i64,
    input: NewCompany,
) -> AppResult<Company> {
    validate(&input)?;
    companies::update(&state.pool, id, &input).await
}

#[tauri::command]
pub async fn delete_company(state: State<'_, AppState>, id: i64) -> AppResult<CompanyRemoval> {
    companies::remove(&state.pool, id).await
}

#[tauri::command]
pub async fn set_company_location(
    state: State<'_, AppState>,
    id: i64,
    latitude: f64,
    longitude: f64,
) -> AppResult<Company> {
    validate_coordinates(Some(latitude), Some(longitude))?;
    // Haritadan elle işaretleme her zaman 'manual' durumunu yazar.
    companies::set_location(&state.pool, id, latitude, longitude, "manual").await
}

/// Sınır doğrulaması: dış veri güvenilmez kabul edilir.
fn validate(input: &NewCompany) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::Validation("İşletme adı boş olamaz".into()));
    }
    if input.address_text.trim().is_empty() {
        return Err(AppError::Validation("Adres boş olamaz".into()));
    }
    if let Some(km) = input.one_way_distance_km {
        if km < 0.0 {
            return Err(AppError::Validation("Mesafe negatif olamaz".into()));
        }
    }
    validate_coordinates(input.latitude, input.longitude)
}

fn validate_coordinates(latitude: Option<f64>, longitude: Option<f64>) -> AppResult<()> {
    if let Some(lat) = latitude {
        if !(-90.0..=90.0).contains(&lat) {
            return Err(AppError::Validation(
                "Enlem -90 ile 90 arasında olmalı".into(),
            ));
        }
    }
    if let Some(lon) = longitude {
        if !(-180.0..=180.0).contains(&lon) {
            return Err(AppError::Validation(
                "Boylam -180 ile 180 arasında olmalı".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_input() -> NewCompany {
        NewCompany {
            name: "Test İşletme A".into(),
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
        }
    }

    #[test]
    fn rejects_blank_name() {
        let mut input = valid_input();
        input.name = "   ".into();
        assert!(validate(&input).is_err());
    }

    #[test]
    fn rejects_blank_address() {
        let mut input = valid_input();
        input.address_text = "  ".into();
        assert!(validate(&input).is_err());
    }

    #[test]
    fn rejects_negative_distance() {
        let mut input = valid_input();
        input.one_way_distance_km = Some(-1.0);
        assert!(validate(&input).is_err());
    }

    #[test]
    fn rejects_out_of_range_latitude() {
        let mut input = valid_input();
        input.latitude = Some(91.0);
        assert!(validate(&input).is_err());
    }

    #[test]
    fn rejects_out_of_range_longitude() {
        let mut input = valid_input();
        input.longitude = Some(-181.0);
        assert!(validate(&input).is_err());
    }

    #[test]
    fn accepts_valid_input() {
        assert!(validate(&valid_input()).is_ok());
    }
}
