//! Kayıtlı sürümler: veritabanının belirli bir andaki TAM kopyası (bkz.
//! `migrations/0013_versions.sql`). Kullanıcı kararı — önceki durumların
//! çıktısı, geçmişi yeniden oynatarak değil, o anın dosyasını saklayarak
//! alınır. Bir sürüm iki yolla oluşur: her başarılı dışa aktarımda
//! kendiliğinden (`kind = 'auto'`), ya da kullanıcının bir ad vererek elle
//! kaydetmesiyle (`kind = 'manual'`).
//!
//! Yedekleme altyapısı (`services::backup::vacuum_into`) burada YENİDEN
//! KULLANILIR: bir sürüm dosyası, tıpkı bir yedek gibi, açık bağlantı
//! üzerinden `VACUUM INTO` ile tek dosyalık tutarlı bir anlık görüntüdür.

use std::path::{Path, PathBuf};

use chrono::{NaiveDate, NaiveDateTime};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use tempfile::TempDir;

use crate::db::read_at::ReadAt;
use crate::error::{AppError, AppResult};

/// Sürüm adının izin verilen azami uzunluğu (brief kararı): listede taşmayan,
/// tek satırlık bir etiket için 80 karakter yeterlidir.
const MAX_VERSION_NAME_LENGTH: usize = 80;
/// `created_at` sütununun biçimi: Türkiye yerel saatiyle, saniye çözünürlüklü.
const CREATED_AT_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";
/// `as_of` sütununun ve `ReadAt::resolve`e verilen değerin biçimi.
const AS_OF_DATE_FORMAT: &str = "%Y-%m-%d";
/// Otomatik sürüm adına eklenen tarih ekinin biçimi (kullanıcıya dönük, Türkçe
/// gün.ay.yıl sırası) — brief kararı: " (10.09.2026 itibarıyla)" gibi.
const AS_OF_DISPLAY_FORMAT: &str = "%d.%m.%Y";
/// Parmak izi hesaplanırken atlanan tablolar: `versions` kendi kendine
/// referans olur (bir sürüm alma işlemi kendi parmak izini değiştiremez);
/// `_sqlx_migrations` şema sürümünü taşır, kullanıcı verisi değildir.
const FINGERPRINT_EXCLUDED_TABLES: [&str; 2] = ["versions", "_sqlx_migrations"];

/// Bir sürümün türü. `kind` sütununda `'auto'`/`'manual'` olarak saklanır.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionKind {
    Auto,
    Manual,
}

impl VersionKind {
    fn as_db_str(self) -> &'static str {
        match self {
            VersionKind::Auto => "auto",
            VersionKind::Manual => "manual",
        }
    }

    /// `CHECK (kind IN ('auto','manual'))` şemada garanti ettiği için bu
    /// yalnızca bozuk/eski bir satırla karşılaşılırsa hata döner.
    fn parse_db_str(raw: &str) -> AppResult<Self> {
        match raw {
            "auto" => Ok(VersionKind::Auto),
            "manual" => Ok(VersionKind::Manual),
            other => Err(AppError::Database(format!("Bilinmeyen sürüm türü: '{other}'"))),
        }
    }
}

/// `list_versions`/`create_version` komutlarının döndüğü, arayüze giden özet.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: i64,
    pub name: String,
    pub kind: VersionKind,
    pub trigger: Option<String>,
    pub term: String,
    pub created_at: String,
    /// Sürümün hangi tarihe göre üretildiği (`YYYY-MM-DD`); `None` ise
    /// `Latest` (bkz. `db::read_at::ReadAt`). Dışa aktarım komutları bu alanı,
    /// isteğinde `asOf` verilmemişse hangi tarihi kullanacağını bulmak için
    /// okur (brief madde 2).
    pub as_of: Option<String>,
    /// Sürüm dosyası diskte hâlâ var mı. Elle silinmiş (veya taşınmış) bir
    /// dosyanın satırı listede kalır ama bu alan `false` olur — kullanıcı
    /// açmayı denemeden önce durumu görür.
    pub is_available: bool,
}

