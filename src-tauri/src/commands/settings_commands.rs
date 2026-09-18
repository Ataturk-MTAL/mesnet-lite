use crate::db::{settings, AppState};
use crate::error::{AppError, AppResult};
use std::collections::BTreeMap;
use tauri::State;

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> AppResult<BTreeMap<String, String>> {
    settings::get_all(&state.pool).await
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    entries: BTreeMap<String, String>,
) -> AppResult<BTreeMap<String, String>> {
    settings::set_many(&state.pool, &entries).await?;
    settings::get_all(&state.pool).await
}

/// Okul konumunu haritadan gelen değerle yazar.
#[tauri::command]
pub async fn set_school_location(
    state: State<'_, AppState>,
    latitude: f64,
    longitude: f64,
) -> AppResult<()> {
    validate_coordinates(latitude, longitude)?;
    settings::set_school_location(&state.pool, latitude, longitude).await
}

fn validate_coordinates(latitude: f64, longitude: f64) -> AppResult<()> {
    if !(-90.0..=90.0).contains(&latitude) {
        return Err(AppError::Validation(
            "Enlem -90 ile 90 arasında olmalı".into(),
        ));
    }
    if !(-180.0..=180.0).contains(&longitude) {
        return Err(AppError::Validation(
            "Boylam -180 ile 180 arasında olmalı".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_out_of_range_coordinates() {
        assert!(validate_coordinates(91.0, 34.0).is_err());
        assert!(validate_coordinates(36.0, 181.0).is_err());
        assert!(validate_coordinates(-91.0, 34.0).is_err());
    }

    #[test]
    fn accepts_mersin_coordinates() {
        assert!(validate_coordinates(36.8121, 34.6415).is_ok());
    }
}
