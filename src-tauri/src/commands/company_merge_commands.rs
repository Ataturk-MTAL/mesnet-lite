use chrono::NaiveDate;
use tauri::State;

use crate::db::AppState;
use crate::domain::terms::today_local;
use crate::error::AppResult;
use crate::services::company_merge::{self, CompanyMergePreview, CompanyMergeSummary};

// Bu katman incedir: iş kuralları `services::company_merge`dedir (bkz.
// `commands/history_commands.rs`'teki aynı desen).

/// Birleştirmeyi YAZMADAN kaynaktan hedefe taşınacakları gösterir.
#[tauri::command]
pub async fn preview_company_merge(state: State<'_, AppState>, from_company_id: i64, into_company_id: i64) -> AppResult<CompanyMergePreview> {
    company_merge::preview_company_merge(&state.pool, from_company_id, into_company_id).await
}

/// Kaynaktaki her şeyi hedefe taşır, kaynağı pasifleştirir.
#[tauri::command]
pub async fn apply_company_merge(
    state: State<'_, AppState>,
    from_company_id: i64,
    into_company_id: i64,
    effective_date: Option<NaiveDate>,
    reason: String,
) -> AppResult<CompanyMergeSummary> {
    company_merge::apply_company_merge(&state.pool, from_company_id, into_company_id, effective_date, reason, today_local()).await
}
