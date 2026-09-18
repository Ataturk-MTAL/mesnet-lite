use crate::db::{availability, class_days, settings, students, teachers, AppState};
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tauri::State;

/// Müsaitlik ekranındaki bir öğretmen.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailabilityTeacher {
    pub teacher_id: i64,
    pub teacher_name: String,
    /// Boş saatler: `{gün}-{saat}` anahtarları.
    pub free_slots: Vec<String>,
    pub free_count: i64,
}

/// Bir sınıfın işletme günleri.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassDays {
    pub grade: String,
    pub days: Vec<i64>,
    /// O sınıftaki öğrenci sayısı — hangi sınıfların gerçekten var olduğunu gösterir.
    pub student_count: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AvailabilityBoard {
    pub term: String,
    pub teachers: Vec<AvailabilityTeacher>,
    pub classes: Vec<ClassDays>,
    pub day_start_hour: i64,
    pub day_end_hour: i64,
    /// Müsaitlik kopyalanabilecek diğer dönemler.
    pub other_terms: Vec<String>,
    pub warnings: Vec<String>,
}

fn parse_hour_setting(all: &BTreeMap<String, String>, key: &str, fallback: i64) -> i64 {
    all.get(key)
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(fallback)
}

async fn load_board(state: &AppState) -> AppResult<AvailabilityBoard> {
    let pool = &state.pool;
    let all_settings = settings::get_all(pool).await?;
    let term = all_settings.get("active_term").cloned().unwrap_or_default();

    let mut board = AvailabilityBoard {
        term: term.clone(),
        day_start_hour: parse_hour_setting(&all_settings, "day_start_hour", 8),
        day_end_hour: parse_hour_setting(&all_settings, "day_end_hour", 17),
        ..Default::default()
    };

    // --- Öğretmenler ve boş saatleri ---
    let all_slots = availability::list_all(pool, &term).await?;
    for teacher in teachers::list_active(pool).await? {
        let free_slots: Vec<String> = all_slots
            .iter()
            .filter(|slot| slot.teacher_id == teacher.id)
            .map(|slot| format!("{}-{}", slot.day_of_week, slot.hour))
            .collect();

        board.teachers.push(AvailabilityTeacher {
            teacher_id: teacher.id,
            teacher_name: format!("{} {}", teacher.first_name, teacher.last_name),
            free_count: free_slots.len() as i64,
            free_slots,
        });
    }
    board
        .teachers
        .sort_by(|a, b| a.teacher_name.cmp(&b.teacher_name));

    // --- Sınıflar ve işletme günleri ---
    // Sınıf listesi öğrenci kayıtlarından türetilir: elle sınıf tanımlamak
    // yerine gerçekte öğrencisi olan sınıflar gösterilir.
    let mut student_counts: BTreeMap<String, i64> = BTreeMap::new();
    for student in students::list_by_term(pool, &term).await? {
        *student_counts.entry(student.grade).or_insert(0) += 1;
    }

    let day_map = class_days::map_by_grade(pool, &term).await?;

    // Kaydı olan ama artık öğrencisi kalmayan sınıflar da görünmeli.
    let mut grades: Vec<String> = student_counts.keys().cloned().collect();
    for grade in day_map.keys() {
        if !grades.contains(grade) {
            grades.push(grade.clone());
        }
    }
    grades.sort();

    for grade in grades {
        board.classes.push(ClassDays {
            days: day_map
                .get(&grade)
                .map(|days| days.iter().copied().collect())
                .unwrap_or_default(),
            student_count: student_counts.get(&grade).copied().unwrap_or(0),
            grade,
        });
    }

    board.other_terms = students::list_terms(pool)
        .await?
        .into_iter()
        .filter(|other| other != &term)
        .collect();

    // --- Uyarılar ---
    if board.teachers.is_empty() {
        board
            .warnings
            .push("Aktif öğretmen yok. Önce Öğretmenler ekranından öğretmen ekleyin.".into());
    }
    let without_slots = board.teachers.iter().filter(|t| t.free_count == 0).count();
    if without_slots > 0 {
        board.warnings.push(format!(
            "{without_slots} öğretmenin boş saati girilmemiş; bu öğretmenlere işletme atanamaz."
        ));
    }
    if board.classes.is_empty() {
        board
            .warnings
            .push("Bu dönemde öğrenci kaydı yok. Önce CSV içe aktarın.".into());
    }
    let without_days = board.classes.iter().filter(|c| c.days.is_empty()).count();
    if without_days > 0 {
        board.warnings.push(format!(
            "{without_days} sınıfın işletme günü tanımlanmamış; o sınıfın öğrencilerinin \
             bulunduğu işletmeler dağıtıma giremez."
        ));
    }

    Ok(board)
}

