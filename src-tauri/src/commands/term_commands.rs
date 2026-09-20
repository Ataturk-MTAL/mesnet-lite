use chrono::NaiveDate;
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::State;

use crate::db::{terms, AppState};
use crate::domain::terms::{parse_date, today_local, TermDates};
use crate::error::AppResult;

// Bu katman incedir: yalnızca sınır doğrulaması ve şekil dönüşümü yapar.
// Tarih kuralları `domain::terms`, güncelleme kuralları `db::terms` içindedir.

/// `list_terms_with_dates` ve `update_term_dates` yanıt öğesi (spec §8).
/// Üç türetilmiş alan (`isPlanning`, `defaultAsOf`, `earliestAllowedDate`)
/// `domain::terms::TermDates` yöntemlerinden gelir; burada yeniden hesaplanmaz.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TermWithDates {
    pub term: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub dates_confirmed: bool,
    pub is_planning: bool,
    pub default_as_of: NaiveDate,
    pub earliest_allowed_date: NaiveDate,
}

impl TermWithDates {
    fn from_dates(dates: &TermDates, today: NaiveDate) -> Self {
        Self {
            term: dates.term.clone(),
            start_date: dates.start,
            end_date: dates.end,
            dates_confirmed: dates.dates_confirmed,
            is_planning: dates.is_planning(today),
            default_as_of: dates.default_as_of(today),
            earliest_allowed_date: dates.earliest_allowed(today),
        }
    }
}

async fn list_with_dates(pool: &SqlitePool, today: NaiveDate) -> AppResult<Vec<TermWithDates>> {
    let all = terms::list(pool).await?;
    Ok(all.iter().map(|dates| TermWithDates::from_dates(dates, today)).collect())
}

/// Tarihler sınırda ayrıştırılır (`YYYY-MM-DD` dışı reddedilir); kuralların
/// kendisi `terms::update_dates`'tedir.
async fn update_with_dates(
    pool: &SqlitePool,
    term: &str,
    start_date: &str,
    end_date: &str,
    confirm: bool,
    today: NaiveDate,
) -> AppResult<TermWithDates> {
    let start = parse_date(start_date)?;
    let end = parse_date(end_date)?;
    let updated = terms::update_dates(pool, term, start, end, confirm, today).await?;
    Ok(TermWithDates::from_dates(&updated, today))
}

#[tauri::command]
pub async fn list_terms_with_dates(state: State<'_, AppState>) -> AppResult<Vec<TermWithDates>> {
    list_with_dates(&state.pool, today_local()).await
}

#[tauri::command]
pub async fn update_term_dates(
    state: State<'_, AppState>,
    term: String,
    start_date: String,
    end_date: String,
    confirm: bool,
) -> AppResult<TermWithDates> {
    update_with_dates(&state.pool, &term, &start_date, &end_date, confirm, today_local()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;
    use crate::error::AppError;
    use serde_json::json;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    /// Dönem başladıysa: bugün varsayılan tarih, ayın 1'i en erken izinli gün.
    #[tokio::test]
    async fn list_derives_the_three_computed_fields_for_a_started_term() {
        let (_dir, pool) = test_pool().await;

        let listed = list_with_dates(&pool, ymd(2026, 11, 10)).await.unwrap();

        assert_eq!(listed.len(), 1);
        let term = &listed[0];
        assert_eq!(term.term, "2026-2027/1");
        assert_eq!((term.start_date, term.end_date), (ymd(2026, 9, 1), ymd(2027, 1, 31)));
        assert!(!term.dates_confirmed, "göçten gelen tarihler onaysızdır");
        assert!(!term.is_planning);
        assert_eq!(term.default_as_of, ymd(2026, 11, 10));
        assert_eq!(term.earliest_allowed_date, ymd(2026, 11, 1));
    }

    /// Planlamada: varsayılan gün dönem başına sıkıştırılır, en erken gün dönem başıdır.
    #[tokio::test]
    async fn list_derives_the_three_computed_fields_for_a_planning_term() {
        let (_dir, pool) = test_pool().await;

        let listed = list_with_dates(&pool, ymd(2026, 8, 15)).await.unwrap();

        assert!(listed[0].is_planning);
        assert_eq!(listed[0].default_as_of, ymd(2026, 9, 1));
        assert_eq!(listed[0].earliest_allowed_date, ymd(2026, 9, 1));
    }

    #[test]
    fn term_with_dates_serializes_to_the_spec_shape() {
        let dates = TermDates { term: "2026-2027/1".into(), start: ymd(2026, 9, 1), end: ymd(2027, 1, 31), dates_confirmed: true };

        let wire = serde_json::to_value(TermWithDates::from_dates(&dates, ymd(2026, 11, 10))).unwrap();

        assert_eq!(
            wire,
            json!({
                "term": "2026-2027/1", "startDate": "2026-09-01", "endDate": "2027-01-31", "datesConfirmed": true,
                "isPlanning": false, "defaultAsOf": "2026-11-10", "earliestAllowedDate": "2026-11-01",
            })
        );
    }

    #[tokio::test]
    async fn update_returns_the_updated_record_with_derived_fields() {
        let (_dir, pool) = test_pool().await;

        let updated = update_with_dates(&pool, "2026-2027/1", "2026-09-10", "2027-01-29", true, ymd(2026, 8, 15)).await.unwrap();

        assert_eq!((updated.start_date, updated.end_date), (ymd(2026, 9, 10), ymd(2027, 1, 29)));
        assert!(updated.dates_confirmed);
        assert!(updated.is_planning);
        assert_eq!(updated.earliest_allowed_date, ymd(2026, 9, 10));
    }

    /// Bozuk tarih metni sınırda reddedilir; hiçbir şey yazılmaz.
    #[tokio::test]
    async fn update_rejects_unparseable_dates_without_writing() {
        let (_dir, pool) = test_pool().await;

        for (start, end) in [("garbage", "2027-01-31"), ("2026-09-01", "31.01.2027"), ("2026-13-01", "2027-01-31")] {
            let result = update_with_dates(&pool, "2026-2027/1", start, end, true, ymd(2026, 8, 15)).await;
            let is_parse_error = matches!(&result, Err(AppError::Validation(message)) if message.contains("Geçersiz tarih"));
            assert!(is_parse_error, "ayrıştırma hatası beklenirdi ({start}, {end}): {result:?}");
        }
        let listed = list_with_dates(&pool, ymd(2026, 8, 15)).await.unwrap();
        assert!(!listed[0].dates_confirmed, "reddedilen istek onayı değiştirmemeli");
    }

    /// Önceki bir aya düşen gün değişikliği `terms::update_dates`'in kuralıyla reddedilir.
    #[tokio::test]
    async fn update_propagates_the_past_month_rule() {
        let (_dir, pool) = test_pool().await;

        let result = update_with_dates(&pool, "2026-2027/1", "2026-09-10", "2027-01-31", true, ymd(2026, 11, 10)).await;

        assert!(matches!(result, Err(AppError::Validation(_))), "{result:?}");
    }
}
