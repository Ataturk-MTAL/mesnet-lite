//! PDF rapor komutları: Koordinatör Görevlendirme Çizelgesi ve Öğretmen
//! Ziyaret Listeleri. Dosyaya yazma yapılmaz; PDF baytları döner, kaydetme
//! işlemini arayüz üstlenir.

use crate::db::{settings, AppState};
use crate::error::AppResult;
use crate::services::pdf_report;
use tauri::State;

#[tauri::command]
pub async fn export_assignment_sheet(state: State<'_, AppState>) -> AppResult<Vec<u8>> {
    let term = settings::get_active_term(&state.pool).await?;
    pdf_report::build_assignment_sheet(&state.pool, &term).await
}

#[tauri::command]
pub async fn export_visit_lists(state: State<'_, AppState>) -> AppResult<Vec<u8>> {
    let term = settings::get_active_term(&state.pool).await?;
    pdf_report::build_visit_lists(&state.pool, &term).await
}
