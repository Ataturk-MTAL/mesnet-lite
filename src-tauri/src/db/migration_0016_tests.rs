//! Migration 0016 (işletme mahallesi + konum kesinliği) doğrulama testleri.
//!
//! Var olan satırların `geocode_precision` değeri, gerçek bir eski şemadan
//! (0015) yükseltilerek KANITLANIR; `migration_0015_tests` ile aynı desen.

use std::borrow::Cow;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

const LAST_LEGACY_VERSION: i64 = 15;

async fn pool_at_0015() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let options = SqliteConnectOptions::new()
        .filename(dir.path().join("legacy.db"))
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();

    let mut legacy = sqlx::migrate!("./migrations");
    let steps: Vec<_> = legacy.migrations.iter().filter(|m| m.version <= LAST_LEGACY_VERSION).cloned().collect();
    legacy.migrations = Cow::Owned(steps);
    legacy.run(&pool).await.unwrap();
    (dir, pool)
}

async fn precision_of(pool: &SqlitePool, name: &str) -> String {
    sqlx::query_scalar("SELECT geocode_precision FROM companies WHERE name = ?1")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Çözülmüş+koordinatlı -> 'address'; elle -> 'manual'; diğerleri ''.
/// Koordinatsız 'resolved' (tutarsız eski veri) 'address' SAYILMAZ.
#[tokio::test]
async fn migration_0016_derives_precision_for_existing_rows() {
    let (_dir, pool) = pool_at_0015().await;
    for (name, status, lat, lon) in [
        ("Çözülmüş", "resolved", Some(36.8), Some(34.6)),
        ("Elle", "manual", Some(36.8), Some(34.6)),
        ("Bekleyen", "pending", None, None),
        ("Başarısız", "failed", None, None),
        ("Koordinatsız Çözülmüş", "resolved", None, None),
    ] {
        sqlx::query(
            "INSERT INTO companies (name, address_text, geocode_status, latitude, longitude, created_at, updated_at)
             VALUES (?1, 'adres', ?2, ?3, ?4, 't', 't')",
        )
        .bind(name)
        .bind(status)
        .bind(lat)
        .bind(lon)
        .execute(&pool)
        .await
        .unwrap();
    }

    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    assert_eq!(precision_of(&pool, "Çözülmüş").await, "address");
    assert_eq!(precision_of(&pool, "Elle").await, "manual");
    assert_eq!(precision_of(&pool, "Bekleyen").await, "");
    assert_eq!(precision_of(&pool, "Başarısız").await, "");
    assert_eq!(precision_of(&pool, "Koordinatsız Çözülmüş").await, "");
    let neighborhood: String =
        sqlx::query_scalar("SELECT neighborhood FROM companies WHERE name = 'Elle'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(neighborhood, "");
}