/// Ham veritabanı satırı. `fingerprint`, yalnız otomatik sürümde önceki
/// sürümle karşılaştırmak için okunur; `Version`e taşınmaz.
#[derive(Debug, sqlx::FromRow)]
struct VersionRow {
    id: i64,
    name: String,
    kind: String,
    trigger: Option<String>,
    term: String,
    file_name: String,
    fingerprint: String,
    created_at: String,
    as_of: Option<String>,
}

impl VersionRow {
    fn into_version(self, versions_dir: &Path) -> AppResult<Version> {
        let is_available = versions_dir.join(&self.file_name).exists();
        Ok(Version {
            id: self.id,
            name: self.name,
            kind: VersionKind::parse_db_str(&self.kind)?,
            trigger: self.trigger,
            term: self.term,
            created_at: self.created_at,
            as_of: self.as_of,
            is_available,
        })
    }
}

/// Sürümlerin saklandığı klasör: uygulama veri dizininin altındaki sabit
/// `versions` alt klasörü (bkz. `services::backup::auto_backup_dir`teki aynı
/// desen — yol tek yerde türetilir, komut katmanı bunu import eder).
pub fn versions_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("versions")
}

/// Elle veya otomatik bir sürüm alır. `as_of`, sürümün hangi güne göre
/// üretildiğidir (`ReadAt::resolve` ile doğrulanır — dönem dışı/bozuk tarih
/// `Validation` döner); `None` ise sürüm `Latest` olarak işaretlenir.
///
/// **Otomatik ve tekrar:** parmak izi VE `as_of` son sürümünkiyle AYNIYSA
/// yeni dosya AÇILMAZ, son sürüm olduğu gibi döner — kullanıcı kararı: aynı
/// veriden aynı tarih için art arda aynı çıktı alınması disk dolduran ayırt
/// etmeyen kopyalar biriktirmemeli. Farklı bir `as_of` ise (veri aynı olsa
/// bile) yeni bir kopya açılır: geçmiş tarihli bir çıktının dayanağı, başka
/// bir tarihin dayanağıyla PAYLAŞILAMAZ. **Elle kayıt** bu kısayolu asla
/// kullanmaz: kullanıcı bir ada bilinçle basmışsa, veri değişmemiş olsa bile
/// o an bir sürüm istemiştir.
pub async fn create_version(
    pool: &SqlitePool,
    versions_dir: &Path,
    name: &str,
    kind: VersionKind,
    trigger: Option<&str>,
    as_of: Option<String>,
    now: NaiveDateTime,
) -> AppResult<Version> {
    let name = validate_version_name(name)?;
    let term = crate::db::settings::get_active_term(pool).await?;
    let as_of_str = resolve_as_of_string(pool, &term, as_of).await?;
    let fingerprint = compute_fingerprint(pool).await?;

    if kind == VersionKind::Auto {
        if let Some(latest) = fetch_latest_version_row(pool).await? {
            if latest.fingerprint == fingerprint && latest.as_of == as_of_str {
                return latest.into_version(versions_dir);
            }
        }
    }

    std::fs::create_dir_all(versions_dir)?;
    let file_name = random_file_name(pool).await?;
    let target = versions_dir.join(&file_name);
    crate::services::backup::vacuum_into(pool, &target).await?;

    let created_at = now.format(CREATED_AT_FORMAT).to_string();
    let id = insert_version_row(
        pool,
        &name,
        kind,
        trigger,
        &term,
        &file_name,
        &fingerprint,
        &created_at,
        as_of_str.as_deref(),
    )
    .await?;

    Ok(Version {
        id,
        name,
        kind,
        trigger: trigger.map(str::to_string),
        term,
        created_at,
        as_of: as_of_str,
        is_available: true,
    })
}

