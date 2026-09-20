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

/// Aktif eğitim-öğretim yılı. Ayar boşsa boş metin döner; uydurma yapılmaz.
pub async fn get_active_term(pool: &SqlitePool) -> AppResult<String> {
    Ok(get(pool, "active_term").await?.unwrap_or_default())
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

/// Bilinen tüm dönemler: döneme bağlı her tablonun birleşimi + aktif dönem.
///
/// Yalnızca `students` tablosuna bakmak (MESNET'in eski `listTerms` davranışı)
/// henüz öğrencisi girilmemiş yeni bir eğitim-öğretim yılını gizler — dönem
/// yönetimi ekranı bu yüzden bunun yerine bu fonksiyonu kullanmalıdır.
/// En yeniden eskiye sıralı, tekrarsız.
pub async fn known_terms(pool: &SqlitePool) -> AppResult<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT term FROM students WHERE term <> ''
         UNION SELECT term FROM teacher_availability WHERE term <> ''
         UNION SELECT term FROM class_workplace_days WHERE term <> ''
         UNION SELECT term FROM company_term_hours WHERE term <> ''
         UNION SELECT term FROM assignments WHERE term <> ''
         UNION SELECT term FROM term_branch_hours WHERE term <> ''
         UNION SELECT value FROM settings WHERE key = 'active_term' AND value <> ''
         ORDER BY term DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(term,)| term).collect())
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

    #[tokio::test]
    async fn known_terms_includes_the_seeded_active_term_by_default() {
        let (_dir, pool) = test_pool().await;
        assert_eq!(known_terms(&pool).await.unwrap(), vec!["2026-2027/1".to_string()]);
    }

    /// Asıl kusur buradaydı: yeni bir eğitim-öğretim yılının henüz öğrencisi
    /// olmayabilir; yalnızca `students` tablosuna bakmak (eski
    /// `studentsApi.listTerms()` davranışı) o dönemi tamamen gizlerdi.
    /// Ders yükü satırı gibi başka herhangi bir döneme bağlı veri bile
    /// dönemin listede görünmesi için yeterli olmalı.
    #[tokio::test]
    async fn known_terms_includes_a_term_that_only_has_teaching_load_rows() {
        let (_dir, pool) = test_pool().await;
        crate::db::teaching_load::replace_for_term(
            &pool,
            "2027-2028/1",
            &[crate::db::teaching_load::TermBranchHoursInput {
                grade: "12/C".into(),
                branch: "Elektronik Haberleşme".into(),
                weekly_hours: 24,
                group_count: 2,
                is_group_manual: true,
            }],
        )
        .await
        .unwrap();

        let terms = known_terms(&pool).await.unwrap();
        assert!(terms.contains(&"2027-2028/1".to_string()));
        assert!(
            terms.contains(&"2026-2027/1".to_string()),
            "seed'deki aktif dönem de görünmeye devam etmeli"
        );
    }

    #[tokio::test]
    async fn known_terms_are_deduplicated_and_sorted_newest_first() {
        let (_dir, pool) = test_pool().await;
        set(&pool, "active_term", "2025-2026/1").await.unwrap();
        crate::db::teaching_load::replace_for_term(
            &pool,
            "2025-2026/1",
            &[crate::db::teaching_load::TermBranchHoursInput {
                grade: "12/C".into(),
                branch: "Dal".into(),
                weekly_hours: 20,
                group_count: 1,
                is_group_manual: true,
            }],
        )
        .await
        .unwrap();
        crate::db::teaching_load::replace_for_term(
            &pool,
            "2027-2028/1",
            &[crate::db::teaching_load::TermBranchHoursInput {
                grade: "12/D".into(),
                branch: "Dal 2".into(),
                weekly_hours: 20,
                group_count: 1,
                is_group_manual: true,
            }],
        )
        .await
        .unwrap();

        // "2025-2026/1" hem active_term hem de ders yükü satırından geliyor;
        // bir kez görünmeli.
        assert_eq!(
            known_terms(&pool).await.unwrap(),
            vec!["2027-2028/1".to_string(), "2025-2026/1".to_string()]
        );
    }
}
