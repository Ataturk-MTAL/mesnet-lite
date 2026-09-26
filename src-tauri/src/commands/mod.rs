pub mod assignment_commands;
pub mod availability_commands;
pub mod backup_commands;
pub mod commission_minutes_commands;
pub mod company_commands;
pub mod company_merge_commands;
pub mod dashboard_commands;
pub mod export_commands;
pub mod file_commands;
pub mod geocoding_commands;
pub mod history_commands;
pub mod hours_commands;
pub mod import_commands;
pub mod report_commands;
pub mod settings_commands;
pub mod student_commands;
pub mod student_list_commands;
pub mod teacher_commands;
pub mod term_commands;
pub mod user_commands;
pub mod version_commands;

use std::path::PathBuf;

use tauri::Manager;

use crate::error::{AppError, AppResult};

/// Uygulama veri dizinini bulur. `backup_commands` (yedek/geri yükleme) ve
/// `version_commands`/beş dışa aktarım komutu (kayıtlı sürümler) AYNI
/// dizinden AYNI şekilde türetir; iki yerde ayrı ayrı yazılmasın diye tek
/// burada tanımlıdır.
pub(crate) fn resolve_app_data_dir(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    app.path()
        .app_data_dir()
        .map_err(|e| AppError::Io(format!("Uygulama veri klasörü bulunamadı: {e}")))
}
