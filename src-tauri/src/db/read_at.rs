//! Tarih itibarıyla okuma kipi (spec §6, plan Görev R5).
//!
//! Beş tarih aralıklı projeksiyonun (`student_placements`, `company_hour_periods`,
//! `coordination_periods`, `teacher_load_periods`, `teacher_schedule_periods`)
//! her okuyucusu bu tek tipten geçer:
//!
//! - `Latest`: BUGÜNKÜ davranışın birebir aynısı. Projeksiyonlar `valid_to IS
//!   NULL` (AÇIK satır) ile, öğretmen yükü ve şeflik saati
//!   `teaching_load::current_as_of` (bugünün döneme sıkıştırılmış hâli) ile
//!   okunur. Gerekçe: ileri tarihli kaydedilen bir değişiklik düzenleme
//!   görünümünde HEMEN görünmeli — kullanıcı kararı.
//! - `AsOf(d)`: beş projeksiyonun HEPSİ `valid_from <= d AND (valid_to IS
//!   NULL OR d < valid_to)` ile; yük ve şeflik saati doğrudan `d` ile okunur.
//!
//! Tarihçesi OLMAYAN bilgiler (`term_branch_hours`, `class_workplace_days`,
//! `company_hour_rules`, `students` satırının kendisi — sınıf/dal/var olma,
//! `teachers`/`companies` kimliği ve `is_active`, mesafe, ayarlar) HER kipte
//! güncel okunur; bunlar bu tipin kapsamı DIŞINDADIR ve onlar için tarihçe
//! İCAT EDİLMEZ.

use chrono::NaiveDate;
use sqlx::SqlitePool;

use crate::domain::terms::parse_date;
use crate::error::{AppError, AppResult};

use super::{teaching_load, terms};

/// Bir okuma komutunun hangi güne göre çalıştığı.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadAt {
    /// Projeksiyonların AÇIK (`valid_to IS NULL`) satırı; yük/şeflik bugünün
    /// döneme sıkıştırılmış hâlinden (`teaching_load::current_as_of`).
    Latest,
    /// Projeksiyonların verilen gündeki satırı; yük/şeflik doğrudan bu gün.
    AsOf(NaiveDate),
}

impl ReadAt {
    /// Komut sınırındaki `Option<String>` girdisini çözer (sınırda doğrulama,
    /// MADDE gerekmez — burada saf tarih/tarih-aralığı kontrolüdür).
    ///
    /// `None` → `Latest`. `Some` biçimi bozuksa (`YYYY-MM-DD` değilse) ya da
    /// aktif dönemin `[start, end]` aralığı dışındaysa `AppError::Validation`
    /// döner: dışarıdan gelen tarih hiçbir zaman doğrulanmadan kullanılmaz.
    pub async fn resolve(pool: &SqlitePool, term: &str, as_of: Option<String>) -> AppResult<ReadAt> {
        let Some(raw) = as_of else {
            return Ok(ReadAt::Latest);
        };
        let date = parse_date(&raw)?;

        let mut conn = pool.acquire().await?;
        let dates = terms::get_in(&mut conn, term).await?;
        if date < dates.start || date > dates.end {
            return Err(AppError::Validation(format!(
                "Seçilen tarih dönem aralığında değil: dönem {} – {} arasındadır.",
                dates.start, dates.end
            )));
        }
        Ok(ReadAt::AsOf(date))
    }

    /// Beş projeksiyonun ortak WHERE koşulu, sütun adları `alias` ile
    /// nitelenmiş (ör. `"cp."`; niteleme gerekmiyorsa boş dize). `param`, bu
    /// sorgudaki bir sonraki BOŞ `?N` numarasıdır; `AsOf` bunu yarı açık
    /// aralığın iki ucu için İKİ KEZ yazar ama tek bir `?N` olduğu için
    /// (SQLite numaralı parametreleri konumdan bağımsız aynı değere karşılık
    /// gelir) çağıran TEK bir `.bind()` yapar — bkz. `value`.
    pub fn condition_with_alias(&self, alias: &str, param: usize) -> String {
        match self {
            ReadAt::Latest => format!("{alias}valid_to IS NULL"),
            ReadAt::AsOf(_) => format!(
                "{alias}valid_from <= ?{param} AND ({alias}valid_to IS NULL OR ?{param} < {alias}valid_to)"
            ),
        }
    }

    /// Niteleme gerekmeyen (JOIN'siz, tek tablolu) sorgular için kısayol.
    pub fn condition(&self, param: usize) -> String {
        self.condition_with_alias("", param)
    }

    /// `condition`/`condition_with_alias`in `AsOf` kolunun bağlaması gereken
    /// TEK değer. `Latest`te bağlanacak bir şey yoktur (sorguda `?N` hiç
    /// geçmez); çağıran `if let Some(d) = read_at.value() { query =
    /// query.bind(d); }` desenini kullanır.
    pub fn value(&self) -> Option<NaiveDate> {
        match self {
            ReadAt::Latest => None,
            ReadAt::AsOf(d) => Some(*d),
        }
    }

    /// Öğretmen yükü ve şeflik saati hesaplarının (`teaching_load::pool_breakdown`,
    /// `teachers::list_with_load_as_of`) kullandığı gün. `Latest`te bugünün
    /// döneme sıkıştırılmış hâli (`teaching_load::current_as_of`); `AsOf`ta
    /// doğrudan verilen tarih — zaten `resolve`de dönem aralığında doğrulandı.
    pub async fn hours_as_of(&self, pool: &SqlitePool, term: &str) -> AppResult<NaiveDate> {
        match self {
            ReadAt::Latest => teaching_load::current_as_of(pool, term).await,
            ReadAt::AsOf(d) => Ok(*d),
        }
    }
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

    /// Seed'in aktif dönemi "2026-2027/1", varsayılan (onaysız) aralığı
    /// 2026-09-01 – 2027-01-31'dir (`TermDates::default_for`).
    const TERM: &str = "2026-2027/1";

    #[tokio::test]
    async fn none_resolves_to_latest() {
        let (_dir, pool) = test_pool().await;
        assert_eq!(ReadAt::resolve(&pool, TERM, None).await.unwrap(), ReadAt::Latest);
    }

    #[tokio::test]
    async fn a_date_within_the_term_resolves_to_as_of() {
        let (_dir, pool) = test_pool().await;
        let resolved = ReadAt::resolve(&pool, TERM, Some("2026-10-15".into())).await.unwrap();
        assert_eq!(resolved, ReadAt::AsOf(NaiveDate::from_ymd_opt(2026, 10, 15).unwrap()));
    }

    #[tokio::test]
    async fn a_date_outside_the_term_is_rejected() {
        let (_dir, pool) = test_pool().await;
        let err = ReadAt::resolve(&pool, TERM, Some("2027-06-01".into())).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn a_malformed_date_is_rejected() {
        let (_dir, pool) = test_pool().await;
        let err = ReadAt::resolve(&pool, TERM, Some("15-10-2026".into())).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn latest_condition_has_no_placeholder() {
        assert_eq!(ReadAt::Latest.condition(2), "valid_to IS NULL");
        assert!(ReadAt::Latest.value().is_none());
    }

    #[test]
    fn as_of_condition_reuses_the_same_placeholder_twice() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        let read_at = ReadAt::AsOf(d);
        assert_eq!(
            read_at.condition_with_alias("cp.", 2),
            "cp.valid_from <= ?2 AND (cp.valid_to IS NULL OR ?2 < cp.valid_to)"
        );
        assert_eq!(read_at.value(), Some(d));
    }
}
