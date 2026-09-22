use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use crate::db::AppState;
use crate::error::{AppError, AppResult};
use crate::services::backup::{self, BackupStatus};

// Bu katman incedir: dosya yolu türetimi ve sınırda doğrulama burada,
// gerçek yedekleme/geri yükleme mantığı `services::backup`tedir (bkz.
// `commands/history_commands.rs`teki aynı ayrım notu).

/// Otomatik günlük yedek klasörünün durumunu döner: yol, son yedek tarihi
/// (varsa), yedek sayısı. Ayarlar ekranı bunu göstermek için kullanır.
#[tauri::command]
pub async fn backup_status(app: AppHandle) -> AppResult<BackupStatus> {
    let dir = resolve_auto_backup_dir(&app)?;
    backup::backup_status(&dir)
}

/// Kullanıcının Kaydet penceresinden seçtiği yola elle yedek alır.
#[tauri::command]
pub async fn create_backup(state: State<'_, AppState>, path: String) -> AppResult<()> {
    let target = require_non_empty_path(&path)?;
    backup::create_manual_backup(&state.pool, &target).await
}

/// Seçilen yedeği doğrular, mevcut veriyi `pre-restore-*` olarak yedekler,
/// veritabanı dosyasını değiştirir ve uygulamayı yeniden başlatır.
///
/// NOT: `AppHandle::restart` asla dönmez (dönüş tipi `!`); bu satırdan
/// sonrasına hiç ulaşılmaz. Doğrulama veya değiştirme başarısız olursa
/// `restore_from_backup` içindeki `?` daha ÖNCE döner, `restart()` hiç
/// çağrılmaz — kullanıcı hata mesajını görür, uygulama yeniden başlamaz.
#[tauri::command]
pub async fn restore_backup(app: AppHandle, state: State<'_, AppState>, path: String) -> AppResult<()> {
    let source = require_non_empty_path(&path)?;
    let data_dir = resolve_app_data_dir(&app)?;
    let db_path = data_dir.join(crate::db::DB_FILE_NAME);
    let backup_dir = backup::auto_backup_dir(&data_dir);
    let pre_restore_at = chrono::Local::now().naive_local();

    let result = backup::restore_from_backup(&state.pool, &db_path, &backup_dir, &source, pre_restore_at).await;

    // Havuz kapandıysa (dosya değiştirme adımına gelindiyse) hata olsa bile
    // yeniden başlatılır: kapalı havuzla uygulama her sorguda düşerdi. Dosya
    // değişimi atomik olduğundan açılışta ya eski ya yeni veri bütün bulunur.
    if result.is_ok() || state.pool.is_closed() {
        app.restart();
    }
    result
}

fn resolve_app_data_dir(app: &AppHandle) -> AppResult<PathBuf> {
    app.path()
        .app_data_dir()
        .map_err(|e| AppError::Io(format!("Uygulama veri klasörü bulunamadı: {e}")))
}

fn resolve_auto_backup_dir(app: &AppHandle) -> AppResult<PathBuf> {
    Ok(backup::auto_backup_dir(&resolve_app_data_dir(app)?))
}

/// Dışarıdan (arayüzden) gelen dosya yolu boş olamaz — sınırda doğrulama.
fn require_non_empty_path(path: &str) -> AppResult<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("Dosya yolu boş olamaz".into()));
    }
    Ok(PathBuf::from(trimmed))
}
