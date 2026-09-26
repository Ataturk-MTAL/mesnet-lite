//! `backup` testleri: yedeğin gerçekten açılabilir olduğu, günlük otomatik
//! yedeğin bugün içinde tekrarlanmadığı ve eskileri budadığı, doğrulamanın
//! dört ret/kabul senaryosu ve dosya değiştirme adımının WAL/SHM'i temizlediği.

use std::path::Path;

use chrono::{NaiveDate, NaiveDateTime};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use super::backup::{
    auto_backup_dir, backup_status, create_daily_backup_if_missing, create_manual_backup, create_pre_reconcile_backup,
    embedded_max_migration_version, replace_database_file, restore_from_backup, validate_backup,
};
use crate::db::init_pool;
use crate::error::AppError;

async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
    (dir, pool)
}

async fn insert_company(pool: &SqlitePool, name: &str) {
    sqlx::query("INSERT INTO companies (name, address_text, created_at, updated_at) VALUES (?1, 'adres', 't', 't')")
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
}

async fn open_plain(path: &Path) -> SqlitePool {
    let options = SqliteConnectOptions::new().filename(path);
    SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap()
}

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

/// Yedek, kaynakla aynı satırları içeren, GERÇEKTEN açılabilir bir DB
/// üretmeli — `VACUUM INTO`nun yalnızca "hata vermedi" değil, okunabilir
/// bir kopya ürettiğini doğrular.
#[tokio::test]
async fn manual_backup_produces_openable_db_with_same_rows() {
    let (dir, pool) = test_pool().await;
    insert_company(&pool, "Test A.Ş.").await;

    let target = dir.path().join("yedek.db");
    create_manual_backup(&pool, &target).await.unwrap();

    let backup_pool = open_plain(&target).await;
    let name: String = sqlx::query_scalar("SELECT name FROM companies WHERE name = 'Test A.Ş.'")
        .fetch_one(&backup_pool)
        .await
        .unwrap();
    assert_eq!(name, "Test A.Ş.");
    backup_pool.close().await;
}

/// Elle yedekte hedef zaten varsa (kullanıcı Kaydet penceresinde üzerine
/// yazmayı zaten onayladı), eski içerik SİLİNİP yeni yedek yazılmalı.
#[tokio::test]
async fn manual_backup_overwrites_existing_target_file() {
    let (dir, pool) = test_pool().await;
    let target = dir.path().join("yedek.db");
    std::fs::write(&target, b"eski, alakasiz bir dosya").unwrap();

    create_manual_backup(&pool, &target).await.unwrap();

    let backup_pool = open_plain(&target).await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM companies").fetch_one(&backup_pool).await.unwrap();
    assert_eq!(count, 0);
    backup_pool.close().await;
}

/// Bugünün yedeği zaten varsa ikinci çağrı yeniden almaz: birinci çağrıdan
/// SONRA eklenen satır, ikinci çağrı sonucundaki dosyada YER ALMAMALI.
#[tokio::test]
async fn daily_backup_is_not_retaken_if_todays_backup_exists() {
    let (dir, pool) = test_pool().await;
    let backup_dir = dir.path().join("backups");
    let today = ymd(2026, 9, 22);

    create_daily_backup_if_missing(&pool, &backup_dir, today).await.unwrap();
    insert_company(&pool, "İkinci Çağrıdan Önce Eklendi").await;
    create_daily_backup_if_missing(&pool, &backup_dir, today).await.unwrap();

    let target = backup_dir.join("mesnet-lite-2026-09-22.db");
    let backup_pool = open_plain(&target).await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM companies").fetch_one(&backup_pool).await.unwrap();
    assert_eq!(count, 0, "ikinci çağrı yedeği yeniden ALMAMALIYDI");
    backup_pool.close().await;
}

/// Eski pano → tarihçe aktarımının yedeği (`services::legacy_reconcile`),
/// günlük otomatik yedekle AYNI klasörde ama AYRI bir adla oturur; ikisi
/// birbirinin dosyasını etkilemez.
#[tokio::test]
async fn pre_reconcile_backup_writes_a_separate_file_from_the_daily_backup() {
    let (dir, pool) = test_pool().await;
    let backup_dir = dir.path().join("backups");
    let today = ymd(2026, 9, 22);

    create_daily_backup_if_missing(&pool, &backup_dir, today).await.unwrap();
    create_pre_reconcile_backup(&pool, &backup_dir, today).await.unwrap();

    assert!(backup_dir.join("mesnet-lite-2026-09-22.db").exists());
    assert!(backup_dir.join("pre-legacy-reconcile-2026-09-22.db").exists());
}

