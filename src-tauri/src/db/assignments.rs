use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Bir işletmenin bir öğretmenin haftalık programındaki yerleşimi.
///
/// Saat burada TUTULMAZ: ek ders saati işletmenin dönemlik takdiridir
/// (`company_term_hours`). Burada yalnızca ziyaretin ne zaman yapılacağı durur.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Assignment {
    pub id: i64,
    pub teacher_id: i64,
    pub company_id: i64,
    pub term: String,
    /// 1 = Pazartesi … 5 = Cuma
    pub visit_day: i64,
    /// Kaçıncı ders saati
    pub visit_hour: i64,
    pub is_forced: i64,
    pub force_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAssignment {
    pub teacher_id: i64,
    pub company_id: i64,
    pub visit_day: i64,
    pub visit_hour: i64,
    pub is_forced: bool,
    pub force_reason: Option<String>,
}

const SELECT_COLUMNS: &str = "id, teacher_id, company_id, term, visit_day, visit_hour, \
     is_forced, force_reason, created_at, updated_at";

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub async fn list(pool: &SqlitePool, term: &str) -> AppResult<Vec<Assignment>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM assignments WHERE term = ?1
         ORDER BY teacher_id, visit_day, visit_hour"
    );
    Ok(sqlx::query_as::<_, Assignment>(&sql)
        .bind(term)
        .fetch_all(pool)
        .await?)
}

pub async fn list_for_teacher(
    pool: &SqlitePool,
    teacher_id: i64,
    term: &str,
) -> AppResult<Vec<Assignment>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM assignments WHERE teacher_id = ?1 AND term = ?2
         ORDER BY visit_day, visit_hour"
    );
    Ok(sqlx::query_as::<_, Assignment>(&sql)
        .bind(teacher_id)
        .bind(term)
        .fetch_all(pool)
        .await?)
}

pub async fn get_for_company(
    pool: &SqlitePool,
    company_id: i64,
    term: &str,
) -> AppResult<Option<Assignment>> {
    let sql =
        format!("SELECT {SELECT_COLUMNS} FROM assignments WHERE company_id = ?1 AND term = ?2");
    Ok(sqlx::query_as::<_, Assignment>(&sql)
        .bind(company_id)
        .bind(term)
        .fetch_optional(pool)
        .await?)
}

/// İşletmeyi bir öğretmenin hücresine yerleştirir.
/// Aynı işletme zaten atanmışsa yerleşim güncellenir (taşıma).
pub async fn assign(
    pool: &SqlitePool,
    term: &str,
    input: &NewAssignment,
) -> AppResult<Assignment> {
    validate(input)?;

    // Hedef hücre başka bir işletme tarafından doluysa açık hata verilir;
    // UNIQUE kısıtının teknik mesajı kullanıcıya bir şey anlatmaz.
    let occupied: Option<i64> = sqlx::query_scalar(
        "SELECT company_id FROM assignments
         WHERE teacher_id = ?1 AND term = ?2 AND visit_day = ?3 AND visit_hour = ?4
           AND company_id <> ?5",
    )
    .bind(input.teacher_id)
    .bind(term)
    .bind(input.visit_day)
    .bind(input.visit_hour)
    .bind(input.company_id)
    .fetch_optional(pool)
    .await?;

    if occupied.is_some() {
        return Err(AppError::Validation(
            "Bu gün ve saatte öğretmenin başka bir işletmesi var".into(),
        ));
    }

    let now = now_iso();
    sqlx::query(
        "INSERT INTO assignments
            (teacher_id, company_id, term, visit_day, visit_hour,
             is_forced, force_reason, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
         ON CONFLICT(company_id, term) DO UPDATE SET
            teacher_id   = excluded.teacher_id,
            visit_day    = excluded.visit_day,
            visit_hour   = excluded.visit_hour,
            is_forced    = excluded.is_forced,
            force_reason = excluded.force_reason,
            updated_at   = excluded.updated_at",
    )
    .bind(input.teacher_id)
    .bind(input.company_id)
    .bind(term)
    .bind(input.visit_day)
    .bind(input.visit_hour)
    .bind(i64::from(input.is_forced))
    .bind(&input.force_reason)
    .bind(&now)
    .execute(pool)
    .await?;

    get_for_company(pool, input.company_id, term)
        .await?
        .ok_or_else(|| AppError::Database("Atama yazıldı ama okunamadı".into()))
}

/// İşletmenin atamasını kaldırır. Atama yoksa sessizce geçer.
pub async fn unassign(pool: &SqlitePool, company_id: i64, term: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM assignments WHERE company_id = ?1 AND term = ?2")
        .bind(company_id)
        .bind(term)
        .execute(pool)
        .await?;
    Ok(())
}

/// Dönemdeki tüm atamaları siler. Öneriyi baştan uygulamadan önce kullanılır.
pub async fn clear_term(pool: &SqlitePool, term: &str) -> AppResult<u64> {
    let affected = sqlx::query("DELETE FROM assignments WHERE term = ?1")
        .bind(term)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(affected)
}

/// Öğretmen başına toplam ek ders saati.
///
/// Saat atamadan değil, işletmenin dönemlik takdirinden gelir; bu yüzden
/// iki tablo birleştirilir. Takdiri girilmemiş işletme 0 saat sayılır.
pub async fn awarded_hours_by_teacher(pool: &SqlitePool, term: &str) -> AppResult<Vec<(i64, i64)>> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT a.teacher_id, COALESCE(SUM(h.awarded_hours), 0)
         FROM assignments a
         LEFT JOIN company_term_hours h
                ON h.company_id = a.company_id AND h.term = a.term
         WHERE a.term = ?1
         GROUP BY a.teacher_id",
    )
    .bind(term)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Öğretmenin gün başına toplam ek ders saati (OÖKY MADDE 88 günlük sınırı).