/// `as_of`'u `ReadAt::resolve` ile doğrular (dönem dışı/bozuk tarih burada
/// elenir) ve DB'ye yazılacak kanonik `YYYY-MM-DD` dizgesine çevirir.
/// Doğrulama TEK yerde (`ReadAt::resolve`) yaşar; burada tekrarlanmaz.
async fn resolve_as_of_string(pool: &SqlitePool, term: &str, as_of: Option<String>) -> AppResult<Option<String>> {
    let read_at = ReadAt::resolve(pool, term, as_of).await?;
    Ok(read_at.value().map(|d| d.format(AS_OF_DATE_FORMAT).to_string()))
}

fn validate_version_name(name: &str) -> AppResult<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("Sürüm adı boş olamaz.".into()));
    }
    if trimmed.chars().count() > MAX_VERSION_NAME_LENGTH {
        return Err(AppError::Validation(format!(
            "Sürüm adı en fazla {MAX_VERSION_NAME_LENGTH} karakter olabilir."
        )));
    }
    Ok(trimmed.to_string())
}

/// SQLite'ın kendi rastgele üretecinden (`randomblob`) türetilmiş, dış bir
/// rastgelelik kütüphanesi gerekmeyen bir dosya adı. Zaman damgası yerine
/// rastgelelik kullanılır çünkü iki sürüm (ör. bir otomatik, hemen ardından
/// bir elle kayıt) aynı `now` anına denk gelebilir — dosya adı zamana değil,
/// rastgeleliğe bağlıysa asla çakışmaz.
async fn random_file_name(pool: &SqlitePool) -> AppResult<String> {
    let suffix: String = sqlx::query_scalar("SELECT lower(hex(randomblob(8)))")
        .fetch_one(pool)
        .await
        .map_err(|e| AppError::Database(format!("Sürüm dosya adı üretilemedi: {e}")))?;
    Ok(format!("version-{suffix}.db"))
}

#[allow(clippy::too_many_arguments)]
async fn insert_version_row(
    pool: &SqlitePool,
    name: &str,
    kind: VersionKind,
    trigger: Option<&str>,
    term: &str,
    file_name: &str,
    fingerprint: &str,
    created_at: &str,
    as_of: Option<&str>,
) -> AppResult<i64> {
    sqlx::query_scalar(
        "INSERT INTO versions (name, kind, trigger, term, file_name, fingerprint, created_at, as_of)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         RETURNING id",
    )
    .bind(name)
    .bind(kind.as_db_str())
    .bind(trigger)
    .bind(term)
    .bind(file_name)
    .bind(fingerprint)
    .bind(created_at)
    .bind(as_of)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::Database(format!("Sürüm satırı eklenemedi: {e}")))
}

async fn fetch_latest_version_row(pool: &SqlitePool) -> AppResult<Option<VersionRow>> {
    sqlx::query_as::<_, VersionRow>(
        "SELECT id, name, kind, trigger, term, file_name, fingerprint, created_at, as_of
         FROM versions ORDER BY id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("Sürümler okunamadı: {e}")))
}

/// En yeniden eskiye sürüm listesi. `id` artan sırayla eklendiği için (her
/// zaman en sona eklenir) `ORDER BY id DESC` = ekleme sırasının tersi.
pub async fn list_versions(pool: &SqlitePool, versions_dir: &Path) -> AppResult<Vec<Version>> {
    let rows: Vec<VersionRow> = sqlx::query_as(
        "SELECT id, name, kind, trigger, term, file_name, fingerprint, created_at, as_of
         FROM versions ORDER BY id DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("Sürümler okunamadı: {e}")))?;

    rows.into_iter().map(|row| row.into_version(versions_dir)).collect()
}

