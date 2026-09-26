//! Bir işletmenin bir öğretmenin haftalık programındaki yerleşiminin OKUMA
//! katmanı.
//!
//! **DONMUŞ, YALNIZ AKTARIM OKUR.** Bu dosyanın eski yazıcıları (`assign`,
//! `unassign`, `clear_term`) KALDIRILMIŞTIR — brief teşhisi: Dağıtım panosu
//! bunlara, yani `assignments` tablosuna yazıyordu; tarihçe servisi ise
//! yalnız `coordination_periods` projeksiyonunu güncelliyordu (canlı
//! veritabanında: eski tabloda 27 atama, `coordination_periods` BOŞTU).
//! Artık TEK yazma yolu tarihçe kapısıdır (`ChangeCommand::AssignCoordinators`/
//! `EndCoordination`/`ClearCoordination` → `db/projection/sync.rs::sync_coordination`).
//!
//! `assignments` tablosu geri dönüş güvenliği için DÜŞÜRÜLMEDİ, ama bu iş
//! tarihinden sonra bir daha YAZILMAZ. Tek okuyucusu, açılışta bir kez
//! çalışan tek seferlik aktarımdır (`services::legacy_reconcile`). Aşağıdaki
//! her fonksiyon artık `coordination_periods`in AÇIK (`valid_to IS NULL`)
//! satırını, saat için `company_hour_periods`in AÇIK satırını okur.
//!
//! Saat burada TUTULMAZ: ek ders saati işletmenin dönemlik takdiridir
//! (`company_hour_periods`). Burada yalnızca ziyaretin ne zaman yapılacağı durur.
//!
//! Panolarda okunan fonksiyonlar `read_at`e göre çalışır (spec §6): `Latest`te
//! iki projeksiyonun da AÇIK satırı, `AsOf(d)`te ikisinin de `d` gününde
//! geçerli satırı. `get_for_company` bunun DIŞINDADIR: yalnız yazma yolunun
//! (`unassign_company_for_term`) idempotentlik kontrolüdür, HER ZAMAN Latest
//! davranır.
use crate::db::read_at::ReadAt;
use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Bir işletmenin bir öğretmenin haftalık programındaki yerleşimi (açık
/// projeksiyon satırından).
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
}

/// `assign_company` komutunun Tauri imzası bunu KORUR (sözleşme); artık
/// `ChangeCommand::AssignCoordinators`'a çevrilip tarihçe kapısından geçer.
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

const SELECT_COLUMNS: &str = "id, teacher_id, company_id, term, visit_day, visit_hour, is_forced, force_reason";

pub async fn list(pool: &SqlitePool, term: &str, read_at: &ReadAt) -> AppResult<Vec<Assignment>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM coordination_periods WHERE term = ?1 AND {}
         ORDER BY teacher_id, visit_day, visit_hour",
        read_at.condition(2)
    );
    let mut query = sqlx::query_as::<_, Assignment>(&sql).bind(term);
    if let Some(date) = read_at.value() {
        query = query.bind(date);
    }
    Ok(query.fetch_all(pool).await?)
}

pub async fn get_for_company(pool: &SqlitePool, company_id: i64, term: &str) -> AppResult<Option<Assignment>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM coordination_periods WHERE company_id = ?1 AND term = ?2 AND valid_to IS NULL"
    );
    Ok(sqlx::query_as::<_, Assignment>(&sql).bind(company_id).bind(term).fetch_optional(pool).await?)
}

