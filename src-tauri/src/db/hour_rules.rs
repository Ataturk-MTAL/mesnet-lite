use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

// `HourRule` ve `select_narrowest` artık saf çekirdekte yaşar (R1,
// `domain::history`nin tarih aralıklı katlaması bu kuralları I/O'suz
// kullanabilsin diye). Bu yeniden dışa aktarım, `crate::db::hour_rules::…`
// yoluyla çağıran mevcut kodun (ör. `commands/hours_commands.rs`) değişmeden
// derlenmesini sağlar.
pub use crate::domain::hour_rules::{select_narrowest, HourRule};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewHourRule {
    pub min_distance_km: f64,
    pub max_distance_km: Option<f64>,
    pub min_students: i64,
    pub max_students: Option<i64>,
    pub max_hours: i64,
}

const SELECT_COLUMNS: &str =
    "id, min_distance_km, max_distance_km, min_students, max_students, max_hours";

pub async fn list(pool: &SqlitePool) -> AppResult<Vec<HourRule>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM company_hour_rules
         ORDER BY min_distance_km, min_students"
    );
    Ok(sqlx::query_as::<_, HourRule>(&sql).fetch_all(pool).await?)
}

pub async fn get(pool: &SqlitePool, id: i64) -> AppResult<HourRule> {
    let sql = format!("SELECT {SELECT_COLUMNS} FROM company_hour_rules WHERE id = ?1");
    Ok(sqlx::query_as::<_, HourRule>(&sql)
        .bind(id)
        .fetch_one(pool)
        .await?)
}

pub async fn create(pool: &SqlitePool, input: &NewHourRule) -> AppResult<HourRule> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO company_hour_rules
            (min_distance_km, max_distance_km, min_students, max_students, max_hours)
         VALUES (?1, ?2, ?3, ?4, ?5)
         RETURNING id",
    )
    .bind(input.min_distance_km)
    .bind(input.max_distance_km)
    .bind(input.min_students)
    .bind(input.max_students)
    .bind(input.max_hours)
    .fetch_one(pool)
    .await?;

    get(pool, id).await
}

pub async fn update(pool: &SqlitePool, id: i64, input: &NewHourRule) -> AppResult<HourRule> {
    let affected = sqlx::query(
        "UPDATE company_hour_rules SET
            min_distance_km = ?1, max_distance_km = ?2,
            min_students = ?3, max_students = ?4, max_hours = ?5
         WHERE id = ?6",
    )
    .bind(input.min_distance_km)
    .bind(input.max_distance_km)
    .bind(input.min_students)
    .bind(input.max_students)
    .bind(input.max_hours)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!("Saat kuralı bulunamadı: {id}")));
    }
    get(pool, id).await
}

pub async fn remove(pool: &SqlitePool, id: i64) -> AppResult<()> {
    let affected = sqlx::query("DELETE FROM company_hour_rules WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!("Saat kuralı bulunamadı: {id}")));
    }
    Ok(())
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
    async fn seed_has_sixteen_rules() {
        let (_dir, pool) = test_pool().await;
        assert_eq!(list(&pool).await.unwrap().len(), 16);
    }

    /// Seed tablosunun dört köşesi.
    #[tokio::test]
    async fn seed_corners_match_the_agreed_table() {
        let (_dir, pool) = test_pool().await;
        let rules = list(&pool).await.unwrap();

        // 0-1 km / 1-2 öğrenci => 2
        assert_eq!(select_narrowest(&rules, 0.5, 1).unwrap().max_hours, 2);
        // 0-1 km / 6+ öğrenci => 5
        assert_eq!(select_narrowest(&rules, 0.5, 9).unwrap().max_hours, 5);
        // 5+ km / 1-2 öğrenci => 8
        assert_eq!(select_narrowest(&rules, 13.6, 1).unwrap().max_hours, 8);
        // 5+ km / 6+ öğrenci => 11
        assert_eq!(select_narrowest(&rules, 13.6, 9).unwrap().max_hours, 11);
    }

    /// CSV'deki tipik işletme: tek yön 6,8 km => gidiş-dönüş 13,6 km, 1 öğrenci.
    #[tokio::test]
    async fn typical_company_from_real_data_gets_eight_hours() {
        let (_dir, pool) = test_pool().await;
        let rules = list(&pool).await.unwrap();
        assert_eq!(select_narrowest(&rules, 6.8 * 2.0, 1).unwrap().max_hours, 8);
    }

    /// Aralık alt sınırı dahil, üst sınırı hariçtir.
    #[tokio::test]
    async fn distance_boundaries_are_lower_inclusive_upper_exclusive() {
        let (_dir, pool) = test_pool().await;
        let rules = list(&pool).await.unwrap();

        // 1,0 km tam sınır: 0-1 aralığına DEĞİL, 1-3 aralığına düşer.
        assert_eq!(select_narrowest(&rules, 1.0, 1).unwrap().max_hours, 4);
        // 0,999 km hâlâ 0-1 aralığında.
        assert_eq!(select_narrowest(&rules, 0.999, 1).unwrap().max_hours, 2);
    }

    #[tokio::test]
    async fn crud_roundtrip() {
        let (_dir, pool) = test_pool().await;

        let created = create(
            &pool,
            &NewHourRule {
                min_distance_km: 100.0,
                max_distance_km: None,
                min_students: 20,
                max_students: None,
                max_hours: 15,
            },
        )
        .await
        .unwrap();
        assert_eq!(list(&pool).await.unwrap().len(), 17);

        let updated = update(
            &pool,
            created.id,
            &NewHourRule {
                min_distance_km: 100.0,
                max_distance_km: None,
                min_students: 20,
                max_students: None,
                max_hours: 18,
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.max_hours, 18);

        remove(&pool, created.id).await.unwrap();
        assert_eq!(list(&pool).await.unwrap().len(), 16);
    }
}
