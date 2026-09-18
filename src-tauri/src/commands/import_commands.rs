use crate::db::AppState;
use crate::error::AppResult;
use crate::services::import_apply::{self, DuplicatePolicy, ImportPreview, ImportSummary};
use std::collections::BTreeMap;
use tauri::State;

// CSV dosyası frontend tarafında okunup metin olarak gönderilir; böylece
// ayrı bir dosya sistemi eklentisine ve izin tanımına gerek kalmaz.

#[tauri::command]
pub async fn preview_csv_import(
    state: State<'_, AppState>,
    content: String,
) -> AppResult<ImportPreview> {
    import_apply::preview(&state.pool, &content).await
}

#[tauri::command]
pub async fn apply_csv_import(
    state: State<'_, AppState>,
    content: String,
    policies: BTreeMap<String, DuplicatePolicy>,
) -> AppResult<ImportSummary> {
    import_apply::apply(&state.pool, &content, &policies).await
}
