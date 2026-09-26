//! Bir işletmenin dönemlik ek ders saati takdirinin OKUMA katmanı.
//!
//! **DONMUŞ, YALNIZ AKTARIM OKUR.** Bu dosyanın eski yazıcıları (`upsert`,
//! `save_many`) KALDIRILMIŞTIR — brief teşhisi: Saat Ayarları panosu bunlara,
//! yani `company_term_hours` tablosuna yazıyordu; tarihçe servisi
//! (`services::change_service::execute_change`) ise yalnız
//! `company_hour_periods` projeksiyonunu güncelliyordu. İkisi hiç
//! eşleşmiyordu (canlı veritabanında: eski tabloda 149 saat, açık
//! projeksiyonda 4 saat). Artık TEK yazma yolu tarihçe kapısıdır
//! (`ChangeCommand::SetCompanyHours` → `db/projection/sync.rs::sync_company_hours`).
//!
//! `company_term_hours` tablosu geri dönüş güvenliği için DÜŞÜRÜLMEDİ, ama
//! bu iş tarihinden sonra bir daha YAZILMAZ. Tek okuyucusu, açılışta bir kez
//! çalışan tek seferlik aktarımdır (`services::legacy_reconcile`). Aşağıdaki
//! her fonksiyon `read_at`e göre okur (spec §6): `Latest`te
//! `company_hour_periods`in AÇIK (`valid_to IS NULL`) satırı, `AsOf(d)`te `d`
//! gününde geçerli satır.
use crate::db::read_at::ReadAt;
use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Bir işletmenin dönemlik saat takdiri (açık projeksiyon satırından).
/// Atamadan ÖNCE belirlenir; atama yalnızca fiyatı belli işletmeyi yerleştirir.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CompanyTermHours {
    pub id: i64,
    pub company_id: i64,
    pub term: String,
    /// Karar anında `decide::company::set_company_hours`in sunucuda
    /// hesapladığı tavan (`cap_for`). Artık istemciden GELMEZ.
    pub max_hours_snapshot: i64,
    /// Takdir edilen saat. Fahri ziyarette 0'dır.
    pub awarded_hours: i64,
    /// Fahri ziyaret: öğretmen gider, ek ders ücreti doğmaz.
    pub is_honorary: i64,
    /// Kilitli satırlar otomatik dağıtımda korunur.
    pub is_locked: i64,
    pub notes: String,
}

/// Takdir ekranından gelen tek satırlık değişiklik — `commands::hours_commands::save_company_hours`nin
/// Tauri imzası bunu KORUR (sözleşme). `maxHoursSnapshot` artık YOK SAYILIR:
/// tavan sunucuda (`decide::company::set_company_hours` → `cap_for`) hesaplanır.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoursInput {
    pub company_id: i64,
    pub max_hours_snapshot: i64,
    pub awarded_hours: i64,
    pub is_honorary: bool,
    pub is_locked: bool,
    pub notes: String,
}

const SELECT_COLUMNS: &str =
    "id, company_id, term, max_hours_snapshot, awarded_hours, is_honorary, is_locked, notes";

/// Dönemin tüm işletmeleri için `read_at`e göre geçerli saat takdiri satırları.
pub async fn list(pool: &SqlitePool, term: &str, read_at: &ReadAt) -> AppResult<Vec<CompanyTermHours>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM company_hour_periods WHERE term = ?1 AND {}",
        read_at.condition(2)
    );
    let mut query = sqlx::query_as::<_, CompanyTermHours>(&sql).bind(term);
    if let Some(date) = read_at.value() {
        query = query.bind(date);
    }
    Ok(query.fetch_all(pool).await?)
}

/// Dönemdeki toplam takdir edilen saat, `read_at`e göre. Fahri satırlar 0
/// saat taşıdığı için toplama doğal olarak katkı vermez.
pub async fn total_awarded(pool: &SqlitePool, term: &str, read_at: &ReadAt) -> AppResult<i64> {
    let sql = format!(
        "SELECT SUM(awarded_hours) FROM company_hour_periods WHERE term = ?1 AND {}",
        read_at.condition(2)
    );
    let mut query = sqlx::query_scalar(&sql).bind(term);
    if let Some(date) = read_at.value() {
        query = query.bind(date);
    }
    let total: Option<i64> = query.fetch_one(pool).await?;
    Ok(total.unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::legacy_seed_test_support::seed_hours;
    use crate::db::{companies, init_pool};
    use crate::domain::models::NewCompany;

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

    /// Okuma katmanı artık `company_hour_periods`in AÇIK satırını görür;
    /// donmuş `company_term_hours`e hiç bakmaz.
    #[tokio::test]
    async fn list_and_total_read_the_open_projection_row() {
        let (_dir, pool) = test_pool().await;
        let company_id = a_company(&pool, "Test İşletme A").await;
        seed_hours(&pool, TERM, company_id, 6, false).await;

        let rows = list(&pool, TERM, &ReadAt::Latest).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].awarded_hours, 6);
        assert_eq!(total_awarded(&pool, TERM, &ReadAt::Latest).await.unwrap(), 6);
    }

    /// Fahri satır havuz toplamına katkı vermez (`legacy_seed_test_support::seed_hours`
    /// eski `upsert` gibi saati 0'a zorlar).
    #[tokio::test]
    async fn honorary_rows_do_not_count_toward_the_total() {
        let (_dir, pool) = test_pool().await;
        let paid = a_company(&pool, "Ucretli").await;
        let free = a_company(&pool, "Fahri").await;
        seed_hours(&pool, TERM, paid, 6, false).await;
        seed_hours(&pool, TERM, free, 8, true).await;

        assert_eq!(total_awarded(&pool, TERM, &ReadAt::Latest).await.unwrap(), 6);
    }

    #[tokio::test]
    async fn total_is_zero_when_nothing_awarded() {
        let (_dir, pool) = test_pool().await;
        assert_eq!(total_awarded(&pool, TERM, &ReadAt::Latest).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn hours_are_scoped_to_term() {
        let (_dir, pool) = test_pool().await;
        let company_id = a_company(&pool, "Test İşletme A").await;
        seed_hours(&pool, TERM, company_id, 6, false).await;

        assert_eq!(total_awarded(&pool, TERM, &ReadAt::Latest).await.unwrap(), 6);
        assert_eq!(list(&pool, TERM, &ReadAt::Latest).await.unwrap().len(), 1);
        assert!(list(&pool, "2027-2028/1", &ReadAt::Latest).await.unwrap().is_empty());
    }

    /// Takdir kaydı olan bir işletme SİLİNMEZ, pasife alınır (spec §5.4);
    /// bu davranış artık `company_hour_periods` üzerinden korunur.
    #[tokio::test]
    async fn deleting_a_company_with_hours_soft_deletes_it_instead_of_removing_the_hours() {
        let (_dir, pool) = test_pool().await;
        let company_id = a_company(&pool, "Test İşletme A").await;
        seed_hours(&pool, TERM, company_id, 6, false).await;

        let result = companies::remove(&pool, company_id).await.unwrap();

        assert!(result.soft_deleted, "takdir geçmişi olan işletme pasife alınmalı");
        assert_eq!(list(&pool, TERM, &ReadAt::Latest).await.unwrap().len(), 1, "takdir kaydı silinmemeli");
    }
}
