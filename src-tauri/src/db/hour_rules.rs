use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// İşletme saat tavanı kuralı.
/// Mesafe eşikleri GİDİŞ-DÖNÜŞ km cinsindendir (tek yönün iki katı).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct HourRule {
    pub id: i64,
    /// Dahil
    pub min_distance_km: f64,
    /// Hariç; None = üst sınırsız
    pub max_distance_km: Option<f64>,
    /// Dahil
    pub min_students: i64,
    /// Dahil; None = üst sınırsız
    pub max_students: Option<i64>,
    pub max_hours: i64,
}

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

impl HourRule {
    fn matches(&self, round_trip_km: f64, student_count: i64) -> bool {
        let above_min_distance = round_trip_km >= self.min_distance_km;
        let below_max_distance = match self.max_distance_km {
            Some(max) => round_trip_km < max,
            None => true,
        };
        let above_min_students = student_count >= self.min_students;
        let below_max_students = match self.max_students {
            Some(max) => student_count <= max,
            None => true,
        };

        above_min_distance && below_max_distance && above_min_students && below_max_students
    }

    /// Mesafe aralığının genişliği. Üst sınırsız aralık sonsuz sayılır.
    fn distance_span(&self) -> f64 {
        match self.max_distance_km {
            Some(max) => max - self.min_distance_km,
            None => f64::INFINITY,
        }
    }

    /// Öğrenci aralığının genişliği. Üst sınırsız aralık sonsuz sayılır.
    fn student_span(&self) -> f64 {
        match self.max_students {
            Some(max) => (max - self.min_students) as f64,
            None => f64::INFINITY,
        }
    }
}

/// Bir işletmeye uyan kurallar arasından EN DAR olanı seçer.
///
/// Sıralama kesindir (spec §6):
/// 1. En küçük mesafe aralığı genişliği
/// 2. Eşitlikte en küçük öğrenci aralığı genişliği
/// 3. Hâlâ eşitse en küçük `id`
///
/// Hiçbir kural uymazsa None döner; varsayılan uydurulmaz.
pub fn select_narrowest(
    rules: &[HourRule],
    round_trip_km: f64,
    student_count: i64,
) -> Option<&HourRule> {
    rules
        .iter()
        .filter(|rule| rule.matches(round_trip_km, student_count))
        .min_by(|a, b| {
            a.distance_span()
                .total_cmp(&b.distance_span())
                .then(a.student_span().total_cmp(&b.student_span()))
                .then(a.id.cmp(&b.id))
        })
}

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

    fn rule(
        id: i64,
        min_distance_km: f64,
        max_distance_km: Option<f64>,
        min_students: i64,
        max_students: Option<i64>,
        max_hours: i64,
    ) -> HourRule {
        HourRule {
            id,
            min_distance_km,
            max_distance_km,
            min_students,
            max_students,
            max_hours,
        }
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

    /// Öğrenci sayısı 6'da 5-6 ve 6+ sütunları çakışır; en dar olan kazanır.
    #[test]
    fn six_students_falls_into_the_narrower_five_to_six_bracket() {
        let rules = vec![
            rule(3, 0.0, Some(1.0), 5, Some(6), 4),
            rule(4, 0.0, Some(1.0), 6, None, 5),
        ];
        let chosen = select_narrowest(&rules, 0.5, 6).unwrap();
        assert_eq!(chosen.id, 3, "5-6 aralığı 6+ aralığından dardır");
        assert_eq!(chosen.max_hours, 4);
    }

    /// Mesafe aralığı genişliği öğrenci aralığından önce gelir.
    #[test]
    fn distance_span_outranks_student_span() {
        let rules = vec![
            // Dar mesafe, geniş öğrenci
            rule(1, 0.0, Some(1.0), 1, None, 2),
            // Geniş mesafe, dar öğrenci
            rule(2, 0.0, Some(100.0), 1, Some(2), 9),
        ];
        assert_eq!(select_narrowest(&rules, 0.5, 1).unwrap().id, 1);
    }

    /// Her şey eşitse en küçük id kazanır; sonuç belirlenimcidir.
    #[test]
    fn identical_spans_resolve_by_lowest_id() {
        let rules = vec![
            rule(7, 0.0, Some(1.0), 1, Some(2), 3),
            rule(2, 0.0, Some(1.0), 1, Some(2), 9),
        ];
        assert_eq!(select_narrowest(&rules, 0.5, 1).unwrap().id, 2);
    }

    #[test]
    fn returns_none_when_no_rule_matches() {
        let rules = vec![rule(1, 0.0, Some(1.0), 1, Some(2), 2)];
        // Mesafe uyuyor ama öğrenci sayısı aralık dışında
        assert!(select_narrowest(&rules, 0.5, 9).is_none());
        // Öğrenci uyuyor ama mesafe aralık dışında
        assert!(select_narrowest(&rules, 50.0, 1).is_none());
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
