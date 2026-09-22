use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Bir işletmenin dönemlik saat takdiri.
/// Atamadan ÖNCE belirlenir; atama yalnızca fiyatı belli işletmeyi yerleştirir.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CompanyTermHours {
    pub id: i64,
    pub company_id: i64,
    pub term: String,
    /// Kural tablosundan hesaplanan tavan, takdir anında dondurulmuştur.
    pub max_hours_snapshot: i64,
    /// Takdir edilen saat. Fahri ziyarette 0'dır.
    pub awarded_hours: i64,
    /// Fahri ziyaret: öğretmen gider, ek ders ücreti doğmaz.
    pub is_honorary: i64,
    /// Kilitli satırlar otomatik dağıtımda korunur.
    pub is_locked: i64,
    pub notes: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Takdir ekranından gelen tek satırlık değişiklik.
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

const SELECT_COLUMNS: &str = "id, company_id, term, max_hours_snapshot, awarded_hours, \
     is_honorary, is_locked, notes, created_at, updated_at";

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub async fn list(pool: &SqlitePool, term: &str) -> AppResult<Vec<CompanyTermHours>> {
    let sql = format!("SELECT {SELECT_COLUMNS} FROM company_term_hours WHERE term = ?1");
    Ok(sqlx::query_as::<_, CompanyTermHours>(&sql)
        .bind(term)
        .fetch_all(pool)
        .await?)
}

pub async fn get(
    pool: &SqlitePool,
    company_id: i64,
    term: &str,
) -> AppResult<Option<CompanyTermHours>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM company_term_hours WHERE company_id = ?1 AND term = ?2"
    );
    Ok(sqlx::query_as::<_, CompanyTermHours>(&sql)
        .bind(company_id)
        .bind(term)
        .fetch_optional(pool)
        .await?)
}

/// Dönemdeki toplam takdir edilen saat. Fahri satırlar 0 saat taşıdığı için
/// toplama doğal olarak katkı vermez.
pub async fn total_awarded(pool: &SqlitePool, term: &str) -> AppResult<i64> {
    let total: Option<i64> =
        sqlx::query_scalar("SELECT SUM(awarded_hours) FROM company_term_hours WHERE term = ?1")
            .bind(term)
            .fetch_one(pool)
            .await?;
    Ok(total.unwrap_or(0))
}

