use crate::db::{availability, class_days, settings, students, teachers, AppState};
use crate::domain::history::decide::{ChangeCommand, ChangeRequest};
use crate::domain::scheduling::Slot;
use crate::domain::terms::today_local;
use crate::error::{AppError, AppResult};
use crate::services::change_service::{execute_change, ChangeMode, ChangeOutcome};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use tauri::State;

/// Izgaradan kaydedilen program için değişiklik günlüğüne yazılan gerekçe.
/// Planlama evresinde tarih sorulmaz (spec §5.1); tarihçeli değişiklikler
/// `commit_change` ile kendi gerekçesini taşır.
const SAVE_SCHEDULE_REASON: &str = "Planlama evresinde kaydedildi";
const COPY_SCHEDULE_REASON: &str = "Önceki dönemden kopyalandı";

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

async fn load_board(pool: &SqlitePool) -> AppResult<AvailabilityBoard> {
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
    load_board(&state.pool).await
}

/// Izgaranın gönderdiği tek hücre.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotInput {
    pub day_of_week: i64,
    pub hour: i64,
}

/// Tek değişiklik kapısından (`execute_change`) yürürlük tarihi VERMEDEN
/// kaydeder: planlamada dönem başı olur (spec §5.1). Dönem başlamışsa karar
/// katmanı `EffectiveDateRequired` ile reddeder; ızgara o durumda tarihçe
/// komutlarını kullanır. Ret, kullanıcıya gerekçesiyle `Validation` olarak
/// döner — sessizce yutulmaz.
async fn commit_schedule_change(
    pool: &SqlitePool,
    term: &str,
    command: ChangeCommand,
    reason: &str,
    today: NaiveDate,
) -> AppResult<()> {
    let request = ChangeRequest {
        term: term.to_string(),
        effective_date: None,
        document_date: None,
        reason: reason.to_string(),
        command,
    };
    match execute_change(pool, request, ChangeMode::Commit { expected_high_water: None }, today).await? {
        ChangeOutcome::Committed { .. } => Ok(()),
        ChangeOutcome::Rejected { reason, .. } => Err(AppError::Validation(reason)),
        // `expected_high_water: None` bayat denetimi yapmaz; yine de gelirse
        // kullanıcıya olduğu gibi iletilir.
        ChangeOutcome::Stale { message } => Err(AppError::Validation(message)),
        ChangeOutcome::Preview { .. } => Err(AppError::Database(
            "Kayıt sırasında beklenmeyen önizleme sonucu döndü".into(),
        )),
    }
}

/// Günü/saati doğrulama `change_input::validate_command`'a aittir (aynı
/// kural iki yerde yaşamaz); komut yalnız olaya çevirir.
async fn save_availability(
    pool: &SqlitePool,
    teacher_id: i64,
    slots: &[SlotInput],
    today: NaiveDate,
) -> AppResult<AvailabilityBoard> {
    let term = settings::get_active_term(pool).await?;
    let command = ChangeCommand::SetTeacherSchedule {
        teacher_id,
        slots: slots.iter().map(|s| Slot::new(s.day_of_week, s.hour)).collect(),
    };
    commit_schedule_change(pool, &term, command, SAVE_SCHEDULE_REASON, today).await?;
    load_board(pool).await
}

