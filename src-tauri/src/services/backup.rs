//! Veritabanı yedekleme ve geri yükleme.
//!
//! Her yedek `VACUUM INTO` ile alınır: SQLite WAL modunda çalışırken diskteki
//! ana dosyayı ham baytlarla kopyalamak (`std::fs::copy`) yarım yazılmış bir
//! kopya üretebilir, çünkü son değişiklikler `-wal` dosyasında bekliyor
//! olabilir. `VACUUM INTO` açık bağlantı üzerinden tutarlı, tek dosyalık bir
//! anlık görüntü üretir; WAL/SHM ile hiç uğraşmaz.
use std::path::{Path, PathBuf};

use chrono::{NaiveDate, NaiveDateTime};
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

/// Günlük otomatik yedeklerden kaç tanesinin saklanacağı. İki haftalık bir
/// geri dönüş penceresi (brief kararı): bozuk bir migration veya yanlış bir
/// toplu içe aktarım genelde aynı gün fark edilir, 14 gün cömert bir pay.
const AUTO_BACKUP_RETENTION_COUNT: usize = 14;

/// Geri yüklenecek dosya SQLite değilse veya beklenen tabloları
/// taşımıyorsa (ör. rastgele bir dosya seçildiyse) gösterilecek mesaj.
const INVALID_BACKUP_MESSAGE: &str = "Seçilen dosya geçerli bir MESNET yedeği değil.";
/// `PRAGMA integrity_check` "ok" dışında bir şey dönerse (veya dosya hiç
/// okunabilir bir SQLite veritabanı değilse) gösterilecek mesaj.
const CORRUPT_BACKUP_MESSAGE: &str = "Yedek dosyası bozuk.";
/// Yedeğin en yüksek migration sürümü, çalışan uygulamaya gömülü olandan
/// büyükse gösterilecek mesaj: yedek, henüz bilinmeyen bir şema taşıyor.
const NEWER_VERSION_MESSAGE: &str =
    "Bu yedek uygulamanın daha yeni bir sürümüyle alınmış; önce uygulamayı güncelleyin.";

/// Geçerli bir MESNET yedeğinde bulunması gereken tablolar. `_sqlx_migrations`
/// olmadan sürüm karşılaştırması yapılamaz; diğer dördü uygulamanın temel
/// verisidir (bkz. `db::init_pool` testindeki tam tablo listesi — burada
/// hepsini değil, doğrulama için yeterli olan bir alt kümeyi kontrol ederiz).
const REQUIRED_BACKUP_TABLES: [&str; 5] = ["_sqlx_migrations", "companies", "students", "users", "settings"];

/// `backup_status` komutunun döndüğü özet. Arayüz ayarlar ekranında gösterir.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupStatus {
    pub backup_dir: String,
    pub last_backup_at: Option<String>,
    pub backup_count: i64,
}

/// Otomatik günlük yedeklerin klasörü: uygulama veri dizininin altındaki
/// sabit `backups` alt klasörü. `db::init_pool` (açılış) ve
/// `commands::backup_commands` (kullanıcı komutları) AYNI yolu bu fonksiyon
/// üzerinden türetir; klasör adı iki yerde ayrı ayrı yazılmaz.
pub fn auto_backup_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("backups")
}

/// Bugünün otomatik yedeği yoksa `VACUUM INTO` ile alır, sonra eski
/// yedekleri budar. `today` parametre olarak alınır ki test gerçek takvim
/// gününe bağlı kalmadan "bugün" senaryosunu kursun.
pub async fn create_daily_backup_if_missing(pool: &SqlitePool, backup_dir: &Path, today: NaiveDate) -> AppResult<()> {
    std::fs::create_dir_all(backup_dir)?;
    let target = backup_dir.join(auto_backup_file_name(today));
    if target.exists() {
        return Ok(());
    }

    vacuum_into(pool, &target).await?;
    prune_old_backups(backup_dir, AUTO_BACKUP_RETENTION_COUNT)?;
    Ok(())
}