pub async fn awarded_hours_by_teacher_and_day(
    pool: &SqlitePool,
    term: &str,
) -> AppResult<Vec<(i64, i64, i64)>> {
    let rows: Vec<(i64, i64, i64)> = sqlx::query_as(
        "SELECT a.teacher_id, a.visit_day, COALESCE(SUM(h.awarded_hours), 0)
         FROM assignments a
         LEFT JOIN company_term_hours h
                ON h.company_id = a.company_id AND h.term = a.term
         WHERE a.term = ?1
         GROUP BY a.teacher_id, a.visit_day",
    )
    .bind(term)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Dönemde atanmış toplam ek ders saati.
pub async fn total_assigned_hours(pool: &SqlitePool, term: &str) -> AppResult<i64> {
    let total: Option<i64> = sqlx::query_scalar(
        "SELECT SUM(h.awarded_hours)
         FROM assignments a
         JOIN company_term_hours h
           ON h.company_id = a.company_id AND h.term = a.term
         WHERE a.term = ?1",
    )
    .bind(term)
    .fetch_one(pool)
    .await?;
    Ok(total.unwrap_or(0))
}

pub async fn assigned_company_ids(pool: &SqlitePool, term: &str) -> AppResult<Vec<i64>> {
    let rows: Vec<(i64,)> = sqlx::query_as("SELECT company_id FROM assignments WHERE term = ?1")
        .bind(term)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

fn validate(input: &NewAssignment) -> AppResult<()> {
    if !(1..=5).contains(&input.visit_day) {
        return Err(AppError::Validation(
            "Ziyaret günü Pazartesi ile Cuma arasında olmalı".into(),
        ));
    }
    if input.visit_hour < 0 {
        return Err(AppError::Validation("Ders saati negatif olamaz".into()));
    }
    if input.is_forced
        && input
            .force_reason
            .as_ref()
            .map(|r| r.trim().is_empty())
            .unwrap_or(true)
    {
        return Err(AppError::Validation(
            "Zorlama gerekçesi boş bırakılamaz".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::company_hours::HoursInput;
    use crate::db::{companies, company_hours, init_pool, teachers};
    use crate::domain::models::{NewCompany, NewTeacher};

    const TERM: &str = "2026-2027/1";

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    async fn a_company(pool: &SqlitePool, name: &str) -> i64 {
        companies::create(
            pool,
            &NewCompany {
                name: name.into(),
                contact_first_name: String::new(),
                contact_last_name: String::new(),
                phone: String::new(),
                email: String::new(),
                address_text: "Test adres".into(),
                latitude: None,
                longitude: None,
                one_way_distance_km: Some(6.8),
                notes: String::new(),
            },
        )
        .await
        .unwrap()
        .id
    }

    async fn a_teacher(pool: &SqlitePool, last_name: &str) -> i64 {
        teachers::create(
            pool,
            &NewTeacher {
                first_name: "Test".into(),
                last_name: last_name.into(),
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
        .unwrap()
        .id
    }

    async fn set_hours(pool: &SqlitePool, company_id: i64, awarded: i64) {
        company_hours::upsert(
            pool,
            TERM,
            &HoursInput {
                company_id,
                max_hours_snapshot: 8,
                awarded_hours: awarded,
                is_honorary: false,
                is_locked: false,
                notes: String::new(),
            },
        )
        .await
        .unwrap();
    }

    fn placement(teacher_id: i64, company_id: i64, day: i64, hour: i64) -> NewAssignment {
        NewAssignment {
            teacher_id,
            company_id,
            visit_day: day,
            visit_hour: hour,
            is_forced: false,
            force_reason: None,
        }
    }

    #[tokio::test]
    async fn assign_places_company_in_one_cell() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        let company_id = a_company(&pool, "Test İşletme A").await;

        let saved = assign(&pool, TERM, &placement(teacher_id, company_id, 3, 4))
            .await
            .unwrap();

        assert_eq!(saved.visit_day, 3);
        assert_eq!(saved.visit_hour, 4);
        assert_eq!(list(&pool, TERM).await.unwrap().len(), 1);
    }

    /// Aynı işletme yeniden atanınca taşınır, ikinci kayıt oluşmaz.
    #[tokio::test]
    async fn reassigning_a_company_moves_it() {
        let (_dir, pool) = test_pool().await;
        let first = a_teacher(&pool, "Bir").await;
        let second = a_teacher(&pool, "Iki").await;
        let company_id = a_company(&pool, "Test İşletme A").await;

        assign(&pool, TERM, &placement(first, company_id, 1, 1))
            .await
            .unwrap();
        let moved = assign(&pool, TERM, &placement(second, company_id, 5, 7))
            .await
            .unwrap();

        assert_eq!(moved.teacher_id, second);
        assert_eq!(moved.visit_day, 5);
        assert_eq!(list(&pool, TERM).await.unwrap().len(), 1);
    }

    /// Aynı öğretmenin aynı hücresine ikinci işletme konulamaz.
    #[tokio::test]
    async fn occupied_cell_is_rejected_with_a_readable_message() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        let first = a_company(&pool, "Bir").await;
        let second = a_company(&pool, "Iki").await;

        assign(&pool, TERM, &placement(teacher_id, first, 2, 3))
            .await
            .unwrap();
        let err = assign(&pool, TERM, &placement(teacher_id, second, 2, 3))
            .await
            .unwrap_err();

        match err {
            AppError::Validation(message) => assert!(message.contains("başka bir işletmesi")),
            other => panic!("beklenmeyen hata: {other:?}"),
        }
    }

    /// Farklı öğretmenler aynı gün ve saatte çalışabilir.
    #[tokio::test]
    async fn same_cell_is_allowed_for_different_teachers() {
        let (_dir, pool) = test_pool().await;
        let first = a_teacher(&pool, "Bir").await;
        let second = a_teacher(&pool, "Iki").await;
        let company_a = a_company(&pool, "A").await;
        let company_b = a_company(&pool, "B").await;

        assign(&pool, TERM, &placement(first, company_a, 2, 3))
            .await
            .unwrap();
        assign(&pool, TERM, &placement(second, company_b, 2, 3))
            .await
            .unwrap();

        assert_eq!(list(&pool, TERM).await.unwrap().len(), 2);
    }

    /// Saat atamadan değil, işletmenin takdirinden gelir.
    #[tokio::test]
    async fn hours_come_from_company_term_hours_not_from_the_assignment() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        let company_id = a_company(&pool, "Test İşletme A").await;

        assign(&pool, TERM, &placement(teacher_id, company_id, 1, 1))
            .await
            .unwrap();
        // Takdir henüz girilmedi
        assert_eq!(total_assigned_hours(&pool, TERM).await.unwrap(), 0);

        set_hours(&pool, company_id, 6).await;
        assert_eq!(total_assigned_hours(&pool, TERM).await.unwrap(), 6);
    }

    #[tokio::test]
    async fn awarded_hours_by_teacher_sums_across_companies() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        let first = a_company(&pool, "Bir").await;
        let second = a_company(&pool, "Iki").await;

        set_hours(&pool, first, 6).await;
        set_hours(&pool, second, 4).await;
        assign(&pool, TERM, &placement(teacher_id, first, 1, 1))
            .await
            .unwrap();
        assign(&pool, TERM, &placement(teacher_id, second, 2, 1))
            .await
            .unwrap();

        let rows = awarded_hours_by_teacher(&pool, TERM).await.unwrap();
        assert_eq!(rows, vec![(teacher_id, 10)]);
    }

    /// Günlük sınır denetimi için gün bazında toplam gerekir.
    #[tokio::test]
    async fn awarded_hours_by_day_groups_per_day() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        let first = a_company(&pool, "Bir").await;
        let second = a_company(&pool, "Iki").await;

        set_hours(&pool, first, 6).await;
        set_hours(&pool, second, 4).await;
        // İkisi de aynı gün, farklı ders saati
        assign(&pool, TERM, &placement(teacher_id, first, 3, 1))
            .await
            .unwrap();
        assign(&pool, TERM, &placement(teacher_id, second, 3, 2))
            .await
            .unwrap();

        let rows = awarded_hours_by_teacher_and_day(&pool, TERM).await.unwrap();
        assert_eq!(rows, vec![(teacher_id, 3, 10)], "aynı günde 10 saat");
    }

    #[tokio::test]
    async fn unassign_removes_the_placement() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        let company_id = a_company(&pool, "Test İşletme A").await;

        assign(&pool, TERM, &placement(teacher_id, company_id, 1, 1))
            .await
            .unwrap();
        unassign(&pool, company_id, TERM).await.unwrap();

        assert!(list(&pool, TERM).await.unwrap().is_empty());
        assert!(get_for_company(&pool, company_id, TERM)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn clear_term_removes_only_that_term() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        let company_id = a_company(&pool, "Test İşletme A").await;

        assign(&pool, TERM, &placement(teacher_id, company_id, 1, 1))
            .await
            .unwrap();
        assign(
            &pool,
            "2027-2028/1",
            &placement(teacher_id, company_id, 2, 2),
        )
        .await
        .unwrap();

        assert_eq!(clear_term(&pool, TERM).await.unwrap(), 1);
        assert!(list(&pool, TERM).await.unwrap().is_empty());
        assert_eq!(list(&pool, "2027-2028/1").await.unwrap().len(), 1);
    }

    /// Zorlama gerekçesiz kaydedilemez; denetimde "neden" sorusunun cevabı kalmalı.
    #[tokio::test]
    async fn forcing_without_a_reason_is_rejected() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        let company_id = a_company(&pool, "Test İşletme A").await;

        let mut forced = placement(teacher_id, company_id, 1, 1);
        forced.is_forced = true;

        assert!(assign(&pool, TERM, &forced).await.is_err());

        forced.force_reason = Some("  ".into());
        assert!(assign(&pool, TERM, &forced).await.is_err());

        forced.force_reason = Some("Ulaşım zorunluluğu".into());
        let saved = assign(&pool, TERM, &forced).await.unwrap();
        assert_eq!(saved.is_forced, 1);
    }

    #[tokio::test]
    async fn invalid_day_is_rejected() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        let company_id = a_company(&pool, "Test İşletme A").await;

        assert!(assign(&pool, TERM, &placement(teacher_id, company_id, 6, 1))
            .await
            .is_err());
        assert!(assign(&pool, TERM, &placement(teacher_id, company_id, 0, 1))
            .await
            .is_err());
    }
}
