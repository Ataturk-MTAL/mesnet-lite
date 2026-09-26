//! İşletme Belirleme Komisyon Tutanağı komutları (PDF ve Excel). Dosyaya
//! yazma yapılmaz; baytlar döner, kaydetme işlemini arayüz üstlenir. İki komut
//! da aynı `MinutesData`'yı okur, yalnızca biçimleri farklıdır.
//!
//! `version_id` verilmemişse güncel veritabanından üretilir ve üretim
//! başarılıysa otomatik bir sürüm alınır (bkz. `services::versions`);
//! verilmişse o sürüm salt okunur açılır, YENİ sürüm ALINMAZ.

use tauri::{AppHandle, State};

use crate::commands::resolve_app_data_dir;
use crate::db::{settings, AppState};
use crate::error::AppResult;
use crate::services::{commission_minutes_pdf, commission_minutes_xlsx, versions};

#[tauri::command]
pub async fn export_commission_minutes_pdf(
    app: AppHandle,
    state: State<'_, AppState>,
    version_id: Option<i64>,
) -> AppResult<Vec<u8>> {
    let dir = versions::versions_dir(&resolve_app_data_dir(&app)?);
    match version_id {
        Some(id) => {
            let (temp_dir, pool, term) = versions::open_version_for_export(&state.pool, &dir, id).await?;
            let bytes = commission_minutes_pdf::build_minutes_pdf(&pool, &term).await;
            pool.close().await;
            drop(temp_dir);
            bytes
        }
        None => {
            let term = settings::get_active_term(&state.pool).await?;
            let bytes = commission_minutes_pdf::build_minutes_pdf(&state.pool, &term).await?;
            versions::record_auto_version(&state.pool, &dir, "commissionMinutesPdf", "Komisyon Tutanağı (PDF) çıktısı")
                .await?;
            Ok(bytes)
        }
    }
}

#[tauri::command]
pub async fn export_commission_minutes_xlsx(
    app: AppHandle,
    state: State<'_, AppState>,
    version_id: Option<i64>,
) -> AppResult<Vec<u8>> {
    let dir = versions::versions_dir(&resolve_app_data_dir(&app)?);
    match version_id {
        Some(id) => {
            let (temp_dir, pool, term) = versions::open_version_for_export(&state.pool, &dir, id).await?;
            let bytes = commission_minutes_xlsx::build_minutes_xlsx(&pool, &term).await;
            pool.close().await;
            drop(temp_dir);
            bytes
        }
        None => {
            let term = settings::get_active_term(&state.pool).await?;
            let bytes = commission_minutes_xlsx::build_minutes_xlsx(&state.pool, &term).await?;
            versions::record_auto_version(&state.pool, &dir, "commissionMinutesXlsx", "Komisyon Tutanağı (Excel) çıktısı")
                .await?;
            Ok(bytes)
        }
    }
}