fn auto_backup_file_name(date: NaiveDate) -> String {
    format!("mesnet-lite-{}.db", date.format("%Y-%m-%d"))
}

/// Eski panodan tarihçeye tek seferlik aktarımdan (`services::legacy_reconcile`)
/// HEMEN ÖNCE alınan, günlük otomatik yedekten AYRI adlandırılmış bir yedek.
/// Aktarım gerçek veriyi (`company_term_hours`/`assignments`in içeriğini)
/// olay günlüğüne YAZDIĞI için, o gün zaten alınmış bir otomatik yedeği
/// (`create_daily_backup_if_missing`, dosya VARSA atlar) üzerine yazmaz —
/// ayrı bir dosya adı kullanır ki her zaman GERÇEKTEN alınmış olsun.
pub async fn create_pre_reconcile_backup(pool: &SqlitePool, backup_dir: &Path, today: NaiveDate) -> AppResult<()> {
    std::fs::create_dir_all(backup_dir)?;
    let target = backup_dir.join(format!("pre-legacy-reconcile-{}.db", today.format("%Y-%m-%d")));
    if target.exists() {
        return Ok(());
    }
    vacuum_into(pool, &target).await
}

/// Otomatik yedek klasörünün durumunu okur: yol, en yeni yedeğin tarihi,
/// toplam sayı. Dosya sistemine dokunur ama SQLite dosyalarını AÇMAZ —
/// yalnızca dosya adlarını ayrıştırır, bu yüzden senkron ve ucuzdur.
pub fn backup_status(backup_dir: &Path) -> AppResult<BackupStatus> {
    let mut backups = list_auto_backups(backup_dir)?;
    backups.sort_by_key(|(date, _)| *date);

    Ok(BackupStatus {
        backup_dir: backup_dir.to_string_lossy().to_string(),
        last_backup_at: backups.last().map(|(date, _)| date.format("%Y-%m-%d").to_string()),
        backup_count: backups.len() as i64,
    })
}

/// Klasördeki `mesnet-lite-YYYY-MM-DD.db` dosyalarını (tarihleriyle birlikte)
/// listeler. `pre-restore-*` gibi başka adlar bu deseni tutturamaz ve hem
/// durum özetine hem saklama budamasına karışmaz.
fn list_auto_backups(backup_dir: &Path) -> AppResult<Vec<(NaiveDate, PathBuf)>> {
    if !backup_dir.exists() {
        return Ok(Vec::new());
    }

    let mut found = Vec::new();
    for entry in std::fs::read_dir(backup_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(date) = parse_auto_backup_date(&name) {
            found.push((date, entry.path()));
        }
    }
    Ok(found)
}

fn parse_auto_backup_date(file_name: &str) -> Option<NaiveDate> {
    let stripped = file_name.strip_prefix("mesnet-lite-")?.strip_suffix(".db")?;
    NaiveDate::parse_from_str(stripped, "%Y-%m-%d").ok()
}