/// Sürümü siler: dosya diskte hâlâ varsa önce dosya, sonra satır. Dosya zaten
/// yoksa (elle silinmiş/taşınmış) yalnızca satır silinir — kullanıcının
/// listeden kaldırmak istediği açık niyeti, dosyanın durumuna bağlı değildir.
pub async fn delete_version(pool: &SqlitePool, versions_dir: &Path, id: i64) -> AppResult<()> {
    let file_name: Option<String> = sqlx::query_scalar("SELECT file_name FROM versions WHERE id = ?1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Database(format!("Sürüm okunamadı: {e}")))?;

    let Some(file_name) = file_name else {
        return Err(AppError::NotFound(format!("Sürüm bulunamadı: {id}")));
    };

    let path = versions_dir.join(&file_name);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    sqlx::query("DELETE FROM versions WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| AppError::Database(format!("Sürüm satırı silinemedi: {e}")))?;

    Ok(())
}

/// Bir sürüm dosyasını GEÇİCİ bir kopyaya açar ve o kopya üzerinde
/// `db::init_pool`ı çalıştırır — gelecekteki göçler ve açılış tamamlamaları
/// (ör. `backfill_missing_term_rows`) eski kopyayı da yükseltsin diye.
/// Orijinal sürüm dosyasına ASLA yazılmaz: yalnızca bu kopya değişir, o da
/// çağıranın döndürdüğü `TempDir` düşünce diskten silinir.
pub async fn open_version(versions_dir: &Path, file_name: &str) -> AppResult<(TempDir, SqlitePool)> {
    let source = versions_dir.join(file_name);
    if !source.exists() {
        return Err(AppError::Validation(format!(
            "Sürüm dosyası bulunamadı: '{file_name}'. Sürüm silinmiş olabilir."
        )));
    }

    let temp_dir = tempfile::tempdir().map_err(|e| AppError::Io(format!("Geçici dizin oluşturulamadı: {e}")))?;
    let temp_db_path = temp_dir.path().join(crate::db::DB_FILE_NAME);
    std::fs::copy(&source, &temp_db_path)?;

    let pool = crate::db::init_pool(&temp_db_path).await?;
    Ok((temp_dir, pool))
}

/// Beş dışa aktarım komutunun ortak `versionId` yolu: seçilen sürümü açar,
/// üreticiye vereceği havuzu, o kopyanın aktif dönemini ve sürümün kayıtlı
/// `as_of`'unu döner. Sürüm bulunamazsa ya da dosyası kayıpsa Türkçe
/// `Validation` döner (brief madde 4). Dönen `as_of`, isteğin kendi `asOf`'u
/// verilmemişse hangi tarihin kullanılacağını belirler (brief madde 2).
///
/// `TempDir` çağıranın elinde kalmalı: havuz kapatılıp bu değer düşürülene
/// kadar geçici dosya silinmez (bkz. `open_version`).
pub async fn open_version_for_export(
    pool: &SqlitePool,
    versions_dir: &Path,
    version_id: i64,
) -> AppResult<(TempDir, SqlitePool, String, Option<String>)> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT file_name, as_of FROM versions WHERE id = ?1")
            .bind(version_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| AppError::Database(format!("Sürüm okunamadı: {e}")))?;

    let Some((file_name, as_of)) = row else {
        return Err(AppError::Validation(format!("Seçilen sürüm ({version_id}) bulunamadı.")));
    };

    let (temp_dir, version_pool) = open_version(versions_dir, &file_name).await?;
    let term = crate::db::settings::get_active_term(&version_pool).await?;
    Ok((temp_dir, version_pool, term, as_of))
}

/// Güncel veritabanından bir çıktı üretildikten SONRA çağrılır: otomatik bir
/// sürüm almayı dener. Kullanıcı kararı — sürüm ALINAMAZSA çıktı hiç
/// döndürülmez, çünkü resmi bir evrakın (çizelge/tutanak/Excel) dayanağı
/// olan anlık görüntü kaybolmamalı.
///
/// `as_of`, çıktının üretildiği `ReadAt::value()`dir. Doluysa sürüm adına
/// " (dd.MM.yyyy itibarıyla)" eki eklenir (brief madde 3) — listede hangi
/// otomatik sürümün hangi tarihe baktığı adından anlaşılsın diye.
pub async fn record_auto_version(
    pool: &SqlitePool,
    versions_dir: &Path,
    trigger: &str,
    name: &str,
    as_of: Option<NaiveDate>,
) -> AppResult<()> {
    let display_name = match as_of {
        Some(date) => format!("{name} ({} itibarıyla)", date.format(AS_OF_DISPLAY_FORMAT)),
        None => name.to_string(),
    };
    let as_of_str = as_of.map(|date| date.format(AS_OF_DATE_FORMAT).to_string());

    create_version(
        pool,
        versions_dir,
        &display_name,
        VersionKind::Auto,
        Some(trigger),
        as_of_str,
        crate::domain::terms::now_local(),
    )
    .await
    .map(|_| ())
    .map_err(|e| AppError::Validation(format!("Sürüm kaydedilemediği için çıktı verilmedi: {e}")))
}

/// `versions`/`_sqlx_migrations` dışındaki tabloların deterministik içerik
/// özeti (SHA-256): tablo adına, sonra rowid'e göre sıralı. `quote()` SQLite
/// fonksiyonu her sütun değerini (NULL/INTEGER/REAL/TEXT ayrımını koruyarak)
/// kendi kanonik SQL metin gösterimine çevirir — Rust tarafında sütun tipine
/// göre dallanmaya gerek kalmaz.
async fn compute_fingerprint(pool: &SqlitePool) -> AppResult<String> {
    let tables = user_table_names(pool).await?;

    let mut hasher = Sha256::new();
    for table in tables {
        hasher.update(table.as_bytes());
        hasher.update(b"\n");
        for row_text in table_rows_as_text(pool, &table).await? {
            hasher.update(row_text.as_bytes());
            hasher.update(b"\n");
        }
    }

    Ok(hex::encode(hasher.finalize()))
}

/// `versions`/`_sqlx_migrations` ve SQLite'ın kendi iç tabloları (`sqlite_%`,
/// ör. `AUTOINCREMENT`ın `sqlite_sequence`si) dışındaki tablolar, ada göre sıralı.
async fn user_table_names(pool: &SqlitePool) -> AppResult<Vec<String>> {
    let placeholders = FINGERPRINT_EXCLUDED_TABLES.map(|_| "?").join(", ");
    let sql = format!(
        "SELECT name FROM sqlite_master
         WHERE type = 'table' AND name NOT IN ({placeholders}) AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'
         ORDER BY name"
    );

    let mut query = sqlx::query_scalar(&sql);
    for table in FINGERPRINT_EXCLUDED_TABLES {
        query = query.bind(table);
    }

    query
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("Parmak izi için tablo listesi okunamadı: {e}")))
}