#[tauri::command]
pub async fn get_availability_board(state: State<'_, AppState>) -> AppResult<AvailabilityBoard> {
    load_board(&state).await
}

/// Izgaranın gönderdiği tek hücre.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotInput {
    pub day_of_week: i64,
    pub hour: i64,
}

/// Bir öğretmenin haftalık boş saatlerini tamamen değiştirir.
/// Izgara her kaydetmede tüm haftayı gönderir; kısmi güncelleme yoktur.
#[tauri::command]
pub async fn save_teacher_availability(
    state: State<'_, AppState>,
    teacher_id: i64,
    slots: Vec<SlotInput>,
) -> AppResult<AvailabilityBoard> {
    for slot in &slots {
        if !(1..=5).contains(&slot.day_of_week) {
            return Err(AppError::Validation(
                "Gün Pazartesi ile Cuma arasında olmalı".into(),
            ));
        }
    }

    let term = settings::get_active_term(&state.pool).await?;
    let pairs: Vec<(i64, i64)> = slots.iter().map(|s| (s.day_of_week, s.hour)).collect();
    availability::replace_for_teacher(&state.pool, teacher_id, &term, &pairs).await?;
    load_board(&state).await
}

#[tauri::command]
pub async fn save_class_days(
    state: State<'_, AppState>,
    grade: String,
    days: Vec<i64>,
) -> AppResult<AvailabilityBoard> {
    if grade.trim().is_empty() {
        return Err(AppError::Validation("Sınıf boş olamaz".into()));
    }
    for day in &days {
        if !(1..=5).contains(day) {
            return Err(AppError::Validation(
                "Gün Pazartesi ile Cuma arasında olmalı".into(),
            ));
        }
    }

    let term = settings::get_active_term(&state.pool).await?;
    class_days::replace_for_grade(&state.pool, grade.trim(), &term, &days).await?;
    load_board(&state).await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyOutcome {
    pub availability_copied: bool,
    pub class_days_copied: bool,
    pub board: AvailabilityBoard,
}

/// Başka bir dönemin haftalık programını bu döneme kopyalar.
/// Hedefte veri varsa o tür kopyalanmaz — mevcut kayıt üzerine yazılmaz.
#[tauri::command]
pub async fn copy_schedule_from_term(
    state: State<'_, AppState>,
    from_term: String,
) -> AppResult<CopyOutcome> {
    let term = settings::get_active_term(&state.pool).await?;
    if from_term == term {
        return Err(AppError::Validation(
            "Kaynak ve hedef dönem aynı olamaz".into(),
        ));
    }

    let availability_copied = availability::copy_term(&state.pool, &from_term, &term).await?;
    let class_days_copied = class_days::copy_term(&state.pool, &from_term, &term).await?;

    Ok(CopyOutcome {
        availability_copied,
        class_days_copied,
        board: load_board(&state).await?,
    })
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
    }

    #[test]
    fn missing_settings_use_the_fallback() {
        assert_eq!(parse_hour_setting(&BTreeMap::new(), "day_start_hour", 8), 8);
    }
}