/// Aynı gün ikinci çağrı hedef dosya zaten VARSA yeniden yazmaz — aktarım
/// başarısız olup uygulama yeniden başlatılırsa, ilk (bozulmamış) yedek korunur.
#[tokio::test]
async fn pre_reconcile_backup_is_not_retaken_if_todays_backup_exists() {
    let (dir, pool) = test_pool().await;
    let backup_dir = dir.path().join("backups");
    let today = ymd(2026, 9, 22);

    create_pre_reconcile_backup(&pool, &backup_dir, today).await.unwrap();
    insert_company(&pool, "İkinci Çağrıdan Önce Eklendi").await;
    create_pre_reconcile_backup(&pool, &backup_dir, today).await.unwrap();

    let target = backup_dir.join("pre-legacy-reconcile-2026-09-22.db");
    let backup_pool = open_plain(&target).await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM companies").fetch_one(&backup_pool).await.unwrap();
    assert_eq!(count, 0, "ikinci çağrı yedeği yeniden ALMAMALIYDI");
    backup_pool.close().await;
}

/// DB dosyası hiç yoksa (ilk kurulum) otomatik yedek alınmaz. `db::init_pool`
/// bu kontrolü havuz açılmadan önce yapar; burada aynı kuralı doğrudan
/// `create_daily_backup_if_missing`in ÇAĞRILMAMASI gerektiği senaryosuyla
/// değil, `init_pool`un GERÇEK akışıyla doğruluyoruz (bkz. `db::tests`).
///
/// 16 eski yedek + bugünün yeni yedeği varken en yeni 14 kalmalı;
/// `pre-restore-*` dosyalarına hiç dokunulmamalı.
#[tokio::test]
async fn daily_backup_prunes_to_newest_14_and_ignores_pre_restore_files() {
    let (dir, pool) = test_pool().await;
    let backup_dir = dir.path().join("backups");
    std::fs::create_dir_all(&backup_dir).unwrap();

    let pre_restore = backup_dir.join("pre-restore-2026-01-01-120000.db");
    std::fs::write(&pre_restore, b"dokunulmamali").unwrap();

    // 16 gün boyunca art arda otomatik yedek al: 1 Eylül .. 16 Eylül.
    for day in 1..=16u32 {
        let today = ymd(2026, 9, day);
        create_daily_backup_if_missing(&pool, &backup_dir, today).await.unwrap();
    }

    let status = backup_status(&backup_dir).unwrap();
    assert_eq!(status.backup_count, 14, "yalnızca en yeni 14 otomatik yedek kalmalı");
    assert_eq!(status.last_backup_at, Some("2026-09-16".to_string()));

    // En eski ikisi (1 ve 2 Eylül) silinmiş olmalı, en yenisi (16 Eylül) kalmalı.
    assert!(!backup_dir.join("mesnet-lite-2026-09-01.db").exists());
    assert!(!backup_dir.join("mesnet-lite-2026-09-02.db").exists());
    assert!(backup_dir.join("mesnet-lite-2026-09-16.db").exists());

    // pre-restore-* dosyası budamadan hiç etkilenmemeli.
    assert!(pre_restore.exists());
}

/// `backup_status`: boş klasörde sayı 0, tarih `None`.
#[tokio::test]
async fn backup_status_reports_zero_for_empty_dir() {
    let dir = tempfile::tempdir().unwrap();
    let backup_dir = auto_backup_dir(dir.path());

    let status = backup_status(&backup_dir).unwrap();
    assert_eq!(status.backup_count, 0);
    assert_eq!(status.last_backup_at, None);
    assert_eq!(status.backup_dir, backup_dir.to_string_lossy().to_string());
}

/// Rastgele bayt içeren bir dosya (geçerli bir SQLite başlığı taşımıyor)
/// doğrulamadan reddedilmeli.
#[tokio::test]
async fn validate_backup_rejects_random_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bozuk.db");
    std::fs::write(&path, [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x11, 0x22, 0x33]).unwrap();

    let result = validate_backup(&path, embedded_max_migration_version()).await;
    assert!(matches!(result, Err(AppError::Validation(_))));
}

/// Tablosuz (boş) bir SQLite dosyası, teknik olarak açılabilir/bozulmamış
/// olsa da MESNET yedeği değildir; reddedilmeli.
#[tokio::test]
async fn validate_backup_rejects_empty_sqlite_without_tables() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bos.db");
    let options = SqliteConnectOptions::new().filename(&path).create_if_missing(true);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
    pool.close().await;

    let result = validate_backup(&path, embedded_max_migration_version()).await;
    assert!(matches!(result, Err(AppError::Validation(_))));
}

/// Yedeğin en yüksek migration sürümü, çalışan uygulamanın gömülü sürümünden
/// büyükse reddedilmeli (yedek uygulamanın DAHA YENİ bir sürümüyle alınmış).
#[tokio::test]
async fn validate_backup_rejects_newer_migration_version() {
    let (dir, pool) = test_pool().await;
    sqlx::query("INSERT INTO _sqlx_migrations (version, description, installed_on, success, checksum, execution_time) VALUES (?1, 'gelecek', datetime('now'), 1, x'00', 0)")
        .bind(embedded_max_migration_version() + 1000)
        .execute(&pool)
        .await
        .unwrap();

    let target = dir.path().join("gelecek.db");
    create_manual_backup(&pool, &target).await.unwrap();

    let result = validate_backup(&target, embedded_max_migration_version()).await;
    assert!(matches!(result, Err(AppError::Validation(_))));
}