/// Bir tablonun tüm satırlarını, sütunları `|` ile ayrılmış tek bir metne
/// dönüştürerek döner. Tablo/sütun adları yalnızca `sqlite_master`/
/// `PRAGMA table_info`den (uygulamanın kendi şeması) geldiği için — dışarıdan
/// gelen serbest metin değil — burada `format!` ile gömülmeleri güvenlidir.
async fn table_rows_as_text(pool: &SqlitePool, table: &str) -> AppResult<Vec<String>> {
    let columns = table_column_names(pool, table).await?;
    let quoted_columns: Vec<String> = columns.iter().map(|c| format!("quote(\"{c}\")")).collect();
    let sql = format!("SELECT {} FROM \"{table}\" ORDER BY rowid", quoted_columns.join(" || '|' || "));

    sqlx::query_scalar(&sql)
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("Parmak izi için '{table}' okunamadı: {e}")))
}

async fn table_column_names(pool: &SqlitePool, table: &str) -> AppResult<Vec<String>> {
    let sql = format!("PRAGMA table_info(\"{table}\")");
    let rows = sqlx::query(&sql)
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("'{table}' şeması okunamadı: {e}")))?;

    rows.iter()
        .map(|row| {
            row.try_get::<String, _>("name")
                .map_err(|e| AppError::Database(format!("'{table}' şeması okunamadı: {e}")))
        })
        .collect()
}
