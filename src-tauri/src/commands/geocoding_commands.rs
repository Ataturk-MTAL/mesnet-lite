use crate::db::AppState;
use crate::error::AppResult;
use crate::services::geocoding::{self, GeocodeSummary};
use tauri::State;

/// Konumu olmayan tüm işletmeleri Nominatim üzerinden coğrafi kodlar.
/// OSM kullanım politikası nedeniyle istekler art arda değil saniyede bir
/// sıklıkla gönderilir; bu yüzden çağrı, kodlanacak işletme sayısına bağlı
/// olarak uzun sürebilir.
#[tauri::command]
pub async fn geocode_pending_companies(state: State<'_, AppState>) -> AppResult<GeocodeSummary> {
    geocoding::geocode_pending(&state.pool).await
}
