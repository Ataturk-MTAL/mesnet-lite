use crate::error::AppResult;
use sqlx::SqlitePool;

/// Verilen dönemde takdir edilmiş toplam koordinatörlük saati.
/// Atama yoksa 0 döner.
pub async fn total_awarded_hours(pool: &SqlitePool, term: &str) -> AppResult<i64> {
    let total: Option<i64> =
        sqlx::query_scalar("SELECT SUM(awarded_hours) FROM assignments WHERE term = ?1")
            .bind(term)
            .fetch_one(pool)
            .await?;
    Ok(total.unwrap_or(0))
}

/// Öğretmen başına takdir edilmiş toplam saat.
pub async fn awarded_hours_by_teacher(
    pool: &SqlitePool,
    term: &str,
) -> AppResult<Vec<(i64, i64)>> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT teacher_id, SUM(awarded_hours) FROM assignments
         WHERE term = ?1 GROUP BY teacher_id",
    )
    .bind(term)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Verilen dönemde ataması olan işletmelerin id'leri.
pub async fn assigned_company_ids(pool: &SqlitePool, term: &str) -> AppResult<Vec<i64>> {
    let rows: Vec<(i64,)> =
        sqlx::query_as("SELECT company_id FROM assignments WHERE term = ?1")
            .bind(term)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{companies, init_pool, teachers};
    use crate::domain::models::{NewCompany, NewTeacher};

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    async fn seed_assignment(pool: &SqlitePool, term: &str, hours: i64, suffix: &str) -> i64 {
        let company = companies::create(
            pool,
            &NewCompany {
                name: format!("İşletme {suffix}"),
                contact_first_name: String::new(),
                contact_last_name: String::new(),
                phone: String::new(),
                email: String::new(),
                address_text: "Test adres".into(),
                latitude: None,
                longitude: None,
                one_way_distance_km: Some(3.0),
                notes: String::new(),
            },
        )
        .await
        .unwrap();

        let teacher = teachers::create(
            pool,
            &NewTeacher {
                first_name: "Test".into(),
                last_name: format!("Ogretmen {suffix}"),
                registry_no: String::new(),
                field: "Elektrik-Elektronik Teknolojisi".into(),
                branches: vec![],
                employment_type: "tenured".into(),
                base_hours: 20,
                max_extra_hours: 24,
                other_extra_hours: 0,
                chief_type: "none".into(),
                is_active: true,
            },
        )
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO assignments
                (teacher_id, company_id, awarded_hours, max_hours_snapshot,
                 is_forced, force_reason, term, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3, 0, NULL, ?4, '2026-09-18', '2026-09-18')",
        )
        .bind(teacher.id)
        .bind(company.id)
        .bind(hours)
        .bind(term)
        .execute(pool)
        .await
        .unwrap();

        teacher.id
    }

    #[tokio::test]
    async fn total_awarded_hours_is_zero_without_assignments() {
        let (_dir, pool) = test_pool().await;
        assert_eq!(total_awarded_hours(&pool, "2026-2027/1").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn total_awarded_hours_sums_only_the_given_term() {
        let (_dir, pool) = test_pool().await;
        seed_assignment(&pool, "2026-2027/1", 6, "a").await;
        seed_assignment(&pool, "2026-2027/1", 4, "b").await;
        seed_assignment(&pool, "2027-2028/1", 8, "c").await;

        assert_eq!(total_awarded_hours(&pool, "2026-2027/1").await.unwrap(), 10);
        assert_eq!(total_awarded_hours(&pool, "2027-2028/1").await.unwrap(), 8);
    }

    #[tokio::test]
    async fn awarded_hours_by_teacher_groups_per_teacher() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = seed_assignment(&pool, "2026-2027/1", 6, "a").await;

        let rows = awarded_hours_by_teacher(&pool, "2026-2027/1").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], (teacher_id, 6));
    }

    #[tokio::test]
    async fn assigned_company_ids_is_scoped_to_term() {
        let (_dir, pool) = test_pool().await;
        seed_assignment(&pool, "2026-2027/1", 6, "a").await;
        seed_assignment(&pool, "2027-2028/1", 6, "b").await;

        assert_eq!(assigned_company_ids(&pool, "2026-2027/1").await.unwrap().len(), 1);
    }
}