/// Öğretmen başına toplam ek ders saati.
///
/// Saat atamadan değil, işletmenin dönemlik takdirinden gelir; bu yüzden iki
/// AÇIK projeksiyon LEFT JOIN edilir. Takdiri girilmemiş (açık satırı olmayan)
/// işletme 0 saat sayılır.
pub async fn awarded_hours_by_teacher(pool: &SqlitePool, term: &str, read_at: &ReadAt) -> AppResult<Vec<(i64, i64)>> {
    // Aynı `read_at` HER İKİ projeksiyona da uygulanır: `AsOf(d)`te koordinatörlük
    // VE saat takdiri aynı `d` gününde geçerli olmalı, aksi hâlde biri
    // güncel biri geçmiş bir günün karışımı yanlış bir toplam üretir.
    let sql = format!(
        "SELECT cp.teacher_id, COALESCE(SUM(chp.awarded_hours), 0)
         FROM coordination_periods cp
         LEFT JOIN company_hour_periods chp
                ON chp.company_id = cp.company_id AND chp.term = cp.term AND {chp_cond}
         WHERE cp.term = ?1 AND {cp_cond}
         GROUP BY cp.teacher_id",
        chp_cond = read_at.condition_with_alias("chp.", 2),
        cp_cond = read_at.condition_with_alias("cp.", 2),
    );
    let mut query = sqlx::query_as(&sql).bind(term);
    if let Some(date) = read_at.value() {
        query = query.bind(date);
    }
    let rows: Vec<(i64, i64)> = query.fetch_all(pool).await?;
    Ok(rows)
}

/// Öğretmenin gün başına toplam ek ders saati (OÖKY MADDE 88 günlük sınırı).
pub async fn awarded_hours_by_teacher_and_day(
    pool: &SqlitePool,
    term: &str,
    read_at: &ReadAt,
) -> AppResult<Vec<(i64, i64, i64)>> {
    let sql = format!(
        "SELECT cp.teacher_id, cp.visit_day, COALESCE(SUM(chp.awarded_hours), 0)
         FROM coordination_periods cp
         LEFT JOIN company_hour_periods chp
                ON chp.company_id = cp.company_id AND chp.term = cp.term AND {chp_cond}
         WHERE cp.term = ?1 AND {cp_cond}
         GROUP BY cp.teacher_id, cp.visit_day",
        chp_cond = read_at.condition_with_alias("chp.", 2),
        cp_cond = read_at.condition_with_alias("cp.", 2),
    );
    let mut query = sqlx::query_as(&sql).bind(term);
    if let Some(date) = read_at.value() {
        query = query.bind(date);
    }
    let rows: Vec<(i64, i64, i64)> = query.fetch_all(pool).await?;
    Ok(rows)
}

/// Dönemde atanmış toplam ek ders saati.
pub async fn total_assigned_hours(pool: &SqlitePool, term: &str, read_at: &ReadAt) -> AppResult<i64> {
    let sql = format!(
        "SELECT SUM(chp.awarded_hours)
         FROM coordination_periods cp
         JOIN company_hour_periods chp
           ON chp.company_id = cp.company_id AND chp.term = cp.term AND {chp_cond}
         WHERE cp.term = ?1 AND {cp_cond}",
        chp_cond = read_at.condition_with_alias("chp.", 2),
        cp_cond = read_at.condition_with_alias("cp.", 2),
    );
    let mut query = sqlx::query_scalar(&sql).bind(term);
    if let Some(date) = read_at.value() {
        query = query.bind(date);
    }
    let total: Option<i64> = query.fetch_one(pool).await?;
    Ok(total.unwrap_or(0))
}

