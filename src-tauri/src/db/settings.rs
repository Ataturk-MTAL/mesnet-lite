use crate::error::AppResult;
use sqlx::SqlitePool;
use std::collections::BTreeMap;

/// Tüm ayarları anahtar/değer haritası olarak döner.
pub async fn get_all(pool: &SqlitePool) -> AppResult<BTreeMap<String, String>> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().collect())
}

/// Tek bir ayarı okur. Anahtar yoksa None döner; varsayılan uydurulmaz.
pub async fn get(pool: &SqlitePool, key: &str) -> AppResult<Option<String>> {
    let value: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?1")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(value)
}

/// Ayarı yazar; anahtar yoksa oluşturur.
pub async fn set(pool: &SqlitePool, key: &str, value: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// Birden çok ayarı tek işlemde yazar. Biri düşerse hiçbiri yazılmaz.
pub async fn set_many(pool: &SqlitePool, entries: &BTreeMap<String, String>) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    for (key, value) in entries {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Okul konumu. İkisi birden dolu değilse None döner.
/// Bu değer mesafe hesabında KULLANILMAZ; harita odağı ve dağıtım motorunun
/// kümeleme referansıdır.
pub async fn get_school_location(pool: &SqlitePool) -> AppResult<Option<(f64, f64)>> {
    let latitude = get(pool, "school_latitude").await?;
    let longitude = get(pool, "school_longitude").await?;

    match (latitude, longitude) {
        (Some(lat), Some(lon)) => {
            match (lat.trim().parse::<f64>(), lon.trim().parse::<f64>()) {
                (Ok(lat), Ok(lon)) => Ok(Some((lat, lon))),
                // Boş veya bozuk değer "konum yok" demektir, hata değil.
                _ => Ok(None),
            }
        }
        _ => Ok(None),
    }
}

pub async fn set_school_location(pool: &SqlitePool, latitude: f64, longitude: f64) -> AppResult<()> {
    let mut entries = BTreeMap::new();
    entries.insert("school_latitude".to_string(), latitude.to_string());
    entries.insert("school_longitude".to_string(), longitude.to_string());
    set_many(pool, &entries).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    #[tokio::test]
    async fn migration_seeds_expected_defaults() {
        let (_dir, pool) = test_pool().await;
        let all = get_all(&pool).await.unwrap();

        assert_eq!(all.get("institution_type").map(String::as_str), Some("other"));
        assert_eq!(
            all.get("is_metropolitan_district").map(String::as_str),
            Some("true")
        );
        assert_eq!(all.get("day_start_hour").map(String::as_str), Some("8"));
        assert_eq!(all.get("day_end_hour").map(String::as_str), Some("17"));
    }

    #[tokio::test]
    async fn set_overwrites_existing_key() {
        let (_dir, pool) = test_pool().await;
        set(&pool, "active_term", "2027-2028/2").await.unwrap();
        assert_eq!(
            get(&pool, "active_term").await.unwrap().as_deref(),
            Some("2027-2028/2")
        );
    }

    #[tokio::test]
    async fn get_returns_none_for_unknown_key() {
        let (_dir, pool) = test_pool().await;
        assert_eq!(get(&pool, "bilinmeyen_anahtar").await.unwrap(), None);
    }

    /// Okul konumu seed'de boştur; kısmi veya bozuk değer "konum yok" sayılır.
    #[tokio::test]
    async fn school_location_is_none_until_set() {
        let (_dir, pool) = test_pool().await;
        assert_eq!(get_school_location(&pool).await.unwrap(), None);

        set(&pool, "school_latitude", "36.8").await.unwrap();
        assert_eq!(
            get_school_location(&pool).await.unwrap(),
            None,
            "yalnızca enlem varken konum yok sayılmalı"
        );
    }

    #[tokio::test]
    async fn school_location_roundtrips() {
        let (_dir, pool) = test_pool().await;
        set_school_location(&pool, 36.8121, 34.6415).await.unwrap();

        let location = get_school_location(&pool).await.unwrap().unwrap();
        assert!((location.0 - 36.8121).abs() < 1e-9);
        assert!((location.1 - 34.6415).abs() < 1e-9);
    }

    #[tokio::test]
    async fn set_many_writes_all_entries() {
        let (_dir, pool) = test_pool().await;
        let mut entries = BTreeMap::new();
        entries.insert("school_name".to_string(), "Test Lisesi".to_string());
        entries.insert("active_term".to_string(), "2026-2027/2".to_string());

        set_many(&pool, &entries).await.unwrap();

        let all = get_all(&pool).await.unwrap();
        assert_eq!(all.get("school_name").map(String::as_str), Some("Test Lisesi"));
        assert_eq!(all.get("active_term").map(String::as_str), Some("2026-2027/2"));
    }
}
