//! Göçten HEMEN ÖNCE alınan zorunlu yedek.
//!
//! Güncelleme sonrası açılışta bekleyen bir şema göçü varsa, günlük otomatik
//! yedek (`backup::create_daily_backup_if_missing`) yetmez: o gün yedek zaten
//! alınmışsa atlanır, yani gün içi girişler göçten önce yedeksiz kalır. Bu
//! yedek ayrı bir dosyadır, günlük yedeğin "bugün var mı" kontrolüne takılmaz
//! ve BAŞARISIZ olursa göç hiç başlamaz (hata çağırana döner).
//!
//! Dosya adı `pre-migration-v{eski}-v{yeni}-{YYYY-MM-DD-HHMMSS}.db`. Günlük
//! yedeğin deseni (`mesnet-lite-YYYY-MM-DD.db`) ile çakışmaz: `backup::
//! list_auto_backups` bu dosyaları görmez, dolayısıyla günlük 14'lük budama
//! onları silmez; bu yüzden kendi saklama sayısı vardır.

use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::services::backup::vacuum_into;

/// Saklanacak göç öncesi yedek sayısı. Göç yalnız sürüm yükseltmesinde olur
/// (ayda bir-iki kez); son 5 yedek birkaç sürümlük geri dönüş sağlar ve her
/// biri tam veritabanı kopyası olduğundan sınırsız büyümemeli.
const PRE_MIGRATION_RETENTION_COUNT: usize = 5;

const FILE_PREFIX: &str = "pre-migration-v";
const FILE_SUFFIX: &str = ".db";
/// Ad sonundaki zaman damgasının biçimi ve uzunluğu (`2026-09-30-123456`).
const TIMESTAMP_FORMAT: &str = "%Y-%m-%d-%H%M%S";
const TIMESTAMP_LEN: usize = 17;

/// Yedek başarısızsa kullanıcıya gösterilen mesaj: verinin değişmediğini ve
/// ne yapacağını söyler (göç bu noktada hiç başlamamıştır).
const BACKUP_FAILED_MESSAGE: &str = "Güncelleme sonrası veritabanı yükseltmesinden önce yedek alınamadı; veriniz değiştirilmedi. Disk alanını kontrol edip uygulamayı yeniden açın.";

/// Uygulanmış en yüksek göç sürümü. `_sqlx_migrations` tablosu yoksa `None`:
/// bu, ilk kurulum (ya da hiç göç görmemiş boş dosya) demektir ve yedeklenecek
/// bir şey yoktur. Yalnız başarıyla uygulananlar (`success = 1`) sayılır.
pub async fn applied_max_version(pool: &SqlitePool) -> AppResult<Option<i64>> {
    let table_exists: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'")
            .fetch_one(pool)
            .await?;
    if table_exists == 0 {
        return Ok(None);
    }
    let version: Option<i64> = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1")
        .fetch_one(pool)
        .await?;
    Ok(version)
}

fn file_name(from: i64, to: i64, now: NaiveDateTime) -> String {
    format!("{FILE_PREFIX}{from}-v{to}-{}{FILE_SUFFIX}", now.format(TIMESTAMP_FORMAT))
}

/// Bekleyen göç varsa (`applied < embedded_max`) yedeği alır ve yolunu döner;
/// yoksa `None`. Yedek alınamazsa Türkçe hata döner — çağıran göçü ÇALIŞTIRMAZ.
pub async fn backup_if_migration_pending(
    pool: &SqlitePool,
    backup_dir: &Path,
    embedded_max: i64,
    now: NaiveDateTime,
) -> AppResult<Option<PathBuf>> {
    let Some(applied) = applied_max_version(pool).await? else {
        return Ok(None);
    };
    if applied >= embedded_max {
        return Ok(None);
    }

    let target = backup_dir.join(file_name(applied, embedded_max, now));
    take_backup(pool, backup_dir, &target).await.map_err(|e| {
        eprintln!("Göç öncesi yedek alınamadı: {e}");
        AppError::Database(BACKUP_FAILED_MESSAGE.into())
    })?;

    // Yedek alındı; budama hatası göçü engellemez ama sessizce de yutulmaz.
    if let Err(e) = prune(backup_dir, PRE_MIGRATION_RETENTION_COUNT) {
        eprintln!("Eski göç öncesi yedekler budanamadı: {e}");
    }
    Ok(Some(target))
}

async fn take_backup(pool: &SqlitePool, backup_dir: &Path, target: &Path) -> AppResult<()> {
    std::fs::create_dir_all(backup_dir)?;
    vacuum_into(pool, target).await
}

/// Ad sonundaki zaman damgası; deseni tutturamayan dosya `None` (asla silinmez).
fn parse_timestamp(file_name: &str) -> Option<NaiveDateTime> {
    let stem = file_name.strip_prefix(FILE_PREFIX)?.strip_suffix(FILE_SUFFIX)?;
    let stamp = stem.get(stem.len().checked_sub(TIMESTAMP_LEN)?..)?;
    NaiveDateTime::parse_from_str(stamp, TIMESTAMP_FORMAT).ok()
}

/// En yeni `keep` göç öncesi yedeği bırakır. Sıralama ad içindeki zaman
/// damgasına göredir (sürüm numarasının basamak sayısı adı sözlüksel
/// sıralamayı bozar).
fn prune(backup_dir: &Path, keep: usize) -> AppResult<()> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(backup_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(stamp) = parse_timestamp(&name) {
            found.push((stamp, entry.path()));
        }
    }
    found.sort_by_key(|(stamp, _)| std::cmp::Reverse(*stamp));
    for (_, path) in found.into_iter().skip(keep) {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(second: u32) -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 9, 30).unwrap().and_hms_opt(12, 34, second).unwrap()
    }

    #[test]
    fn file_name_carries_both_versions_and_a_sortable_timestamp() {
        assert_eq!(file_name(9, 15, at(56)), "pre-migration-v9-v15-2026-09-30-123456.db");
    }

    #[test]
    fn prune_keeps_newest_five_and_never_touches_other_files() {
        let dir = tempfile::tempdir().unwrap();
        // Sürüm basamak sayısı değişse de (9 -> 10) sıralama zamana göre olmalı.
        for (i, (from, to)) in [(9, 10), (10, 11), (11, 12), (12, 13), (13, 14), (14, 15), (15, 16)].iter().enumerate() {
            std::fs::write(dir.path().join(file_name(*from, *to, at(i as u32))), b"x").unwrap();
        }
        std::fs::write(dir.path().join("mesnet-lite-2026-01-01.db"), b"gunluk").unwrap();
        std::fs::write(dir.path().join("pre-migration-v1-v2-bozuk.db"), b"ad bozuk").unwrap();

        prune(dir.path(), PRE_MIGRATION_RETENTION_COUNT).unwrap();

        let mut names: Vec<String> =
            std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
        names.sort();
        assert_eq!(names.len(), 7, "5 yedek + günlük + bozuk adlı dosya: {names:?}");
        assert!(!names.contains(&file_name(9, 10, at(0))), "en eski silinmeli");
        assert!(!names.contains(&file_name(10, 11, at(1))), "ikinci en eski silinmeli");
        assert!(names.contains(&file_name(15, 16, at(6))), "en yeni kalmalı");
        assert!(names.contains(&"mesnet-lite-2026-01-01.db".to_string()));
        assert!(names.contains(&"pre-migration-v1-v2-bozuk.db".to_string()));
    }
}
