//! PDF rapor komutları: Koordinatör Görevlendirme Çizelgesi ve Öğretmen
//! Ziyaret Listeleri. Dosyaya yazma yapılmaz; PDF baytları döner, kaydetme
//! işlemini arayüz üstlenir.
//!
//! `version_id` verilmemişse güncel veritabanından üretilir ve üretim
//! başarılıysa otomatik bir sürüm alınır (bkz. `services::versions`);
//! verilmişse o sürüm salt okunur açılır, YENİ sürüm ALINMAZ.

use tauri::{AppHandle, State};

use crate::commands::resolve_app_data_dir;
use crate::db::{settings, AppState};
use crate::error::AppResult;
use crate::services::{pdf_report, versions};

#[tauri::command]
pub async fn export_assignment_sheet(
    app: AppHandle,
    state: State<'_, AppState>,
    version_id: Option<i64>,
) -> AppResult<Vec<u8>> {
    let dir = versions::versions_dir(&resolve_app_data_dir(&app)?);
    match version_id {
        Some(id) => {
            let (temp_dir, pool, term) = versions::open_version_for_export(&state.pool, &dir, id).await?;
            let bytes = pdf_report::build_assignment_sheet(&pool, &term).await;
            pool.close().await;
            drop(temp_dir);
            bytes
        }
        None => {
            let term = settings::get_active_term(&state.pool).await?;
            let bytes = pdf_report::build_assignment_sheet(&state.pool, &term).await?;
            versions::record_auto_version(&state.pool, &dir, "assignmentSheet", "Görevlendirme Çizelgesi çıktısı").await?;
            Ok(bytes)
        }
    }
}

#[tauri::command]
pub async fn export_visit_lists(app: AppHandle, state: State<'_, AppState>, version_id: Option<i64>) -> AppResult<Vec<u8>> {
    let dir = versions::versions_dir(&resolve_app_data_dir(&app)?);
    match version_id {
        Some(id) => {
            let (temp_dir, pool, term) = versions::open_version_for_export(&state.pool, &dir, id).await?;
            let bytes = pdf_report::build_visit_lists(&pool, &term).await;
            pool.close().await;
            drop(temp_dir);
            bytes
        }
        None => {
            let term = settings::get_active_term(&state.pool).await?;
            let bytes = pdf_report::build_visit_lists(&state.pool, &term).await?;
            versions::record_auto_version(&state.pool, &dir, "visitLists", "Ziyaret Listeleri çıktısı").await?;
            Ok(bytes)
        }
    }
}
