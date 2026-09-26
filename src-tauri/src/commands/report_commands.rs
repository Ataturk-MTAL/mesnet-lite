//! PDF rapor komutları: Koordinatör Görevlendirme Çizelgesi ve Öğretmen
//! Ziyaret Listeleri. Dosyaya yazma yapılmaz; PDF baytları döner, kaydetme
//! işlemini arayüz üstlenir.
//!
//! `version_id` verilmemişse güncel veritabanından üretilir ve üretim
//! başarılıysa otomatik bir sürüm alınır (bkz. `services::versions`);
//! verilmişse o sürüm salt okunur açılır, YENİ sürüm ALINMAZ.

use tauri::{AppHandle, State};

use crate::commands::resolve_app_data_dir;
use crate::db::read_at::ReadAt;
use crate::db::{settings, AppState};
use crate::error::AppResult;
use crate::services::{pdf_report, versions};

/// `as_of`, kenar çubuğunda seçilen tarihtir (`YYYY-MM-DD`); boşsa güncel
/// duruma (`ReadAt::Latest`) göre üretilir. `version_id` verilmişse
/// kullanılacak tarih isteğin kendi `as_of`'u varsa odur, yoksa sürümün
/// kayıtlı `as_of`'udur (brief madde 2).
#[tauri::command]
pub async fn export_assignment_sheet(
    app: AppHandle,
    state: State<'_, AppState>,
    version_id: Option<i64>,
    as_of: Option<String>,
) -> AppResult<Vec<u8>> {
    let dir = versions::versions_dir(&resolve_app_data_dir(&app)?);
    match version_id {
        Some(id) => {
            let (temp_dir, pool, term, version_as_of) = versions::open_version_for_export(&state.pool, &dir, id).await?;
            let bytes = match ReadAt::resolve(&pool, &term, as_of.or(version_as_of)).await {
                Ok(read_at) => pdf_report::build_assignment_sheet(&pool, &term, &read_at).await,
                Err(e) => Err(e),
            };
            pool.close().await;
            drop(temp_dir);
            bytes
        }
        None => {
            let term = settings::get_active_term(&state.pool).await?;
            let read_at = ReadAt::resolve(&state.pool, &term, as_of).await?;
            let bytes = pdf_report::build_assignment_sheet(&state.pool, &term, &read_at).await?;
            versions::record_auto_version(&state.pool, &dir, "assignmentSheet", "Görevlendirme Çizelgesi çıktısı", read_at.value())
                .await?;
            Ok(bytes)
        }
    }
}

/// `export_assignment_sheet` ile aynı `as_of`/`versionId` sözleşmesi.
#[tauri::command]
pub async fn export_visit_lists(
    app: AppHandle,
    state: State<'_, AppState>,
    version_id: Option<i64>,
    as_of: Option<String>,
) -> AppResult<Vec<u8>> {
    let dir = versions::versions_dir(&resolve_app_data_dir(&app)?);
    match version_id {
        Some(id) => {
            let (temp_dir, pool, term, version_as_of) = versions::open_version_for_export(&state.pool, &dir, id).await?;
            let bytes = match ReadAt::resolve(&pool, &term, as_of.or(version_as_of)).await {
                Ok(read_at) => pdf_report::build_visit_lists(&pool, &term, &read_at).await,
                Err(e) => Err(e),
            };
            pool.close().await;
            drop(temp_dir);
            bytes
        }
        None => {
            let term = settings::get_active_term(&state.pool).await?;
            let read_at = ReadAt::resolve(&state.pool, &term, as_of).await?;
            let bytes = pdf_report::build_visit_lists(&state.pool, &term, &read_at).await?;
            versions::record_auto_version(&state.pool, &dir, "visitLists", "Ziyaret Listeleri çıktısı", read_at.value()).await?;
            Ok(bytes)
        }
    }
}