/// Bir öğretmenin haftalık boş saatlerini tamamen değiştirir.
/// Izgara her kaydetmede tüm haftayı gönderir; kısmi güncelleme yoktur.
/// Kayıt olay günlüğüne yazılır; okuma projeksiyondan gelir.
#[tauri::command]
pub async fn save_teacher_availability(
    state: State<'_, AppState>,
    teacher_id: i64,
    slots: Vec<SlotInput>,
) -> AppResult<AvailabilityBoard> {
    save_availability(&state.pool, teacher_id, &slots, today_local()).await
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
    load_board(&state.pool).await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyOutcome {
    pub availability_copied: bool,
    pub class_days_copied: bool,
    pub board: AvailabilityBoard,
}

/// Kaynak dönemin programını olaylarla hedefe kopyalar. Hedefte program
/// varsa ya da kaynakta kopyalanacak program yoksa hiçbir şey yazılmaz ve
/// `false` döner: karar katmanı kopyayı mevcut programın ÜSTÜNE yazardı, ama
/// bu ekranın sözleşmesi "mevcut kayıt üzerine yazılmaz"dır.
async fn copy_schedules(pool: &SqlitePool, term: &str, from_term: &str, today: NaiveDate) -> AppResult<bool> {
    let target_has_schedule = !availability::list_all(pool, term).await?.is_empty();
    let source_is_empty = availability::list_all(pool, from_term).await?.is_empty();
    if target_has_schedule || source_is_empty {
        return Ok(false);
    }

    let command = ChangeCommand::CopySchedulesFromTerm { from_term: from_term.to_string() };
    commit_schedule_change(pool, term, command, COPY_SCHEDULE_REASON, today).await?;
    Ok(true)
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

    let availability_copied = copy_schedules(&state.pool, &term, &from_term, today_local()).await?;
    let class_days_copied = class_days::copy_term(&state.pool, &from_term, &term).await?;

    Ok(CopyOutcome {
        availability_copied,
        class_days_copied,
        board: load_board(&state.pool).await?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::teaching_load_test_support::{seed_teacher, TERM};
    use crate::db::{init_pool, terms};
    use crate::domain::models::ChiefType;
    use crate::domain::terms::TermDates;

    const SOURCE_TERM: &str = "2025-2026/1";

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// Dönem (2026-09-01) başlamadan önce: tarih sorulmaz.
    fn planning_today() -> NaiveDate {
        ymd(2026, 8, 15)
    }

    /// Dönem başladı: yürürlük tarihi zorunlu, eski yazıcı bunu veremez.
    fn november_today() -> NaiveDate {
        ymd(2026, 11, 10)
    }

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    fn slot(day_of_week: i64, hour: i64) -> SlotInput {
        SlotInput { day_of_week, hour }
    }

    fn free_slots_of(board: &AvailabilityBoard, teacher_id: i64) -> Vec<String> {
        board
            .teachers
            .iter()
            .find(|t| t.teacher_id == teacher_id)
            .map(|t| t.free_slots.clone())
            .unwrap_or_default()
    }

    async fn count(pool: &SqlitePool, table: &str, kind: Option<&str>) -> i64 {
        let sql = match kind {
            Some(_) => format!("SELECT COUNT(*) FROM {table} WHERE kind = ?1"),
            None => format!("SELECT COUNT(*) FROM {table}"),
        };
        let mut query = sqlx::query_scalar(&sql);
        if let Some(kind) = kind {
            query = query.bind(kind);
        }
        query.fetch_one(pool).await.unwrap()
    }

    /// Kaynak dönemi açar ve öğretmene o dönemde bir program yazar.
    async fn seed_source_schedule(pool: &SqlitePool, teacher_id: i64, slots: &[SlotInput]) {
        let mut conn = pool.acquire().await.unwrap();
        terms::insert_in(&mut conn, &TermDates::default_for(SOURCE_TERM)).await.unwrap();
        drop(conn);
        let command = ChangeCommand::SetTeacherSchedule {
            teacher_id,
            slots: slots.iter().map(|s| Slot::new(s.day_of_week, s.hour)).collect(),
        };
        commit_schedule_change(pool, SOURCE_TERM, command, "test", ymd(2025, 8, 1)).await.unwrap();
    }

    /// Asıl regresyon: kaydedilen program, ızgara yeniden yüklenince kaybolmamalı.
    #[tokio::test]
    async fn saved_schedule_stays_on_the_board_and_goes_through_the_change_log() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = seed_teacher(&pool, "Ada", ChiefType::None).await;

        let saved = save_availability(&pool, teacher_id, &[slot(1, 9), slot(3, 14)], planning_today())
            .await
            .unwrap();
        assert_eq!(free_slots_of(&saved, teacher_id), vec!["1-9", "3-14"]);

        let reloaded = load_board(&pool).await.unwrap();
        assert_eq!(free_slots_of(&reloaded, teacher_id), vec!["1-9", "3-14"]);
        assert_eq!(reloaded.teachers[0].free_count, 2);
        assert_eq!(count(&pool, "change_sets", Some("set_teacher_schedule")).await, 1, "olay yoluyla yazılmalı");
    }

    /// Atama motoru ve panoları `availability::list_all`'dan beslenir; tahta da
    /// aynı okuyucudan gelir, yani ikisi ayrışamaz.
    #[tokio::test]
    async fn board_and_the_shared_reader_agree() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = seed_teacher(&pool, "Ada", ChiefType::None).await;
        save_availability(&pool, teacher_id, &[slot(2, 10), slot(4, 11)], planning_today()).await.unwrap();

        let from_reader: Vec<String> = availability::list_all(&pool, TERM)
            .await
            .unwrap()
            .iter()
            .map(|s| format!("{}-{}", s.day_of_week, s.hour))
            .collect();

        let board = load_board(&pool).await.unwrap();
        assert_eq!(free_slots_of(&board, teacher_id), from_reader);
        assert_eq!(from_reader, vec!["2-10", "4-11"]);
    }

    /// İkinci kayıt öncekinin yerine geçer; birikmez. Boş liste de geçerlidir.
    #[tokio::test]
    async fn saving_again_replaces_the_previous_schedule() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = seed_teacher(&pool, "Ada", ChiefType::None).await;

        save_availability(&pool, teacher_id, &[slot(1, 9), slot(1, 10)], planning_today()).await.unwrap();
        let second = save_availability(&pool, teacher_id, &[slot(2, 11)], planning_today()).await.unwrap();
        assert_eq!(free_slots_of(&second, teacher_id), vec!["2-11"]);

        let cleared = save_availability(&pool, teacher_id, &[], planning_today()).await.unwrap();
        assert!(free_slots_of(&cleared, teacher_id).is_empty());
        assert_eq!(cleared.teachers[0].free_count, 0);
    }

    /// Dönem başladıysa eski komut yürürlük tarihi veremez: karar katmanı
    /// `EffectiveDateRequired` der, komut bunu `Validation` yapar ve HİÇBİR
    /// şey yazılmaz (ne günlük ne projeksiyon).
    #[tokio::test]
    async fn saving_after_the_term_started_is_a_validation_error_and_writes_nothing() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = seed_teacher(&pool, "Ada", ChiefType::None).await;
        let sets_before = count(&pool, "change_sets", None).await;
        let events_before = count(&pool, "change_events", None).await;

        let result = save_availability(&pool, teacher_id, &[slot(1, 9)], november_today()).await;

        assert!(matches!(result, Err(AppError::Validation(_))), "Validation beklenirdi: {result:?}");
        assert_eq!(count(&pool, "change_sets", None).await, sets_before);
        assert_eq!(count(&pool, "change_events", None).await, events_before);
        assert_eq!(count(&pool, "teacher_schedule_periods", None).await, 0);
    }

    /// Sınır doğrulaması: geçersiz gün olay yoluna girmeden reddedilir.
    #[tokio::test]
    async fn weekend_day_is_rejected_before_anything_is_written() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = seed_teacher(&pool, "Ada", ChiefType::None).await;
        let sets_before = count(&pool, "change_sets", None).await;

        let result = save_availability(&pool, teacher_id, &[slot(6, 9)], planning_today()).await;

        assert!(matches!(result, Err(AppError::Validation(_))));
        assert_eq!(count(&pool, "change_sets", None).await, sets_before);
    }

    #[tokio::test]
    async fn copying_a_term_writes_events_and_shows_on_the_board() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = seed_teacher(&pool, "Ada", ChiefType::None).await;
        seed_source_schedule(&pool, teacher_id, &[slot(1, 9), slot(5, 15)]).await;

        let copied = copy_schedules(&pool, TERM, SOURCE_TERM, planning_today()).await.unwrap();

        assert!(copied);
        assert_eq!(count(&pool, "change_sets", Some("copy_schedules_from_term")).await, 1);
        let board = load_board(&pool).await.unwrap();
        assert_eq!(free_slots_of(&board, teacher_id), vec!["1-9", "5-15"]);
    }

    /// Hedefte program varsa kopya üzerine yazmaz ve hiçbir şey yazmaz.
    #[tokio::test]
    async fn copying_refuses_when_the_target_already_has_a_schedule() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = seed_teacher(&pool, "Ada", ChiefType::None).await;
        seed_source_schedule(&pool, teacher_id, &[slot(1, 9)]).await;
        save_availability(&pool, teacher_id, &[slot(5, 15)], planning_today()).await.unwrap();
        let sets_before = count(&pool, "change_sets", None).await;

        let copied = copy_schedules(&pool, TERM, SOURCE_TERM, planning_today()).await.unwrap();

        assert!(!copied);
        assert_eq!(count(&pool, "change_sets", None).await, sets_before);
        let board = load_board(&pool).await.unwrap();
        assert_eq!(free_slots_of(&board, teacher_id), vec!["5-15"], "mevcut kayıt korunmalı");
    }

    /// Kaynakta program yoksa kopyalanacak bir şey yoktur: hata değil, `false`.
    #[tokio::test]
    async fn copying_from_a_term_without_schedules_is_not_an_error() {
        let (_dir, pool) = test_pool().await;
        seed_teacher(&pool, "Ada", ChiefType::None).await;

        let copied = copy_schedules(&pool, TERM, SOURCE_TERM, planning_today()).await.unwrap();

        assert!(!copied);
        assert_eq!(count(&pool, "change_sets", Some("copy_schedules_from_term")).await, 0);
    }

    /// Dönem başladıysa kopya da yürürlük tarihi ister; ret `Validation` olur.
    #[tokio::test]
    async fn copying_after_the_term_started_is_a_validation_error() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = seed_teacher(&pool, "Ada", ChiefType::None).await;
        seed_source_schedule(&pool, teacher_id, &[slot(1, 9)]).await;

        let result = copy_schedules(&pool, TERM, SOURCE_TERM, november_today()).await;

        assert!(matches!(result, Err(AppError::Validation(_))), "Validation beklenirdi: {result:?}");
        assert_eq!(count(&pool, "change_sets", Some("copy_schedules_from_term")).await, 0);
    }

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
