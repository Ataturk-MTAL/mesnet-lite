//! Uygulama kimliği değişiminde (`ai.alplab.mesnet-lite` →
//! `org.ataturkmtal.mesnet-lite`) veri klasörünün tek seferlik kopyalanması.
//!
//! Kimlik, işletim sisteminin veri klasörü adını belirler (`app_data_dir`);
//! değişince uygulama boş bir klasör görür ve veri "kaybolmuş" görünür.
//! Bu yüzden eski klasör KOPYALANIR (taşınmaz): eski klasör olduğu gibi kalır
//! ve eski sürüme dönülürse veri orada durur.
//!
//! Tek seferliğin güvencesi: yeni klasörde `mesnet-lite.db` VARSA hiçbir şey
//! yapılmaz. Bu yüzden DB, kopyanın SON adımıdır; her hata yolunda yeni
//! konumda DB oluşmaz, sonraki açılışta taşıma yeniden denenir. Çağıran,
//! hata dönerse `init_pool`u ÇALIŞTIRMAMALIDIR (aksi hâlde boş DB oluşur ve
//! taşıma bir daha denenmez).

use std::path::{Path, PathBuf};

use chrono::Local;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use crate::db::DB_FILE_NAME;
use crate::error::{AppError, AppResult};
use crate::services::backup::vacuum_into;

/// Eski kimliğin (`ai.alplab.mesnet-lite`) veri klasörü adı; yeni klasörle
/// AYNI üst dizinde durur (macOS Application Support, Windows %APPDATA%,
/// Linux ~/.local/share hepsinde kardeş klasör).
pub const LEGACY_DATA_DIR_NAME: &str = "ai.alplab.mesnet-lite";

/// Geçici kopya klasörünün ek ortası: `<yeni>.migrating-<zaman>`.
const TEMP_SUFFIX_MARKER: &str = ".migrating-";
const TEMP_TIMESTAMP_FORMAT: &str = "%Y%m%d%H%M%S%3f";
/// SQLite'ın yan dosyaları: DB `VACUUM INTO` ile tek tutarlı dosya olarak
/// yazıldığından ham kopyalanmaz (eski `-wal` yeni DB'ye karışırsa bozar).
const SQLITE_SIDE_FILE_SUFFIXES: [&str; 2] = ["-wal", "-shm"];

#[derive(Debug, PartialEq, Eq)]
pub enum MigrationOutcome {
    NotNeeded,
    Migrated { from: PathBuf },
}

/// Kullanıcıya gösterilen hata: verinin silinmediğini ve eski konumu söyler.
fn migration_error(old_dir: &Path, cause: impl std::fmt::Display) -> AppError {
    AppError::Io(format!(
        "Verileriniz yeni konuma taşınamadı; hiçbir veri silinmedi. Eski klasör: {}. Ayrıntı: {cause}. \
         Disk alanını ve klasör izinlerini kontrol edip uygulamayı yeniden açın.",
        old_dir.display()
    ))
}

/// Eski klasörde DB var, yeni klasörde yoksa eski klasörün tamamını yeni
/// klasöre kopyalar. Bkz. modül başlığı.
pub async fn migrate_legacy_data_dir(old_dir: &Path, new_dir: &Path) -> AppResult<MigrationOutcome> {
    let old_db = old_dir.join(DB_FILE_NAME);
    if new_dir.join(DB_FILE_NAME).exists() || !old_db.is_file() {
        return Ok(MigrationOutcome::NotNeeded);
    }
    let temp_dir = temp_sibling(new_dir).ok_or_else(|| migration_error(old_dir, "yeni klasörün üst dizini yok"))?;

    let result = copy_then_publish(old_dir, new_dir, &temp_dir).await;
    // Başarıda temp zaten boşalmış/yeniden adlandırılmıştır; hatada yarım kalan silinir.
    if temp_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&temp_dir) {
            eprintln!("Geçici taşıma klasörü silinemedi ({}): {e}", temp_dir.display());
        }
    }
    result.map_err(|e| migration_error(old_dir, e))?;
    eprintln!("Veri klasörü taşındı: {} -> {}", old_dir.display(), new_dir.display());
    Ok(MigrationOutcome::Migrated { from: old_dir.to_path_buf() })
}

