use crate::db::{companies, students, teachers, AppState};
use crate::error::AppResult;
use serde::Serialize;
use tauri::State;

/// Genel Bakış ekranının sayaçları. Tek çağrıda toplanır ki ekran
/// dört ayrı istek atmasın.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub company_count: i64,
    pub student_count: i64,
    pub teacher_count: i64,
    pub active_teacher_count: i64,
    /// Enlem/boylamı olmayan işletmeler; haritada gösterilemezler.
    pub companies_without_location: i64,
    /// Hiçbir işletmeye bağlı olmayan öğrenciler.
    pub students_without_company: i64,
    /// Öğrencisi olmayan işletmeler.
    pub companies_without_students: i64,
}

#[tauri::command]
pub async fn get_dashboard_stats(state: State<'_, AppState>) -> AppResult<DashboardStats> {
    let all_companies = companies::list(&state.pool).await?;
    let all_students = students::list(&state.pool).await?;
    let all_teachers = teachers::list(&state.pool).await?;
    let counts = students::count_by_company(&state.pool).await?;

    let companies_with_students: std::collections::HashSet<i64> =
        counts.iter().map(|(id, _)| *id).collect();

    Ok(DashboardStats {
        company_count: all_companies.len() as i64,
        student_count: all_students.len() as i64,
        teacher_count: all_teachers.len() as i64,
        active_teacher_count: all_teachers.iter().filter(|t| t.is_active == 1).count() as i64,
        companies_without_location: all_companies
            .iter()
            .filter(|c| c.latitude.is_none() || c.longitude.is_none())
            .count() as i64,
        students_without_company: all_students
            .iter()
            .filter(|s| s.company_id.is_none())
            .count() as i64,
        companies_without_students: all_companies
            .iter()
            .filter(|c| !companies_with_students.contains(&c.id))
            .count() as i64,
    })
}