pub async fn assigned_company_ids(pool: &SqlitePool, term: &str, read_at: &ReadAt) -> AppResult<Vec<i64>> {
    let sql = format!(
        "SELECT company_id FROM coordination_periods WHERE term = ?1 AND {}",
        read_at.condition(2)
    );
    let mut query = sqlx::query_as(&sql).bind(term);
    if let Some(date) = read_at.value() {
        query = query.bind(date);
    }
    let rows: Vec<(i64,)> = query.fetch_all(pool).await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::legacy_seed_test_support::{seed_coordinator, seed_hours};
    use crate::db::{companies, init_pool, teachers};
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
                district: String::new(),
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

    #[tokio::test]
    async fn list_reads_the_open_coordination_period() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        let company_id = a_company(&pool, "Test İşletme A").await;
        seed_coordinator(&pool, TERM, company_id, teacher_id, 3, 4, false, None).await;

        let rows = list(&pool, TERM, &ReadAt::Latest).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].visit_day, 3);
        assert_eq!(rows[0].visit_hour, 4);
        assert!(get_for_company(&pool, company_id, TERM).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn get_for_company_is_none_without_an_open_period() {
        let (_dir, pool) = test_pool().await;
        let company_id = a_company(&pool, "Test İşletme A").await;
        assert!(get_for_company(&pool, company_id, TERM).await.unwrap().is_none());
    }

    /// Saat atamadan değil, işletmenin AÇIK takdirinden gelir.
    #[tokio::test]
    async fn hours_come_from_the_open_hours_period_not_from_the_assignment() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        let company_id = a_company(&pool, "Test İşletme A").await;

        seed_coordinator(&pool, TERM, company_id, teacher_id, 1, 1, false, None).await;
        assert_eq!(total_assigned_hours(&pool, TERM, &ReadAt::Latest).await.unwrap(), 0, "takdir henüz girilmedi");

        seed_hours(&pool, TERM, company_id, 6, false).await;
        assert_eq!(total_assigned_hours(&pool, TERM, &ReadAt::Latest).await.unwrap(), 6);
    }

    #[tokio::test]
    async fn awarded_hours_by_teacher_sums_across_companies() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        let first = a_company(&pool, "Bir").await;
        let second = a_company(&pool, "Iki").await;

        seed_hours(&pool, TERM, first, 6, false).await;
        seed_hours(&pool, TERM, second, 4, false).await;
        seed_coordinator(&pool, TERM, first, teacher_id, 1, 1, false, None).await;
        seed_coordinator(&pool, TERM, second, teacher_id, 2, 1, false, None).await;

        let rows = awarded_hours_by_teacher(&pool, TERM, &ReadAt::Latest).await.unwrap();
        assert_eq!(rows, vec![(teacher_id, 10)]);
    }

    /// Günlük sınır denetimi için gün bazında toplam gerekir.
    #[tokio::test]
    async fn awarded_hours_by_day_groups_per_day() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        let first = a_company(&pool, "Bir").await;
        let second = a_company(&pool, "Iki").await;

        seed_hours(&pool, TERM, first, 6, false).await;
        seed_hours(&pool, TERM, second, 4, false).await;
        seed_coordinator(&pool, TERM, first, teacher_id, 3, 1, false, None).await;
        seed_coordinator(&pool, TERM, second, teacher_id, 3, 7, false, None).await;

        let rows = awarded_hours_by_teacher_and_day(&pool, TERM, &ReadAt::Latest).await.unwrap();
        assert_eq!(rows, vec![(teacher_id, 3, 10)], "aynı günde 10 saat");
    }

    #[tokio::test]
    async fn assigned_company_ids_lists_open_companies_only() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        let company_id = a_company(&pool, "Test İşletme A").await;
        assert!(assigned_company_ids(&pool, TERM, &ReadAt::Latest).await.unwrap().is_empty());

        seed_coordinator(&pool, TERM, company_id, teacher_id, 1, 1, false, None).await;
        assert_eq!(assigned_company_ids(&pool, TERM, &ReadAt::Latest).await.unwrap(), vec![company_id]);
    }

    #[tokio::test]
    async fn assignments_are_scoped_to_term() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        let company_id = a_company(&pool, "Test İşletme A").await;
        crate::db::terms::ensure(&pool, "2027-2028/1").await.unwrap();

        seed_coordinator(&pool, TERM, company_id, teacher_id, 1, 1, false, None).await;

        assert_eq!(list(&pool, TERM, &ReadAt::Latest).await.unwrap().len(), 1);
        assert!(list(&pool, "2027-2028/1", &ReadAt::Latest).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn forced_assignment_carries_its_reason() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        let company_id = a_company(&pool, "Test İşletme A").await;

        seed_coordinator(&pool, TERM, company_id, teacher_id, 1, 1, true, Some("Ulaşım zorunluluğu".into())).await;

        let row = get_for_company(&pool, company_id, TERM).await.unwrap().unwrap();
        assert_eq!(row.is_forced, 1);
        assert_eq!(row.force_reason.as_deref(), Some("Ulaşım zorunluluğu"));
    }
}
