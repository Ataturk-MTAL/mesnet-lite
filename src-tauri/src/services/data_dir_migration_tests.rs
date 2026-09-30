//! `data_dir_migration` testleri: tutarlı kopya (WAL dahil), idempotentlik,
//! ilk kurulumda no-op ve başarısızlıkta temizlik.

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

use super::data_dir_migration::{migrate_legacy_data_dir, MigrationOutcome};
use crate::db::{init_pool, DB_FILE_NAME};

async fn make_old_dir(root: &Path) -> (std::path::PathBuf, SqlitePool) {
    let old = root.join("old");
    let pool = init_pool(&old.join(DB_FILE_NAME)).await.unwrap();
    sqlx::query("INSERT INTO companies (name, address_text, created_at, updated_at) VALUES ('Kurgusal A.Ş.', 'adres', 't', 't')")
        .execute(&pool)
        .await
        .unwrap();
    std::fs::create_dir_all(old.join("backups")).unwrap();
    std::fs::write(old.join("backups/a.db"), b"yedek").unwrap();
    std::fs::create_dir_all(old.join("versions/v1")).unwrap();
    std::fs::write(old.join("versions/v1/s.db"), b"surum").unwrap();
    (old, pool)
}

async fn count_companies(db: &Path) -> i64 {
    let opts = SqliteConnectOptions::new().filename(db);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await.unwrap();
    let n = sqlx::query_scalar("SELECT COUNT(*) FROM companies").fetch_one(&pool).await.unwrap();
    pool.close().await;
    n
}

fn no_temp_left(root: &Path) -> bool {
    std::fs::read_dir(root).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().contains(".migrating-"))
}

#[tokio::test]
async fn copies_everything_and_leaves_old_dir_intact() {
    let root = tempfile::tempdir().unwrap();
    let (old, pool) = make_old_dir(root.path()).await;
    pool.close().await;
    let new = root.path().join("new");

    let outcome = migrate_legacy_data_dir(&old, &new).await.unwrap();

    assert_eq!(outcome, MigrationOutcome::Migrated { from: old.clone() });
    assert_eq!(count_companies(&new.join(DB_FILE_NAME)).await, 1);
    assert_eq!(std::fs::read(new.join("backups/a.db")).unwrap(), b"yedek");
    assert_eq!(std::fs::read(new.join("versions/v1/s.db")).unwrap(), b"surum");
    assert_eq!(count_companies(&old.join(DB_FILE_NAME)).await, 1);
    assert!(old.join("backups/a.db").exists());
    assert!(no_temp_left(root.path()));
}

#[tokio::test]
async fn skips_when_new_db_already_exists() {
    let root = tempfile::tempdir().unwrap();
    let (old, pool) = make_old_dir(root.path()).await;
    pool.close().await;
    let new = root.path().join("new");
    std::fs::create_dir_all(&new).unwrap();
    std::fs::write(new.join(DB_FILE_NAME), b"yeni-db").unwrap();

    let outcome = migrate_legacy_data_dir(&old, &new).await.unwrap();

    assert_eq!(outcome, MigrationOutcome::NotNeeded);
    assert_eq!(std::fs::read(new.join(DB_FILE_NAME)).unwrap(), b"yeni-db");
    assert!(!new.join("backups").exists());
}

#[tokio::test]
async fn does_nothing_on_first_install() {
    let root = tempfile::tempdir().unwrap();
    let new = root.path().join("new");

    let outcome = migrate_legacy_data_dir(&root.path().join("old"), &new).await.unwrap();

    assert_eq!(outcome, MigrationOutcome::NotNeeded);
    assert!(!new.exists());
}

#[tokio::test]
async fn second_run_is_noop() {
    let root = tempfile::tempdir().unwrap();
    let (old, pool) = make_old_dir(root.path()).await;
    pool.close().await;
    let new = root.path().join("new");
    migrate_legacy_data_dir(&old, &new).await.unwrap();

    let outcome = migrate_legacy_data_dir(&old, &new).await.unwrap();

    assert_eq!(outcome, MigrationOutcome::NotNeeded);
}

#[tokio::test]
async fn fails_cleanly_when_target_is_a_file() {
    let root = tempfile::tempdir().unwrap();
    let (old, pool) = make_old_dir(root.path()).await;
    pool.close().await;
    let new = root.path().join("new");
    std::fs::write(&new, b"dosya").unwrap();

    let result = migrate_legacy_data_dir(&old, &new).await;

    assert!(result.is_err());
    assert!(new.is_file());
    assert!(no_temp_left(root.path()));
    assert_eq!(count_companies(&old.join(DB_FILE_NAME)).await, 1);
}

#[tokio::test]
async fn fails_cleanly_when_old_db_is_corrupt() {
    let root = tempfile::tempdir().unwrap();
    let old = root.path().join("old");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join(DB_FILE_NAME), b"bu bir sqlite dosyasi degil, yeterince uzun bir govde").unwrap();
    let new = root.path().join("new");

    let result = migrate_legacy_data_dir(&old, &new).await;

    assert!(result.is_err());
    assert!(!new.join(DB_FILE_NAME).exists());
    assert!(no_temp_left(root.path()));
    assert!(old.join(DB_FILE_NAME).exists());
}

#[tokio::test]
async fn merges_into_existing_dir_without_overwriting() {
    let root = tempfile::tempdir().unwrap();
    let (old, pool) = make_old_dir(root.path()).await;
    pool.close().await;
    let new = root.path().join("new");
    std::fs::create_dir_all(new.join("backups")).unwrap();
    std::fs::write(new.join("webview.dat"), b"webview").unwrap();
    std::fs::write(new.join("backups/mevcut.txt"), b"x").unwrap();

    migrate_legacy_data_dir(&old, &new).await.unwrap();

    assert_eq!(std::fs::read(new.join("webview.dat")).unwrap(), b"webview");
    assert!(new.join("backups/mevcut.txt").exists());
    assert_eq!(count_companies(&new.join(DB_FILE_NAME)).await, 1);
    assert!(new.join("versions/v1/s.db").exists());
}

/// WAL'da henüz checkpoint edilmemiş yazma: kapanış yapılmadan taşınır.
#[tokio::test]
async fn includes_uncheckpointed_wal_writes() {
    let root = tempfile::tempdir().unwrap();
    let old = root.path().join("old");
    let pool = init_pool(&old.join(DB_FILE_NAME)).await.unwrap();
    pool.close().await;
    let opts = SqliteConnectOptions::new()
        .filename(old.join(DB_FILE_NAME))
        .journal_mode(SqliteJournalMode::Wal)
        .pragma("wal_autocheckpoint", "0");
    let wal_pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await.unwrap();
    sqlx::query("INSERT INTO companies (name, address_text, created_at, updated_at) VALUES ('WAL satırı', 'a', 't', 't')")
        .execute(&wal_pool)
        .await
        .unwrap();
    assert!(old.join(format!("{DB_FILE_NAME}-wal")).metadata().unwrap().len() > 0);
    let new = root.path().join("new");

    migrate_legacy_data_dir(&old, &new).await.unwrap();

    assert_eq!(count_companies(&new.join(DB_FILE_NAME)).await, 1);
    wal_pool.close().await;
}