/// Gerçek bir uygulama yedeği (tam şema, güncel sürüm) kabul edilmeli.
#[tokio::test]
async fn validate_backup_accepts_valid_backup() {
    let (dir, pool) = test_pool().await;
    let target = dir.path().join("gecerli.db");
    create_manual_backup(&pool, &target).await.unwrap();

    validate_backup(&target, embedded_max_migration_version()).await.unwrap();
}

/// Değiştirme adımı: hedef dosya yedeğin içeriğini almalı, eski `-wal`/`-shm`
/// dosyaları silinmeli. Bu, `restore_from_backup`in async/havuz/restart
/// karmaşasından bağımsız, saf dosya-sistemi mantığıdır.
#[test]
fn replace_database_file_moves_backup_content_and_removes_wal_shm() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("mesnet-lite.db");
    let source = dir.path().join("kaynak.db");
    std::fs::write(&db_path, b"eski veri").unwrap();
    std::fs::write(&source, b"yeni veri").unwrap();
    std::fs::write(format!("{}-wal", db_path.display()), b"eski wal").unwrap();
    std::fs::write(format!("{}-shm", db_path.display()), b"eski shm").unwrap();

    replace_database_file(&db_path, &source).unwrap();

    assert_eq!(std::fs::read(&db_path).unwrap(), b"yeni veri");
    assert!(!Path::new(&format!("{}-wal", db_path.display())).exists());
    assert!(!Path::new(&format!("{}-shm", db_path.display())).exists());
}

/// Uçtan uca: doğrulanmış bir yedek geri yüklenince DB dosyası değişir VE
/// önceki veri `pre-restore-*` altında bulunabilir kalır.
#[tokio::test]
async fn restore_from_backup_replaces_db_and_keeps_pre_restore_copy() {
    let (dir, pool) = test_pool().await;
    insert_company(&pool, "Eski Veri").await;

    let db_path = dir.path().join("mesnet-lite.db");
    let backup_dir = dir.path().join("backups");

    // Geri yüklenecek "yeni" yedek: farklı bir kaynak veritabanı.
    let (source_dir, source_pool) = test_pool().await;
    insert_company(&source_pool, "Yeni Veri").await;
    let source_backup = source_dir.path().join("kaynak-yedek.db");
    create_manual_backup(&source_pool, &source_backup).await.unwrap();
    source_pool.close().await;

    let pre_restore_at = NaiveDateTime::parse_from_str("2026-09-22 10:30:00", "%Y-%m-%d %H:%M:%S").unwrap();
    restore_from_backup(&pool, &db_path, &backup_dir, &source_backup, pre_restore_at).await.unwrap();

    let restored_pool = open_plain(&db_path).await;
    let name: String = sqlx::query_scalar("SELECT name FROM companies").fetch_one(&restored_pool).await.unwrap();
    assert_eq!(name, "Yeni Veri");
    restored_pool.close().await;

    let pre_restore_path = backup_dir.join("pre-restore-2026-09-22-103000.db");
    assert!(pre_restore_path.exists());
    let pre_restore_pool = open_plain(&pre_restore_path).await;
    let old_name: String = sqlx::query_scalar("SELECT name FROM companies").fetch_one(&pre_restore_pool).await.unwrap();
    assert_eq!(old_name, "Eski Veri");
    pre_restore_pool.close().await;
}

/// Geçersiz bir yedekle geri yükleme denemesi, DB dosyasına HİÇ
/// dokunmadan reddedilmeli (doğrulama, pre-restore yedeğinden ÖNCE gelir).
#[tokio::test]
async fn restore_from_backup_rejects_invalid_source_without_touching_db() {
    let (dir, pool) = test_pool().await;
    insert_company(&pool, "Dokunulmamali").await;

    let db_path = dir.path().join("mesnet-lite.db");
    let backup_dir = dir.path().join("backups");
    let bad_source = dir.path().join("bozuk.db");
    std::fs::write(&bad_source, b"gecersiz").unwrap();

    let pre_restore_at = NaiveDateTime::parse_from_str("2026-09-22 10:30:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let result = restore_from_backup(&pool, &db_path, &backup_dir, &bad_source, pre_restore_at).await;

    assert!(matches!(result, Err(AppError::Validation(_))));
    // Havuz hâlâ açık ve veri hâlâ yerinde olmalı: hiçbir şey değişmemiş.
    let name: String = sqlx::query_scalar("SELECT name FROM companies").fetch_one(&pool).await.unwrap();
    assert_eq!(name, "Dokunulmamali");
}