/// Tek bir satırı yazar; kayıt yoksa oluşturur.
///
/// Fahri işaretliyse takdir saati 0'a zorlanır — arayüz unutsa bile
/// "öğretmen gider, ücret doğmaz" kuralı burada garanti altına alınır.
pub async fn upsert(
    pool: &SqlitePool,
    term: &str,
    input: &HoursInput,
) -> AppResult<CompanyTermHours> {
    validate(input)?;

    let awarded = if input.is_honorary { 0 } else { input.awarded_hours };
    let now = now_iso();

    sqlx::query(
        "INSERT INTO company_term_hours
            (company_id, term, max_hours_snapshot, awarded_hours,
             is_honorary, is_locked, notes, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
         ON CONFLICT(company_id, term) DO UPDATE SET
            max_hours_snapshot = excluded.max_hours_snapshot,
            awarded_hours      = excluded.awarded_hours,
            is_honorary        = excluded.is_honorary,
            is_locked          = excluded.is_locked,
            notes              = excluded.notes,
            updated_at         = excluded.updated_at",
    )
    .bind(input.company_id)
    .bind(term)
    .bind(input.max_hours_snapshot)
    .bind(awarded)
    .bind(i64::from(input.is_honorary))
    .bind(i64::from(input.is_locked))
    .bind(&input.notes)
    .bind(&now)
    .execute(pool)
    .await?;

    get(pool, input.company_id, term)
        .await?
        .ok_or_else(|| AppError::Database("Takdir kaydı yazıldı ama okunamadı".into()))
}

/// Takdir ekranı her kaydetmede değişen satırların tamamını gönderir.
/// İşlem atomiktir: biri düşerse hiçbiri yazılmaz.
pub async fn save_many(
    pool: &SqlitePool,
    term: &str,
    inputs: &[HoursInput],
) -> AppResult<usize> {
    for input in inputs {
        validate(input)?;
    }

    let mut tx = pool.begin().await?;
    let now = now_iso();

    for input in inputs {
        let awarded = if input.is_honorary { 0 } else { input.awarded_hours };
        sqlx::query(
            "INSERT INTO company_term_hours
                (company_id, term, max_hours_snapshot, awarded_hours,
                 is_honorary, is_locked, notes, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
             ON CONFLICT(company_id, term) DO UPDATE SET
                max_hours_snapshot = excluded.max_hours_snapshot,
                awarded_hours      = excluded.awarded_hours,
                is_honorary        = excluded.is_honorary,
                is_locked          = excluded.is_locked,
                notes              = excluded.notes,
                updated_at         = excluded.updated_at",
        )
        .bind(input.company_id)
        .bind(term)
        .bind(input.max_hours_snapshot)
        .bind(awarded)
        .bind(i64::from(input.is_honorary))
        .bind(i64::from(input.is_locked))
        .bind(&input.notes)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(inputs.len())
}

fn validate(input: &HoursInput) -> AppResult<()> {
    if input.awarded_hours < 0 {
        return Err(AppError::Validation("Takdir edilen saat negatif olamaz".into()));
    }
    if input.max_hours_snapshot < 0 {
        return Err(AppError::Validation("Tavan negatif olamaz".into()));
    }
    // Fahri satırda saat zaten 0'a zorlanır; tavan kontrolü yalnızca ücretli
    // satırlar için anlamlıdır.
    if !input.is_honorary && input.awarded_hours > input.max_hours_snapshot {
        return Err(AppError::Validation(format!(
            "Takdir edilen {} saat, tavan {} saati aşamaz",
            input.awarded_hours, input.max_hours_snapshot
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn input(company_id: i64, awarded: i64, max: i64) -> HoursInput {
        HoursInput {
            company_id,
            max_hours_snapshot: max,
            awarded_hours: awarded,
            is_honorary: false,
            is_locked: false,
            notes: String::new(),
        }
    }

    #[tokio::test]
    async fn upsert_creates_then_updates_the_same_row() {
        let (_dir, pool) = test_pool().await;
        let company_id = a_company(&pool, "Test İşletme A").await;

        let created = upsert(&pool, TERM, &input(company_id, 6, 8)).await.unwrap();
        assert_eq!(created.awarded_hours, 6);

        let updated = upsert(&pool, TERM, &input(company_id, 4, 8)).await.unwrap();
        assert_eq!(updated.awarded_hours, 4);
        assert_eq!(updated.id, created.id, "aynı satır güncellenmeli");
        assert_eq!(list(&pool, TERM).await.unwrap().len(), 1);
    }

    /// Takdir tavanı aşamaz.
    #[tokio::test]
    async fn awarded_above_snapshot_is_rejected() {
        let (_dir, pool) = test_pool().await;
        let company_id = a_company(&pool, "Test İşletme A").await;

        let err = upsert(&pool, TERM, &input(company_id, 9, 8)).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    /// Fahri ziyarette saat 0'a zorlanır, arayüz ne gönderirse göndersin.
    #[tokio::test]
    async fn honorary_visit_forces_zero_hours() {
        let (_dir, pool) = test_pool().await;
        let company_id = a_company(&pool, "Test İşletme A").await;

        let mut honorary = input(company_id, 6, 8);
        honorary.is_honorary = true;

        let saved = upsert(&pool, TERM, &honorary).await.unwrap();
        assert_eq!(saved.awarded_hours, 0);
        assert_eq!(saved.is_honorary, 1);
    }

    /// Fahri satır havuz toplamına katkı vermez.
    #[tokio::test]
    async fn honorary_rows_do_not_count_toward_the_total() {
        let (_dir, pool) = test_pool().await;
        let paid = a_company(&pool, "Ucretli").await;
        let free = a_company(&pool, "Fahri").await;

        let mut honorary = input(free, 8, 8);
        honorary.is_honorary = true;

        save_many(&pool, TERM, &[input(paid, 6, 8), honorary]).await.unwrap();

        assert_eq!(total_awarded(&pool, TERM).await.unwrap(), 6);
    }

    #[tokio::test]
    async fn total_is_zero_when_nothing_awarded() {
        let (_dir, pool) = test_pool().await;
        assert_eq!(total_awarded(&pool, TERM).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn hours_are_scoped_to_term() {
        let (_dir, pool) = test_pool().await;
        let company_id = a_company(&pool, "Test İşletme A").await;

        upsert(&pool, TERM, &input(company_id, 6, 8)).await.unwrap();
        upsert(&pool, "2027-2028/1", &input(company_id, 2, 8)).await.unwrap();

        assert_eq!(total_awarded(&pool, TERM).await.unwrap(), 6);
        assert_eq!(total_awarded(&pool, "2027-2028/1").await.unwrap(), 2);
        assert_eq!(list(&pool, TERM).await.unwrap().len(), 1);
    }

    /// Toplu kayıt atomiktir: bir satır geçersizse hiçbiri yazılmaz.
    #[tokio::test]
    async fn save_many_rejects_the_whole_batch_on_invalid_row() {
        let (_dir, pool) = test_pool().await;
        let first = a_company(&pool, "Bir").await;
        let second = a_company(&pool, "Iki").await;

        let err = save_many(&pool, TERM, &[input(first, 6, 8), input(second, 99, 8)])
            .await
            .unwrap_err();

        assert!(matches!(err, AppError::Validation(_)));
        assert!(list(&pool, TERM).await.unwrap().is_empty(), "hiçbiri yazılmamalı");
    }

    #[tokio::test]
    async fn locked_flag_roundtrips() {
        let (_dir, pool) = test_pool().await;
        let company_id = a_company(&pool, "Test İşletme A").await;

        let mut locked = input(company_id, 6, 8);
        locked.is_locked = true;

        assert_eq!(upsert(&pool, TERM, &locked).await.unwrap().is_locked, 1);
    }

    /// Takdir kaydı olan bir işletme SİLİNMEZ, pasife alınır (spec §5.4).
    ///
    /// Eski test burada `company_term_hours.company_id`nin `ON DELETE
    /// CASCADE` ile sessizce silindiğini doğruluyordu — bu, teşhis edilen
    /// asıl kusurdu: `companies::remove` geçmişi (burada: takdir edilmiş
    /// saat) hiç denetlemeden sert siliyor, FK de takdir kaydını sessizce
    /// yok ediyordu. `remove` artık ÖNCE geçmişi denetler; kayıt hem
    /// veritabanında hem de `company_term_hours`te KALMALI.
    #[tokio::test]
    async fn deleting_a_company_with_hours_soft_deletes_it_instead_of_removing_the_hours() {
        let (_dir, pool) = test_pool().await;
        let company_id = a_company(&pool, "Test İşletme A").await;
        upsert(&pool, TERM, &input(company_id, 6, 8)).await.unwrap();

        let result = companies::remove(&pool, company_id).await.unwrap();

        assert!(result.soft_deleted, "takdir geçmişi olan işletme pasife alınmalı");
        assert_eq!(list(&pool, TERM).await.unwrap().len(), 1, "takdir kaydı silinmemeli");
    }
}
