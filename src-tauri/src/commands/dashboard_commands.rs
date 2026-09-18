use crate::db::{assignments, companies, settings, students, teachers, AppState};
use crate::domain::workload::{coordinator_capacity, statutory_cap, InstitutionType};
use crate::error::AppResult;
use serde::Serialize;
use tauri::State;

/// Genel Bakış ekranının sayaçları ve saat dengesi.
/// Tek çağrıda toplanır ki ekran ayrı ayrı istek atmasın.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    /// Sayıların ait olduğu eğitim-öğretim yılı.
    pub term: String,

    // Kayıt sayıları
    /// İşletmeler kalıcıdır; dönemden bağımsız toplam.
    pub company_count: i64,
    /// Yalnızca aktif dönemdeki öğrenciler.
    pub student_count: i64,
    pub teacher_count: i64,
    pub active_teacher_count: i64,

    // Saat dengesi — hepsi aktif dönem içindir
    /// Aktif öğretmenlerin koordinatörlük kapasiteleri toplamı (dağıtılabilir azami).
    pub total_capacity_hours: i64,
    /// Atamalarda takdir edilmiş toplam saat (dağıtılmış).
    pub assigned_hours: i64,
    /// Dağıtılabilir azamiden kalan. Aşım varsa negatif olur ve uyarı anlamına gelir.
    pub remaining_hours: i64,
    /// Kapasitesi dolmuş aktif öğretmen sayısı.
    pub teachers_at_capacity: i64,
    /// Kapasitesi aşılmış aktif öğretmen sayısı — mevzuat ihlalidir.
    pub teachers_over_capacity: i64,

    // Eksik kayıtlar
    pub companies_without_location: i64,
    pub students_without_company: i64,
    pub companies_without_students: i64,
    /// Aktif dönemde öğrencisi olup koordinatörü atanmamış işletmeler.
    pub companies_without_assignment: i64,
}

#[tauri::command]
pub async fn get_dashboard_stats(state: State<'_, AppState>) -> AppResult<DashboardStats> {
    let pool = &state.pool;
    let all = settings::get_all(pool).await?;
    let term = all.get("active_term").cloned().unwrap_or_default();

    let institution_type = InstitutionType::parse(
        all.get("institution_type").map(String::as_str).unwrap_or("other"),
    );
    let is_metropolitan = all
        .get("is_metropolitan_district")
        .map(|v| v == "true")
        .unwrap_or(false);
    let cap = statutory_cap(institution_type, is_metropolitan);

    let all_companies = companies::list(pool).await?;
    let all_students = students::list_by_term(pool, &term).await?;
    let all_teachers = teachers::list(pool).await?;
    let student_counts = students::count_by_company(pool, &term).await?;
    let awarded_by_teacher = assignments::awarded_hours_by_teacher(pool, &term).await?;
    let assigned_companies: std::collections::HashSet<i64> =
        assignments::assigned_company_ids(pool, &term).await?.into_iter().collect();

    let companies_with_students: std::collections::HashSet<i64> =
        student_counts.iter().map(|(id, _)| *id).collect();

    // Saat dengesi yalnızca aktif öğretmenler üzerinden hesaplanır;
    // pasif öğretmene atama yapılmaz.
    let mut total_capacity_hours = 0;
    let mut teachers_at_capacity = 0;
    let mut teachers_over_capacity = 0;

    for teacher in all_teachers.iter().filter(|t| t.is_active == 1) {
        let chief_hours = teachers::parse_chief_type(&teacher.chief_type).weekly_hours();
        let capacity = coordinator_capacity(
            teacher.max_extra_hours,
            chief_hours,
            teacher.other_extra_hours,
            cap,
        );
        total_capacity_hours += capacity;

        let awarded = awarded_by_teacher
            .iter()
            .find(|(id, _)| *id == teacher.id)
            .map(|(_, hours)| *hours)
            .unwrap_or(0);

        if awarded > capacity {
            teachers_over_capacity += 1;
        } else if awarded == capacity && capacity > 0 {
            teachers_at_capacity += 1;
        }
    }

    let assigned_hours = assignments::total_awarded_hours(pool, &term).await?;

    Ok(DashboardStats {
        term,
        company_count: all_companies.len() as i64,
        student_count: all_students.len() as i64,
        teacher_count: all_teachers.len() as i64,
        active_teacher_count: all_teachers.iter().filter(|t| t.is_active == 1).count() as i64,

        total_capacity_hours,
        assigned_hours,
        remaining_hours: total_capacity_hours - assigned_hours,
        teachers_at_capacity,
        teachers_over_capacity,

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
        // Öğrencisi olmayan işletme için koordinatör gerekmez; bu yüzden
        // yalnızca öğrencisi olanlar sayılır.
        companies_without_assignment: all_companies
            .iter()
            .filter(|c| companies_with_students.contains(&c.id) && !assigned_companies.contains(&c.id))
            .count() as i64,
    })
}
