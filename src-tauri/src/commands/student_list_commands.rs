use tauri::State;

use crate::db::AppState;
use crate::domain::terms::today_local;
use crate::error::AppResult;
use crate::services::student_list_apply::{self, StudentListFile, StudentListPreview, StudentListSummary};

// e-Okul .XLS dosyaları frontend'de okunup ham bayt olarak gönderilir
// (JotForm CSV'nin metin sürümünün baytlara karşılığı) — böylece burada da
// ayrı bir dosya sistemi izni gerekmez.

/// Dosyaları ayrıştırıp veritabanıyla karşılaştırır. HİÇBİR ŞEY YAZMAZ.
#[tauri::command]
pub async fn preview_student_list_import(
    state: State<'_, AppState>,
    files: Vec<StudentListFile>,
) -> AppResult<StudentListPreview> {
    student_list_apply::preview(&state.pool, &files).await
}

/// Önizlemede onaylanan içe aktarmayı uygular. `today`, komut sınırında BİR
/// kez hesaplanır (`history_commands.rs`'teki desenin aynısı) ve hizmet
/// katmanına parametre olarak verilir.
#[tauri::command]
pub async fn apply_student_list_import(
    state: State<'_, AppState>,
    files: Vec<StudentListFile>,
    effective_date: Option<chrono::NaiveDate>,
    reason: String,
) -> AppResult<StudentListSummary> {
    student_list_apply::apply(&state.pool, &files, effective_date, &reason, today_local()).await
}
