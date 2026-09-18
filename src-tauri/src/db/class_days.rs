use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::{BTreeMap, BTreeSet};

/// Bir sınıfın işletmede bulunduğu gün.
/// Sınıflar farklı günlerde işletmeye gider; koordinatör ziyareti bu günlere denk gelmelidir.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ClassWorkplaceDay {
    pub grade: String,
    /// 1 = Pazartesi … 5 = Cuma
    pub day_of_week: i64,
}

pub async fn list(pool: &SqlitePool, term: &str) -> AppResult<Vec<ClassWorkplaceDay>> {
    Ok(sqlx::query_as::<_, ClassWorkplaceDay>(
        "SELECT grade, day_of_week FROM class_workplace_days
         WHERE term = ?1 ORDER BY grade COLLATE NOCASE, day_of_week",
    )
    .bind(term)
    .fetch_all(pool)
    .await?)
}

/// Sınıf adına göre gün kümesi. Zamanlama motoru bu haritayı kullanır.
pub async fn map_by_grade(pool: &SqlitePool, term: &str) -> AppResult<BTreeMap<String, BTreeSet<i64>>> {
    let rows = list(pool, term).await?;
    let mut map: BTreeMap<String, BTreeSet<i64>> = BTreeMap::new();
    for row in rows {
        map.entry(row.grade).or_default().insert(row.day_of_week);
    }
    Ok(map)
}

/// Bir sınıfın günlerini tamamen değiştirir.
/// Ekran her kaydetmede sınıfın tüm günlerini gönderir; işlem atomiktir.
pub async fn replace_for_grade(
    pool: &SqlitePool,
    grade: &str,
    term: &str,
    days: &[i64],
) -> AppResult<()> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM class_workplace_days WHERE grade = ?1 AND term = ?2")
        .bind(grade)
        .bind(term)
        .execute(&mut *tx)
        .await?;

    // Aynı gün iki kez gönderilirse UNIQUE kısıtı düşer; tekilleştirilir.
    let unique: BTreeSet<i64> = days.iter().copied().collect();
    for day in unique {
        sqlx::query(
            "INSERT INTO class_workplace_days (grade, day_of_week, term) VALUES (?1, ?2, ?3)",
        )
        .bind(grade)
        .bind(day)
        .bind(term)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Bir dönemin sınıf günlerini bir sonraki döneme kopyalar.
/// Hedef dönemde kayıt varsa hiçbir şey yapılmaz ve `false` döner.
pub async fn copy_term(pool: &SqlitePool, from_term: &str, to_term: &str) -> AppResult<bool> {
    let existing: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM class_workplace_days WHERE term = ?1")
            .bind(to_term)
            .fetch_one(pool)
            .await?;

    if existing > 0 {
        return Ok(false);
    }

    sqlx::query(
        "INSERT INTO class_workplace_days (grade, day_of_week, term)
         SELECT grade, day_of_week, ?1 FROM class_workplace_days WHERE term = ?2",
    )
    .bind(to_term)
    .bind(from_term)
    .execute(pool)
    .await?;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;

    const TERM: &str = "2026-2027/1";

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    #[tokio::test]
    async fn replace_writes_days_for_a_grade() {
        let (_dir, pool) = test_pool().await;
        replace_for_grade(&pool, "12/C", TERM, &[1, 2, 3]).await.unwrap();

        let rows = list(&pool, TERM).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].day_of_week, 1);
    }

    /// Farklı sınıflar farklı günlerde gider; bu modelin varlık sebebi.
    #[tokio::test]
    async fn grades_can_have_different_days() {
        let (_dir, pool) = test_pool().await;
        replace_for_grade(&pool, "12/C", TERM, &[1, 2, 3]).await.unwrap();
        replace_for_grade(&pool, "12/D", TERM, &[4, 5]).await.unwrap();

        let map = map_by_grade(&pool, TERM).await.unwrap();
        assert_eq!(map["12/C"], BTreeSet::from([1, 2, 3]));
        assert_eq!(map["12/D"], BTreeSet::from([4, 5]));
    }

    #[tokio::test]
    async fn replace_overwrites_previous_days() {
        let (_dir, pool) = test_pool().await;
        replace_for_grade(&pool, "12/C", TERM, &[1, 2, 3]).await.unwrap();
        replace_for_grade(&pool, "12/C", TERM, &[5]).await.unwrap();

        let map = map_by_grade(&pool, TERM).await.unwrap();
        assert_eq!(map["12/C"], BTreeSet::from([5]));
    }

    /// Aynı gün iki kez gönderilse bile UNIQUE kısıtı düşmemeli.
    #[tokio::test]
    async fn duplicate_days_in_input_are_deduplicated() {
        let (_dir, pool) = test_pool().await;
        replace_for_grade(&pool, "12/C", TERM, &[2, 2, 2]).await.unwrap();

        assert_eq!(list(&pool, TERM).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn days_are_scoped_to_term() {
        let (_dir, pool) = test_pool().await;
        replace_for_grade(&pool, "12/C", TERM, &[1]).await.unwrap();
        replace_for_grade(&pool, "12/C", "2027-2028/1", &[4, 5]).await.unwrap();

        assert_eq!(list(&pool, TERM).await.unwrap().len(), 1);
        assert_eq!(list(&pool, "2027-2028/1").await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn copy_term_duplicates_into_empty_term_only() {
        let (_dir, pool) = test_pool().await;
        replace_for_grade(&pool, "12/C", TERM, &[1, 2]).await.unwrap();

        assert!(copy_term(&pool, TERM, "2027-2028/1").await.unwrap());
        assert_eq!(list(&pool, "2027-2028/1").await.unwrap().len(), 2);

        // İkinci çağrı üzerine yazmamalı
        assert!(!copy_term(&pool, TERM, "2027-2028/1").await.unwrap());
    }

    #[tokio::test]
    async fn map_by_grade_is_empty_when_nothing_set() {
        let (_dir, pool) = test_pool().await;
        assert!(map_by_grade(&pool, TERM).await.unwrap().is_empty());
    }
}