fn temp_sibling(new_dir: &Path) -> Option<PathBuf> {
    let parent = new_dir.parent()?;
    let name = new_dir.file_name()?.to_string_lossy();
    let stamp = Local::now().format(TEMP_TIMESTAMP_FORMAT);
    Some(parent.join(format!("{name}{TEMP_SUFFIX_MARKER}{stamp}")))
}

async fn copy_then_publish(old_dir: &Path, new_dir: &Path, temp_dir: &Path) -> AppResult<()> {
    std::fs::create_dir_all(temp_dir)?;
    copy_tree(old_dir, temp_dir, true)?;
    vacuum_old_db(&old_dir.join(DB_FILE_NAME), &temp_dir.join(DB_FILE_NAME)).await?;
    publish(temp_dir, new_dir)
}

/// DB'yi `VACUUM INTO` ile kopyalar: WAL'daki henüz checkpoint edilmemiş
/// yazmalar dahil, tek tutarlı dosya üretir (db+wal+shm'i ayrı kopyalamak
/// kopya sırasında yazan süreç olursa tutarsız olabilir). Eski DB
/// `create_if_missing` OLMADAN açılır: var olmayanı sessizce yaratmasın.
async fn vacuum_old_db(old_db: &Path, target: &Path) -> AppResult<()> {
    let options = SqliteConnectOptions::new().filename(old_db).create_if_missing(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(format!("Eski veritabanı açılamadı: {e}")))?;
    let result = vacuum_into(&pool, target).await;
    pool.close().await;
    result
}

/// Özyinelemeli kopya. Üst düzeyde DB ve yan dosyaları atlanır (ayrıca
/// `VACUUM INTO` ile yazılır). Var olan hedefin üstüne yazılmaz.
fn copy_tree(src: &Path, dst: &Path, is_root: bool) -> AppResult<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if is_root && is_db_file(&name.to_string_lossy()) {
            continue;
        }
        let target = dst.join(&name);
        if entry.file_type()?.is_dir() {
            std::fs::create_dir_all(&target)?;
            copy_tree(&entry.path(), &target, false)?;
        } else if !target.exists() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn is_db_file(name: &str) -> bool {
    name == DB_FILE_NAME || SQLITE_SIDE_FILE_SUFFIXES.iter().any(|s| name == format!("{DB_FILE_NAME}{s}"))
}

/// Geçici klasörü yeni klasör adına yayımlar. Yeni klasör yoksa tek `rename`
/// (atomik). Varsa (örn. yalnız WebView verisi) içerik BİRLEŞTİRİLİR: var
/// olan ad atlanır (üzerine yazılmaz), olmayan ad `rename` ile girer; DB en
/// SON girer ki yarıda kalırsa yeni konumda DB olmasın ve taşıma yeniden denensin.
fn publish(temp_dir: &Path, new_dir: &Path) -> AppResult<()> {
    if !new_dir.exists() {
        std::fs::rename(temp_dir, new_dir)?;
        return Ok(());
    }
    if !new_dir.is_dir() {
        return Err(AppError::Io(format!("{} bir klasör değil", new_dir.display())));
    }
    let mut names: Vec<_> = std::fs::read_dir(temp_dir)?.collect::<Result<Vec<_>, _>>()?.into_iter().map(|e| e.file_name()).collect();
    names.sort_by_key(|n| n.to_string_lossy() == DB_FILE_NAME);
    for name in names {
        let target = new_dir.join(&name);
        if target.exists() {
            continue;
        }
        std::fs::rename(temp_dir.join(&name), target)?;
    }
    Ok(())
}
