use tauri::{AppHandle, State};

use crate::commands::resolve_app_data_dir;
use crate::db::{settings, AppState};
use crate::error::AppResult;
use crate::services::{excel_export, versions};

/// Aktif dönemin atama, işletme ve öğrenci verilerini bir Excel çalışma
/// kitabına dönüştürür ve baytlarını döner. Dosya diske YAZILMAZ; arayüz
/// baytları indirilebilir bir dosyaya kendisi çevirir.
///
/// `version_id` verilmemişse güncel veritabanından üretilir ve üretim
/// başarılıysa otomatik bir sürüm alınır (bkz. `services::versions`);
/// verilmişse o sürüm salt okunur açılır, YENİ sürüm ALINMAZ.
#[tauri::command]
pub async fn export_workbook(app: AppHandle, state: State<'_, AppState>, version_id: Option<i64>) -> AppResult<Vec<u8>> {
    let dir = versions::versions_dir(&resolve_app_data_dir(&app)?);
    match version_id {
        Some(id) => {
            let (temp_dir, pool, term) = versions::open_version_for_export(&state.pool, &dir, id).await?;
            let bytes = excel_export::build_workbook(&pool, &term).await;
            pool.close().await;
            drop(temp_dir);
            bytes
        }
        None => {
            let term = settings::get_active_term(&state.pool).await?;
            let bytes = excel_export::build_workbook(&state.pool, &term).await?;
            versions::record_auto_version(&state.pool, &dir, "workbook", "Çalışma Kitabı çıktısı").await?;
            Ok(bytes)
        }
    }
}