/// En yeni `keep` otomatik yedeği bırakır, gerisini siler. `pre-restore-*`
/// dosyaları `list_auto_backups`in deseninden geçmediği için hiç görülmez.
fn prune_old_backups(backup_dir: &Path, keep: usize) -> AppResult<()> {
    let mut backups = list_auto_backups(backup_dir)?;
    backups.sort_by_key(|(date, _)| std::cmp::Reverse(*date));

    for (_, path) in backups.into_iter().skip(keep) {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// Ham `VACUUM INTO` çağrısı: hedef dosya YOKSA çalışır, VARSA SQLite hata
/// verir. Bilinçli: günlük otomatik yedek ve geri-yükleme-öncesi yedek
/// çağrıcıları hedefin daha önce var OLMADIĞINI zaten garanti eder (tarih/
/// saat damgalı benzersiz ad); bir çakışma orada sessizce üstüne yazılacak
/// bir şey değil, araştırılması gereken gerçek bir hatadır.
async fn vacuum_into(pool: &SqlitePool, target: &Path) -> AppResult<()> {
    let target_str = target.to_string_lossy().to_string();
    sqlx::query("VACUUM INTO ?1")
        .bind(target_str)
        .execute(pool)
        .await
        .map_err(|e| AppError::Database(format!("Yedek alınamadı: {e}")))?;
    Ok(())
}

/// Kullanıcının elle aldığı yedek: hedefi Kaydet penceresinden kendisi
/// seçtiği için üzerine yazmayı zaten onaylamıştır, bu yüzden (günlük
/// otomatik yedeğin aksine) burada var olan hedef ÖNCE silinir.
pub async fn create_manual_backup(pool: &SqlitePool, target: &Path) -> AppResult<()> {
    if target.exists() {
        std::fs::remove_file(target)?;
    }
    vacuum_into(pool, target).await
}

/// Derlenmiş ikiliye gömülü migration'ların en yüksek sürüm numarası.
/// Geri yüklenecek yedeğin bundan daha yeni bir migration içermesi, yedeğin
/// uygulamanın DAHA YENİ bir sürümüyle alındığı ve bu sürümün henüz
/// bilmediği bir şema değişikliği taşıdığı anlamına gelir.
pub fn embedded_max_migration_version() -> i64 {
    sqlx::migrate!("./migrations")
        .migrations
        .iter()
        .map(|m| m.version)
        .max()
        .unwrap_or(0)
}

/// Seçilen dosyanın geçerli, bozulmamış ve bu uygulama sürümüyle uyumlu bir
/// MESNET yedeği olduğunu doğrular. ASIL havuza hiç dokunmaz: ayrı, salt
/// okunur bir bağlantı açar ve işi bitince kapatır — bu sayede doğrulama
/// başarısız olsa bile çalışan uygulamanın verisi asla riske girmez.
pub async fn validate_backup(path: &Path, embedded_max_version: i64) -> AppResult<()> {
    let pool = open_readonly(path).await?;
    let result = validate_opened_backup(&pool, embedded_max_version).await;
    pool.close().await;
    result
}

async fn validate_opened_backup(pool: &SqlitePool, embedded_max_version: i64) -> AppResult<()> {
    check_integrity(pool).await?;
    check_required_tables(pool).await?;
    check_migration_version(pool, embedded_max_version).await
}

async fn open_readonly(path: &Path) -> AppResult<SqlitePool> {
    let options = SqliteConnectOptions::new().filename(path).read_only(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|_| AppError::Validation(INVALID_BACKUP_MESSAGE.into()))
}

/// Not: sıfır bayt uzunluğunda bir dosya SQLite tarafından geçerli, BOŞ bir
/// veritabanı sayılır ve burada "ok" döner — tablosuzluğu asıl yakalayan
/// bir sonraki adım olan `check_required_tables`tir.
async fn check_integrity(pool: &SqlitePool) -> AppResult<()> {
    let rows: Result<Vec<String>, sqlx::Error> = sqlx::query_scalar("PRAGMA integrity_check").fetch_all(pool).await;

    match rows {
        Ok(rows) if rows.len() == 1 && rows[0] == "ok" => Ok(()),
        _ => Err(AppError::Validation(CORRUPT_BACKUP_MESSAGE.into())),
    }
}

async fn check_required_tables(pool: &SqlitePool) -> AppResult<()> {
    for table in REQUIRED_BACKUP_TABLES {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1")
            .bind(table)
            .fetch_one(pool)
            .await
            .map_err(|e| AppError::Database(format!("Yedek doğrulanamadı: {e}")))?;
        if count == 0 {
            return Err(AppError::Validation(INVALID_BACKUP_MESSAGE.into()));
        }
    }
    Ok(())
}

async fn check_migration_version(pool: &SqlitePool, embedded_max_version: i64) -> AppResult<()> {
    let backup_max: Option<i64> = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(pool)
        .await
        .map_err(|e| AppError::Database(format!("Yedek doğrulanamadı: {e}")))?;

    match backup_max {
        Some(version) if version > embedded_max_version => Err(AppError::Validation(NEWER_VERSION_MESSAGE.into())),
        _ => Ok(()),
    }
}

/// Doğrulanmış bir yedeği geri yükler: önce şimdiki veriyi
/// `pre-restore-<zaman damgası>.db` olarak yedekler, sonra havuzu kapatıp
/// DB dosyasını değiştirir. `pre_restore_at` parametre olarak alınır ki
/// dosya adı testte gerçek saate bağlı olmadan üretilebilsin.
///
/// Doğrulamadan SONRAKİ (`pre-restore` yedeğinden sonraki) bir hata, mesajda
/// bilerek `pre-restore` dosyasının yolunu taşır: kullanıcı verisi kaybolmuş
/// GİBİ görünmesin diye, onu nerede bulacağını hemen söyler.
pub async fn restore_from_backup(
    pool: &SqlitePool,
    db_path: &Path,
    backup_dir: &Path,
    source: &Path,
    pre_restore_at: NaiveDateTime,
) -> AppResult<()> {
    validate_backup(source, embedded_max_migration_version()).await?;

    std::fs::create_dir_all(backup_dir)?;
    let pre_restore_path = backup_dir.join(format!("pre-restore-{}.db", pre_restore_at.format("%Y-%m-%d-%H%M%S")));
    vacuum_into(pool, &pre_restore_path)
        .await
        .map_err(|e| AppError::Database(format!("Geri yükleme öncesi güvenlik yedeği alınamadı: {e}")))?;

    pool.close().await;

    replace_database_file(db_path, source).map_err(|e| {
        AppError::Io(format!(
            "Veritabanı dosyası değiştirilemedi: {e}. Önceki veriniz şu yedekte duruyor: {}",
            pre_restore_path.display()
        ))
    })
}

/// DB dosyasını `source` içeriğiyle değiştirir: önce dosyanın bulunduğu
/// klasörde geçici bir dosyaya kopyalanır, sonra ATOMİK `rename` ile hedefin
/// üstüne yazılır (kopyala-sonra-taşı: aynı dosya sisteminde `rename` tek
/// bir adımdır, bu yüzden işlem yarıda kesilse bile ya eski ya yeni dosya
/// bütün kalır, hiçbir zaman yarım bir dosya kalmaz). Eski `-wal`/`-shm`
/// dosyaları (artık geçersiz WAL kayıtları) varsa silinir.
///
/// `pub(super)`: `services::backup_tests` (kardeş modül) bu adımı `restore_from_backup`in
/// async/havuz/restart karmaşasından ayrı, doğrudan test edebilsin diye.
pub(super) fn replace_database_file(db_path: &Path, source: &Path) -> AppResult<()> {
    let parent = db_path.parent().ok_or_else(|| AppError::Io("Veritabanı klasörü belirlenemedi".into()))?;
    let file_name = db_path.file_name().and_then(|n| n.to_str()).unwrap_or(crate::db::DB_FILE_NAME);
    let temp_path = parent.join(format!("{file_name}.restoring-tmp"));

    std::fs::copy(source, &temp_path)?;
    std::fs::rename(&temp_path, db_path)?;

    remove_if_exists(&sidecar_path(db_path, "-wal"))?;
    remove_if_exists(&sidecar_path(db_path, "-shm"))?;

    Ok(())
}

fn sidecar_path(db_path: &Path, suffix: &str) -> PathBuf {
    let mut os_string = db_path.as_os_str().to_owned();
    os_string.push(suffix);
    PathBuf::from(os_string)
}

fn remove_if_exists(path: &Path) -> AppResult<()> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
