use crate::db::{settings, AppState};
use crate::error::AppResult;
use crate::services::excel_export;
use tauri::State;

/// Aktif dönemin atama, işletme ve öğrenci verilerini bir Excel çalışma
/// kitabına dönüştürür ve baytlarını döner. Dosya diske YAZILMAZ; arayüz
/// baytları indirilebilir bir dosyaya kendisi çevirir.
#[tauri::command]
pub async fn export_workbook(state: State<'_, AppState>) -> AppResult<Vec<u8>> {
    let pool = &state.pool;
    let term = settings::get_active_term(pool).await?;
    excel_export::build_workbook(pool, &term).await
}
