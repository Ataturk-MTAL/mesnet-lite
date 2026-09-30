//! Migration 0015 (öğretmen unvanlarına müdür / müdür yardımcısı ekleme)
//! doğrulama testleri.
//!
//! Göç `teachers` tablosunu yeniden kurar; `teachers`'a `ON DELETE CASCADE`
//! ile bağlı satırların (müsaitlik, atama) kaybolmadığı, gerçek bir eski
//! şemadan (0014) yükseltilerek KANITLANIR — `availability.rs`teki
//! "göçten önce yazılmış veri göçten sağ çıkar" desenindeki gibi.

use std::borrow::Cow;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

const LAST_LEGACY_VERSION: i64 = 14;
const TERM: &str = "2026-2027/1";

/// 0014 şemasında, `foreign_keys = ON` ile (uygulamadaki gibi) tek bağlantılı havuz.
async fn pool_at_0014() -> (tempfile::TempDir, SqlitePool) {
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

async fn scalar(pool: &SqlitePool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

/// İki öğretmen (en yüksek kimlikli olan SONRA silinir: sayaç testi için),
/// biri müsaitlik ve atama taşır.
async fn seed_legacy_rows(pool: &SqlitePool) {
    for (first, chief) in [("Bir", "department"), ("Iki", "none"), ("Uc", "none")] {
        sqlx::query("INSERT INTO teachers (first_name, last_name, field, chief_type) VALUES (?1, 'Kurgusal', 'Elektrik', ?2)")
            .bind(first)
            .bind(chief)
            .execute(pool)
            .await
            .unwrap();
    }
    for (day, hour) in [(1, 2), (1, 3), (5, 8)] {
        sqlx::query("INSERT INTO teacher_availability (teacher_id, day_of_week, hour, term) VALUES (1, ?1, ?2, ?3)")
            .bind(day)
            .bind(hour)
            .bind(TERM)
            .execute(pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO companies (name, address_text, created_at, updated_at) VALUES ('Kurgusal İşletme', 'Adres', 'x', 'x')")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO assignments (teacher_id, company_id, term, visit_day, visit_hour, created_at, updated_at)
         VALUES (1, 1, ?1, 2, 3, 'x', 'x')",
    )
    .bind(TERM)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM teachers WHERE id = 3").execute(pool).await.unwrap();
}

/// Asıl veri güvenliği kanıtı: bağlı satırlar (`ON DELETE CASCADE`) ve öğretmen
/// satırları göçten sonra aynen durur.
#[tokio::test]
async fn migration_0015_keeps_teachers_and_their_cascading_children() {
    let (_dir, pool) = pool_at_0014().await;
    seed_legacy_rows(&pool).await;
    let ids_before: Vec<i64> = sqlx::query_scalar("SELECT id FROM teachers ORDER BY id").fetch_all(&pool).await.unwrap();

    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    let ids_after: Vec<i64> = sqlx::query_scalar("SELECT id FROM teachers ORDER BY id").fetch_all(&pool).await.unwrap();
    assert_eq!(ids_after, ids_before, "öğretmen kimlikleri korunmalı");
    assert_eq!(scalar(&pool, "SELECT COUNT(*) FROM teacher_availability").await, 3, "müsaitlik satırları silinmemeli");
    assert_eq!(scalar(&pool, "SELECT COUNT(*) FROM assignments").await, 1, "atama silinmemeli");
    assert_eq!(scalar(&pool, "SELECT COUNT(*) FROM teachers WHERE chief_type = 'department'").await, 1, "mevcut unvan korunmalı");
    assert_eq!(scalar(&pool, "SELECT COUNT(*) FROM pragma_foreign_key_check").await, 0, "yabancı anahtar ihlali kalmamalı");
}

/// Yabancı anahtarlar göçten sonra da çalışır: öğretmen silinince bağlı satırlar
/// hâlâ gider (şema `teachers`'a doğru bağlanmış olmalı).
#[tokio::test]
async fn migration_0015_leaves_cascade_and_foreign_key_enforcement_working() {
    let (_dir, pool) = pool_at_0014().await;
    seed_legacy_rows(&pool).await;
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    assert_eq!(scalar(&pool, "PRAGMA foreign_keys").await, 1, "bağlantı FK denetimini sürdürmeli");
    let orphan = sqlx::query("INSERT INTO teacher_availability (teacher_id, day_of_week, hour, term) VALUES (999, 1, 1, ?1)")
        .bind(TERM)
        .execute(&pool)
        .await;
    assert!(orphan.is_err(), "olmayan öğretmene bağlı satır reddedilmeli");

    sqlx::query("DELETE FROM teachers WHERE id = 1").execute(&pool).await.unwrap();
    assert_eq!(scalar(&pool, "SELECT COUNT(*) FROM teacher_availability").await, 0);
    assert_eq!(scalar(&pool, "SELECT COUNT(*) FROM assignments").await, 0);
}

/// Yeni unvanlar kabul edilir, tanınmayan reddedilir; kimlik sayacı taşınır
/// (silinmiş en yüksek kimlik yeniden verilmez).
#[tokio::test]
async fn migration_0015_accepts_new_titles_and_keeps_the_id_counter() {
    let (_dir, pool) = pool_at_0014().await;
    seed_legacy_rows(&pool).await;
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    for title in ["principal", "deputy_principal"] {
        let result = sqlx::query("INSERT INTO teachers (first_name, last_name, field, chief_type) VALUES ('Y', 'Z', 'A', ?1)")
            .bind(title)
            .execute(&pool)
            .await;
        assert!(result.is_ok(), "{title} kabul edilmeli: {result:?}");
    }
    assert!(sqlx::query("INSERT INTO teachers (first_name, last_name, field, chief_type) VALUES ('Y', 'Z', 'A', 'muhtar')")
        .execute(&pool)
        .await
        .is_err());

    // Silinen 3 numaralı öğretmenden sonra ilk yeni kimlik 4 olmalı.
    let first_new: i64 = scalar(&pool, "SELECT MIN(id) FROM teachers WHERE chief_type = 'principal'").await;
    assert_eq!(first_new, 4, "AUTOINCREMENT sayacı taşınmalı");
}

/// Boş bir veritabanında da (taze kurulum) göç sorunsuz uygulanır.
#[tokio::test]
async fn migration_0015_applies_on_an_empty_teachers_table() {
    let (_dir, pool) = pool_at_0014().await;
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    assert_eq!(scalar(&pool, "SELECT COUNT(*) FROM teachers").await, 0);
}
