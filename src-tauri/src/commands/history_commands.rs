use tauri::State;

use crate::db::AppState;
use crate::domain::history::decide::ChangeRequest;
use crate::domain::terms::today_local;
use crate::error::AppResult;
use crate::services::change_service::{execute_change, ChangeMode, ChangeOutcome};
use crate::services::history_service::{self, HistoryEventEntry, HistoryFilter, HistoryPage};

// Bu katman incedir: `today` tek yerde (komut sınırında) hesaplanır ve
// hizmet katmanına parametre olarak verilir; iş kuralları `services/` ve
// `domain/` içindedir.

/// Değişikliği YAZMADAN etkisini gösterir. Dönen `highWater`, sonraki
/// `commit_change`'e `expectedHighWater` olarak verilir.
#[tauri::command]
pub async fn preview_change(state: State<'_, AppState>, request: ChangeRequest) -> AppResult<ChangeOutcome> {
    execute_change(&state.pool, request, ChangeMode::Preview, today_local()).await
}

/// Değişikliği kaydeder. `expectedHighWater` doluysa ve önizlemeden sonra
/// günlük ilerlediyse `stale` döner; `null` ise bayat kontrolü yapılmaz.
#[tauri::command]
pub async fn commit_change(
    state: State<'_, AppState>,
    request: ChangeRequest,
    expected_high_water: Option<i64>,
) -> AppResult<ChangeOutcome> {
    let mode = ChangeMode::Commit { expected_high_water };
    execute_change(&state.pool, request, mode, today_local()).await
}

#[tauri::command]
pub async fn list_history(state: State<'_, AppState>, filter: HistoryFilter) -> AppResult<HistoryPage> {
    history_service::list_history(&state.pool, &filter, today_local()).await
}

#[tauri::command]
pub async fn get_subject_history(
    state: State<'_, AppState>,
    stream: String,
    subject_id: i64,
    term: String,
) -> AppResult<Vec<HistoryEventEntry>> {
    history_service::subject_history(&state.pool, &stream, subject_id, &term).await
}
