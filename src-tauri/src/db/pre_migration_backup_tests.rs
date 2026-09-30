//! Göç öncesi zorunlu yedeğin (`services::pre_migration_backup`) `init_pool`
//! içindeki davranışı. Eski şema, gerçek bir önceki sürümün göçleri
//! uygulanarak kurulur (`migration_0015_tests.rs`teki desen).

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use super::init_pool;

/// 0015'ten önceki son sürüm: "güncelleme sonrası bekleyen göç" senaryosu.
const OLD_SCHEMA_VERSION: i64 = 14;

/// `path` konumunda, yalnız `version`'a kadar göç uygulanmış bir veritabanı
/// kurar ve havuzu KAPATIR (uygulama kapalıyken diskte kalan eski sürüm gibi).
async fn create_db_at_version(path: &Path, version: i64) {
    let options = SqliteConnectOptions::new().filename(path).create_if_missing(true).foreign_keys(true);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
    let mut legacy = sqlx::migrate!("./migrations");
    let steps: Vec<_> = legacy.migrations.iter().filter(|m| m.version <= version).cloned().collect();
    legacy.migrations = Cow::Owned(steps);
    legacy.run(&pool).await.unwrap();
    sqlx::query("INSERT INTO companies (name, address_text, created_at, updated_at) VALUES ('Kurgusal İşletme', 'Adres', 'x', 'x')")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

async fn open_readonly(path: &Path) -> SqlitePool {
    let options = SqliteConnectOptions::new().filename(path).read_only(true);
    SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap()
}

async fn max_applied_version(path: &Path) -> i64 {
    let pool = open_readonly(path).await;
    let version = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations").fetch_one(&pool).await.unwrap();
    pool.close().await;
    version
}

fn pre_migration_files(backup_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(backup_dir) else {
        return Vec::new();
    };
    entries
        .map(|e| e.unwrap().path())
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("pre-migration-"))
        .collect()
}

#[tokio::test]
async fn pending_migration_takes_a_separate_backup_holding_the_old_schema() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    create_db_at_version(&db_path, OLD_SCHEMA_VERSION).await;
    let latest = crate::services::backup::embedded_max_migration_version();

    // Bugünün günlük yedeği ÖNCEDEN var: eski davranış (atla) göçü korumasız
    // bırakırdı; ayrı yedek yine de alınmalı.
    let backup_dir = dir.path().join("backups");
    std::fs::create_dir_all(&backup_dir).unwrap();
    let today = crate::domain::terms::today_local();
    std::fs::write(backup_dir.join(format!("mesnet-lite-{}.db", today.format("%Y-%m-%d"))), b"sabah").unwrap();

    init_pool(&db_path).await.unwrap().close().await;

    let files = pre_migration_files(&backup_dir);
    assert_eq!(files.len(), 1, "tam bir göç öncesi yedeği olmalı: {files:?}");
    let name = files[0].file_name().unwrap().to_string_lossy().to_string();
    assert!(name.starts_with(&format!("pre-migration-v{OLD_SCHEMA_VERSION}-v{latest}-")), "ad: {name}");
    assert!(name.ends_with(".db"), "ad: {name}");

    assert_eq!(max_applied_version(&files[0]).await, OLD_SCHEMA_VERSION, "yedek ESKİ şemayı taşımalı");
    assert_eq!(max_applied_version(&db_path).await, latest, "asıl veritabanı yükseltilmiş olmalı");
}

#[tokio::test]
async fn up_to_date_schema_takes_no_pre_migration_backup() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    init_pool(&db_path).await.unwrap().close().await;
    init_pool(&db_path).await.unwrap().close().await;

    assert!(pre_migration_files(&dir.path().join("backups")).is_empty());
}

#[tokio::test]
async fn first_install_takes_no_pre_migration_backup() {
    let dir = tempfile::tempdir().unwrap();
    init_pool(&dir.path().join("test.db")).await.unwrap().close().await;

    assert!(pre_migration_files(&dir.path().join("backups")).is_empty());
}

#[tokio::test]
async fn failed_pre_migration_backup_blocks_the_migration() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    create_db_at_version(&db_path, OLD_SCHEMA_VERSION).await;
    // `backups` bir DİZİN yerine DOSYA: yedek hedefi yazılamaz.
    std::fs::write(dir.path().join("backups"), b"dizin degil").unwrap();

    let err = init_pool(&db_path).await.unwrap_err().to_string();

    assert!(err.contains("yedek alınamadı"), "kullanıcıya gösterilecek mesaj: {err}");
    assert!(err.contains("veriniz değiştirilmedi"), "mesaj: {err}");
    assert_eq!(max_applied_version(&db_path).await, OLD_SCHEMA_VERSION, "göç ÇALIŞMAMALI");
}
