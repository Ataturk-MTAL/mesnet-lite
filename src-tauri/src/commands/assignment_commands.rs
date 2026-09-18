use crate::db::assignments::NewAssignment;
use crate::db::{
    assignments, availability, class_days, companies, company_hours, settings, students, teachers,
    AppState,
};
use crate::domain::scheduling::MAX_HOURS_PER_DAY;
use crate::domain::workload::{coordinator_capacity, statutory_cap, InstitutionType};
use crate::error::AppResult;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use tauri::State;

/// Atama ekranındaki bir işletme kartı.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardCompany {
    pub company_id: i64,
    pub company_name: String,
    pub address_text: String,
    pub one_way_distance_km: Option<f64>,
    pub student_count: i64,
    pub student_names: Vec<String>,
    /// İşletmedeki öğrencilerin dalları.
    pub branches: Vec<String>,
    /// Takdir edilen haftalık saat. Fahri ziyarette 0.
    pub awarded_hours: i64,
    pub is_honorary: bool,
    /// Takdir hiç girilmemişse true — atanabilir ama saat taşımaz.
    pub hours_missing: bool,
    /// Öğrencilerin sınıflarının işletmede bulunduğu günler (1–5).
    pub workplace_days: Vec<i64>,
    /// Atanmışsa yerleşim bilgisi.
    pub assigned_teacher_id: Option<i64>,
    pub visit_day: Option<i64>,
    pub visit_hour: Option<i64>,
    pub is_forced: bool,
    pub force_reason: Option<String>,
}

/// Atama ekranındaki bir öğretmen ve haftalık ızgarası.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardTeacher {
    pub teacher_id: i64,
    pub teacher_name: String,
    pub branches: Vec<String>,
    /// Koordinatörlük kapasitesi (MADDE 15/2 tavanı eksi şeflik ve diğer ek dersler).
    pub capacity: i64,
    /// Atanmış işletmelerin takdir toplamı.
    pub assigned_hours: i64,
    pub company_count: i64,
    /// Boş saatler: `{gün}-{saat}` anahtarları.
    pub free_slots: Vec<String>,
    /// Gün başına toplam saat — günlük 8 saat sınırının denetimi için.
    pub hours_per_day: BTreeMap<i64, i64>,
    pub days_over_cap: Vec<i64>,
    pub is_over_capacity: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AssignmentBoard {
    pub term: String,
    pub teachers: Vec<BoardTeacher>,
    pub companies: Vec<BoardCompany>,
    /// Izgaranın saat aralığı (ayarlardan).
    pub day_start_hour: i64,
    pub day_end_hour: i64,
    // Üst sayaçlar — MESNET'teki dört kart
    pub pool_hours: i64,
    pub assigned_hours: i64,
    pub remaining_hours: i64,
    pub assigned_company_count: i64,
    pub total_company_count: i64,
    pub honorary_count: i64,
    pub warnings: Vec<String>,
}

fn parse_hour_setting(all: &BTreeMap<String, String>, key: &str, fallback: i64) -> i64 {
    all.get(key)
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(fallback)
}

fn pool_from_settings(all: &BTreeMap<String, String>) -> i64 {
    let hours: BTreeMap<String, i64> = serde_json::from_str(
        all.get("branch_weekly_hours")
            .map(String::as_str)
            .unwrap_or("{}"),
    )
    .unwrap_or_default();
    let groups: BTreeMap<String, i64> = serde_json::from_str(
        all.get("branch_group_counts")
            .map(String::as_str)
            .unwrap_or("{}"),
    )
    .unwrap_or_default();

    hours
        .iter()
        .filter_map(|(key, weekly)| groups.get(key).map(|count| weekly * count))
        .sum()
}

