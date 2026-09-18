use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Bir öğretmenin BOŞ olduğu tek bir saat dilimi.
/// Satır varsa o saat boştur; satır yoksa dolu kabul edilir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AvailabilitySlot {
    pub teacher_id: i64,
    /// 1 = Pazartesi … 5 = Cuma
    pub day_of_week: i64,
    pub hour: i64,
}

/// Bir öğretmenin verilen dönemdeki boş saatleri.
pub async fn list_for_teacher(
    pool: &SqlitePool,
    teacher_id: i64,
    term: &str,
) -> AppResult<Vec<AvailabilitySlot>> {
    Ok(sqlx::query_as::<_, AvailabilitySlot>(
        "SELECT teacher_id, day_of_week, hour FROM teacher_availability
         WHERE teacher_id = ?1 AND term = ?2
         ORDER BY day_of_week, hour",
    )
    .bind(teacher_id)
    .bind(term)
    .fetch_all(pool)
    .await?)
}

/// Dönemdeki tüm öğretmenlerin boş saatleri. Dağıtım motoru bunu tek seferde okur.
pub async fn list_all(pool: &SqlitePool, term: &str) -> AppResult<Vec<AvailabilitySlot>> {
    Ok(sqlx::query_as::<_, AvailabilitySlot>(
        "SELECT teacher_id, day_of_week, hour FROM teacher_availability
         WHERE term = ?1
         ORDER BY teacher_id, day_of_week, hour",
    )
    .bind(term)
    .fetch_all(pool)
    .await?)
}

/// Bir öğretmenin boş saatlerini tamamen değiştirir.
///
/// Izgara ekranı her kaydetmede tüm haftayı gönderir; tek tek ekleme/silme
/// yerine değiştirme, arayüzle veritabanının ayrışmasını imkânsız kılar.
/// İşlem atomiktir: yeni liste yazılamazsa eski liste de silinmez.
pub async fn replace_for_teacher(
    pool: &SqlitePool,
    teacher_id: i64,
    term: &str,
    slots: &[(i64, i64)],
) -> AppResult<()> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM teacher_availability WHERE teacher_id = ?1 AND term = ?2")
        .bind(teacher_id)
        .bind(term)
        .execute(&mut *tx)
        .await?;

    for (day_of_week, hour) in slots {
        sqlx::query(
            "INSERT INTO teacher_availability (teacher_id, day_of_week, hour, term)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(teacher_id)
        .bind(day_of_week)
        .bind(hour)
        .bind(term)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Bir dönemin müsaitliklerini bir sonraki döneme kopyalar.
/// Yeni öğretim yılına başlarken haftalık programı sıfırdan girmeyi önler.
/// Hedef dönemde zaten kayıt varsa hiçbir şey yapılmaz ve `false` döner.
pub async fn copy_term(pool: &SqlitePool, from_term: &str, to_term: &str) -> AppResult<bool> {
    let existing: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM teacher_availability WHERE term = ?1")
            .bind(to_term)
            .fetch_one(pool)
            .await?;

    if existing > 0 {
        return Ok(false);
    }

    sqlx::query(
        "INSERT INTO teacher_availability (teacher_id, day_of_week, hour, term)
         SELECT teacher_id, day_of_week, hour, ?1 FROM teacher_availability WHERE term = ?2",
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
    use crate::db::{init_pool, teachers};
    use crate::domain::models::NewTeacher;

    const TERM: &str = "2026-2027/1";

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
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

    #[tokio::test]
    async fn replace_writes_the_whole_week() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;

        replace_for_teacher(&pool, teacher_id, TERM, &[(1, 9), (1, 10), (3, 14)])
            .await
            .unwrap();

        let slots = list_for_teacher(&pool, teacher_id, TERM).await.unwrap();
        assert_eq!(slots.len(), 3);
        assert_eq!(slots[0], AvailabilitySlot { teacher_id, day_of_week: 1, hour: 9 });
        assert_eq!(slots[2], AvailabilitySlot { teacher_id, day_of_week: 3, hour: 14 });
    }

    /// İkinci kayıt öncekinin yerine geçer; birikmez.
    #[tokio::test]
    async fn replace_overwrites_previous_slots() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;

        replace_for_teacher(&pool, teacher_id, TERM, &[(1, 9), (1, 10)]).await.unwrap();
        replace_for_teacher(&pool, teacher_id, TERM, &[(2, 11)]).await.unwrap();

        let slots = list_for_teacher(&pool, teacher_id, TERM).await.unwrap();
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].day_of_week, 2);
    }

    /// Boş liste "hiç boş saati yok" demektir ve geçerlidir.
    #[tokio::test]
    async fn replace_with_empty_list_clears_availability() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;

        replace_for_teacher(&pool, teacher_id, TERM, &[(1, 9)]).await.unwrap();
        replace_for_teacher(&pool, teacher_id, TERM, &[]).await.unwrap();

        assert!(list_for_teacher(&pool, teacher_id, TERM).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn availability_is_scoped_to_term_and_teacher() {
        let (_dir, pool) = test_pool().await;
        let first = a_teacher(&pool, "Bir").await;
        let second = a_teacher(&pool, "Iki").await;

        replace_for_teacher(&pool, first, TERM, &[(1, 9)]).await.unwrap();
        replace_for_teacher(&pool, second, TERM, &[(2, 10), (2, 11)]).await.unwrap();
        replace_for_teacher(&pool, first, "2027-2028/1", &[(5, 15)]).await.unwrap();

        assert_eq!(list_for_teacher(&pool, first, TERM).await.unwrap().len(), 1);
        assert_eq!(list_for_teacher(&pool, second, TERM).await.unwrap().len(), 2);
        assert_eq!(list_all(&pool, TERM).await.unwrap().len(), 3);
        assert_eq!(list_all(&pool, "2027-2028/1").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn copy_term_duplicates_schedule_into_empty_term() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        replace_for_teacher(&pool, teacher_id, TERM, &[(1, 9), (1, 10)]).await.unwrap();

        assert!(copy_term(&pool, TERM, "2027-2028/1").await.unwrap());
        assert_eq!(list_all(&pool, "2027-2028/1").await.unwrap().len(), 2);
    }

    /// Hedef dönemde veri varsa kopyalama üzerine yazmaz.
    #[tokio::test]
    async fn copy_term_refuses_when_target_already_has_data() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        replace_for_teacher(&pool, teacher_id, TERM, &[(1, 9)]).await.unwrap();
        replace_for_teacher(&pool, teacher_id, "2027-2028/1", &[(5, 15)]).await.unwrap();

        assert!(!copy_term(&pool, TERM, "2027-2028/1").await.unwrap());
        let target = list_all(&pool, "2027-2028/1").await.unwrap();
        assert_eq!(target.len(), 1);
        assert_eq!(target[0].day_of_week, 5, "mevcut kayıt korunmalı");
    }

    /// Öğretmen silinince müsaitlikleri de silinir (ON DELETE CASCADE).
    #[tokio::test]
    async fn deleting_teacher_removes_availability() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        replace_for_teacher(&pool, teacher_id, TERM, &[(1, 9)]).await.unwrap();

        teachers::remove(&pool, teacher_id).await.unwrap();

        assert!(list_all(&pool, TERM).await.unwrap().is_empty());
    }
}
