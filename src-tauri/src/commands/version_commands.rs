//! Kayıtlı sürüm komutları: listeleme, elle kayıt, silme. Beş dışa aktarım
//! komutu (`report_commands`/`commission_minutes_commands`/`export_commands`)
//! sürüm alma/açma mantığını doğrudan `services::versions`ten kullanır; bu
//! dosya yalnız arayüzün doğrudan çağırdığı üç komutu barındırır.
//!
//! Bu katman incedir: dosya yolu türetimi burada, gerçek iş
//! `services::versions`tedir (bkz. `commands/backup_commands.rs`teki aynı
//! ayrım notu).

use tauri::{AppHandle, State};

use crate::commands::resolve_app_data_dir;
use crate::db::AppState;
use crate::domain::terms::now_local;
use crate::error::AppResult;
use crate::services::versions::{self, Version, VersionKind};

/// Kayıtlı sürümleri en yeniden eskiye döner.
#[tauri::command]
pub async fn list_versions(app: AppHandle, state: State<'_, AppState>) -> AppResult<Vec<Version>> {
    let dir = versions::versions_dir(&resolve_app_data_dir(&app)?);
    versions::list_versions(&state.pool, &dir).await
}

/// Kullanıcının bir ad vererek elle aldığı sürüm. Veri değişmemiş olsa bile
/// HER ZAMAN yeni bir sürüm açar (bkz. `services::versions::create_version`).
/// `as_of` isteğe bağlıdır; verilirse aktif dönemin `[start, end]` aralığında
/// olmalıdır (`ReadAt::resolve`), aksi hâlde Türkçe `Validation` döner.
#[tauri::command]
pub async fn create_version(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    as_of: Option<String>,
) -> AppResult<Version> {
    let dir = versions::versions_dir(&resolve_app_data_dir(&app)?);
    versions::create_version(&state.pool, &dir, &name, VersionKind::Manual, None, as_of, now_local()).await
}

/// Bir sürümü siler: dosyasını ve satırını.
#[tauri::command]
pub async fn delete_version(app: AppHandle, state: State<'_, AppState>, id: i64) -> AppResult<()> {
    let dir = versions::versions_dir(&resolve_app_data_dir(&app)?);
    versions::delete_version(&state.pool, &dir, id).await
}