async fn load_board(state: &AppState) -> AppResult<AssignmentBoard> {
    let pool = &state.pool;
    let all_settings = settings::get_all(pool).await?;
    let term = all_settings.get("active_term").cloned().unwrap_or_default();

    let institution_type = InstitutionType::parse(
        all_settings
            .get("institution_type")
            .map(String::as_str)
            .unwrap_or("other"),
    );
    let is_metropolitan = all_settings
        .get("is_metropolitan_district")
        .map(|v| v == "true")
        .unwrap_or(false);
    let cap = statutory_cap(institution_type, is_metropolitan);

    let mut board = AssignmentBoard {
        term: term.clone(),
        day_start_hour: parse_hour_setting(&all_settings, "day_start_hour", 8),
        day_end_hour: parse_hour_setting(&all_settings, "day_end_hour", 17),
        pool_hours: pool_from_settings(&all_settings),
        ..Default::default()
    };

    // --- İşletmeler ---
    let all_companies = companies::list(pool).await?;
    let all_students = students::list_by_term(pool, &term).await?;
    let class_day_map = class_days::map_by_grade(pool, &term).await?;
    let hours_map: BTreeMap<i64, _> = company_hours::list(pool, &term)
        .await?
        .into_iter()
        .map(|row| (row.company_id, row))
        .collect();
    let assignment_map: BTreeMap<i64, _> = assignments::list(pool, &term)
        .await?
        .into_iter()
        .map(|row| (row.company_id, row))
        .collect();

    for company in &all_companies {
        let company_students: Vec<_> = all_students
            .iter()
            .filter(|s| s.company_id == Some(company.id))
            .collect();

        // İşletmenin ziyaret edilebileceği günler: öğrencilerin sınıflarının
        // işletme günlerinin BİRLEŞİMİ.
        let mut workplace_days: BTreeSet<i64> = BTreeSet::new();
        for student in &company_students {
            if let Some(days) = class_day_map.get(&student.grade) {
                workplace_days.extend(days.iter().copied());
            }
        }

        let branches: BTreeSet<String> =
            company_students.iter().map(|s| s.branch.clone()).collect();

        let hours = hours_map.get(&company.id);
        let assignment = assignment_map.get(&company.id);

        board.companies.push(BoardCompany {
            company_id: company.id,
            company_name: company.name.clone(),
            address_text: company.address_text.clone(),
            one_way_distance_km: company.one_way_distance_km,
            student_count: company_students.len() as i64,
            student_names: company_students
                .iter()
                .map(|s| format!("{} {}", s.first_name, s.last_name))
                .collect(),
            branches: branches.into_iter().collect(),
            awarded_hours: hours.map(|h| h.awarded_hours).unwrap_or(0),
            is_honorary: hours.map(|h| h.is_honorary == 1).unwrap_or(false),
            hours_missing: hours.is_none(),
            workplace_days: workplace_days.into_iter().collect(),
            assigned_teacher_id: assignment.map(|a| a.teacher_id),
            visit_day: assignment.map(|a| a.visit_day),
            visit_hour: assignment.map(|a| a.visit_hour),
            is_forced: assignment.map(|a| a.is_forced == 1).unwrap_or(false),
            force_reason: assignment.and_then(|a| a.force_reason.clone()),
        });
    }

    board
        .companies
        .sort_by(|a, b| a.company_name.cmp(&b.company_name));

    // --- Öğretmenler ---
    let all_teachers = teachers::list_active(pool).await?;
    let free_slots = availability::list_all(pool, &term).await?;
    let hours_by_teacher: BTreeMap<i64, i64> = assignments::awarded_hours_by_teacher(pool, &term)
        .await?
        .into_iter()
        .collect();

    let mut per_day: BTreeMap<(i64, i64), i64> = BTreeMap::new();
    for (teacher_id, day, hours) in
        assignments::awarded_hours_by_teacher_and_day(pool, &term).await?
    {
        per_day.insert((teacher_id, day), hours);
    }

    for teacher in all_teachers {
        let chief_hours = teachers::parse_chief_type(&teacher.chief_type).weekly_hours();
        let capacity = coordinator_capacity(
            teacher.max_extra_hours,
            chief_hours,
            teacher.other_extra_hours,
            cap,
        );
        let assigned_hours = hours_by_teacher.get(&teacher.id).copied().unwrap_or(0);

        let hours_per_day: BTreeMap<i64, i64> = (1..=5)
            .filter_map(|day| per_day.get(&(teacher.id, day)).map(|hours| (day, *hours)))
            .collect();

        // OÖKY MADDE 88: aynı gün 8 saatten fazla ek ders verilmez.
        let days_over_cap: Vec<i64> = hours_per_day
            .iter()
            .filter(|(_, hours)| **hours > MAX_HOURS_PER_DAY)
            .map(|(day, _)| *day)
            .collect();

        board.teachers.push(BoardTeacher {
            teacher_id: teacher.id,
            teacher_name: format!("{} {}", teacher.first_name, teacher.last_name),
            branches: teachers::decode_branches(&teacher.branches),
            capacity,
            assigned_hours,
            company_count: board
                .companies
                .iter()
                .filter(|c| c.assigned_teacher_id == Some(teacher.id))
                .count() as i64,
            free_slots: free_slots
                .iter()
                .filter(|slot| slot.teacher_id == teacher.id)
                .map(|slot| format!("{}-{}", slot.day_of_week, slot.hour))
                .collect(),
            hours_per_day,
            days_over_cap,
            is_over_capacity: assigned_hours > capacity,
        });
    }

    board
        .teachers
        .sort_by(|a, b| a.teacher_name.cmp(&b.teacher_name));

    // --- Sayaçlar ---
    board.assigned_hours = assignments::total_assigned_hours(pool, &term).await?;
    board.remaining_hours = board.pool_hours - board.assigned_hours;
    board.assigned_company_count = board
        .companies
        .iter()
        .filter(|c| c.assigned_teacher_id.is_some())
        .count() as i64;
    board.total_company_count = board.companies.len() as i64;
    board.honorary_count = board.companies.iter().filter(|c| c.is_honorary).count() as i64;

    if board.teachers.is_empty() {
        board.warnings.push(
            "Aktif öğretmen yok. Dağıtım yapabilmek için Öğretmenler ekranından öğretmen ekleyin."
                .into(),
        );
    }
    if !board.teachers.is_empty() && board.teachers.iter().all(|t| t.free_slots.is_empty()) {
        board.warnings.push(
            "Hiçbir öğretmenin boş saati girilmemiş. Müsaitlik Takvimi ekranından haftalık \
             programı girin."
                .into(),
        );
    }
    if class_day_map.is_empty() {
        board.warnings.push(
            "Sınıfların işletme günleri tanımlanmamış. Müsaitlik Takvimi ekranından girin.".into(),
        );
    }
    let missing_hours = board.companies.iter().filter(|c| c.hours_missing).count();
    if missing_hours > 0 {
        board.warnings.push(format!(
            "{missing_hours} işletmenin saat takdiri girilmemiş. Dağıtımdan ÖNCE \
             İşletme Saat Ayarları ekranından takdir edin."
        ));
    }
    for teacher in board.teachers.iter().filter(|t| t.is_over_capacity) {
        board.warnings.push(format!(
            "{}: atanan {} saat, kapasitesi {} saati aşıyor.",
            teacher.teacher_name, teacher.assigned_hours, teacher.capacity
        ));
    }
    for teacher in board.teachers.iter().filter(|t| !t.days_over_cap.is_empty()) {
        board.warnings.push(format!(
            "{}: {} numaralı günde günlük {MAX_HOURS_PER_DAY} saat sınırı aşıldı (OÖKY MADDE 88).",
            teacher.teacher_name,
            teacher
                .days_over_cap
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    Ok(board)
}

#[tauri::command]
pub async fn get_assignment_board(state: State<'_, AppState>) -> AppResult<AssignmentBoard> {
    load_board(&state).await
}

#[tauri::command]
pub async fn assign_company(
    state: State<'_, AppState>,
    input: NewAssignment,
) -> AppResult<AssignmentBoard> {
    let term = settings::get_active_term(&state.pool).await?;
    assignments::assign(&state.pool, &term, &input).await?;
    load_board(&state).await
}

#[tauri::command]
pub async fn unassign_company(
    state: State<'_, AppState>,
    company_id: i64,
) -> AppResult<AssignmentBoard> {
    let term = settings::get_active_term(&state.pool).await?;
    assignments::unassign(&state.pool, company_id, &term).await?;
    load_board(&state).await
}

/// Dönemdeki tüm atamaları siler. Geri alınamaz.
#[tauri::command]
pub async fn clear_assignments(state: State<'_, AppState>) -> AppResult<AssignmentBoard> {
    let term = settings::get_active_term(&state.pool).await?;
    assignments::clear_term(&state.pool, &term).await?;
    load_board(&state).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn hour_settings_fall_back_when_missing_or_invalid() {
        let all = settings_map(&[("day_start_hour", "9"), ("day_end_hour", "bozuk")]);

        assert_eq!(parse_hour_setting(&all, "day_start_hour", 8), 9);
        assert_eq!(parse_hour_setting(&all, "day_end_hour", 17), 17);
        assert_eq!(parse_hour_setting(&all, "yok", 5), 5);
    }

    #[test]
    fn pool_is_computed_from_the_two_settings_maps() {
        let all = settings_map(&[
            ("branch_weekly_hours", r#"{"12/C|Dal": 24}"#),
            ("branch_group_counts", r#"{"12/C|Dal": 2}"#),
        ]);

        assert_eq!(pool_from_settings(&all), 48);
    }

    #[test]
    fn pool_is_zero_when_settings_are_absent() {
        assert_eq!(pool_from_settings(&BTreeMap::new()), 0);
    }
}
