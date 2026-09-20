//! İşletme Belirleme Komisyon Tutanağı komutları (PDF ve Excel). Dosyaya
//! yazma yapılmaz; baytlar döner, kaydetme işlemini arayüz üstlenir. İki komut
//! da aynı `MinutesData`'yı okur, yalnızca biçimleri farklıdır.

use crate::db::{settings, AppState};
use crate::error::AppResult;
use crate::services::{commission_minutes_pdf, commission_minutes_xlsx};
use tauri::State;

#[tauri::command]
pub async fn export_commission_minutes_pdf(state: State<'_, AppState>) -> AppResult<Vec<u8>> {
    let term = settings::get_active_term(&state.pool).await?;
    commission_minutes_pdf::build_minutes_pdf(&state.pool, &term).await
}

#[tauri::command]
pub async fn export_commission_minutes_xlsx(state: State<'_, AppState>) -> AppResult<Vec<u8>> {
    let term = settings::get_active_term(&state.pool).await?;
    commission_minutes_xlsx::build_minutes_xlsx(&state.pool, &term).await
}
