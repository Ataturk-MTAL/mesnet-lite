use crate::db::assignments::NewAssignment;
use crate::db::{
    assignments, availability, class_days, companies, company_hours, settings, students, teachers,
    teaching_load, AppState,
};
use crate::domain::allocation::{
    propose, AllocationProposal, CompanyInput, TeacherInput,
};
use crate::domain::scheduling::{visit_span, Slot, MAX_HOURS_PER_DAY};
use crate::domain::validation::{check_pool, check_teacher_totals, Violation};
use crate::domain::workload::{statutory_cap, teacher_capacity, InstitutionType};
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
    /// Atanmamış işletmelerin ilçe bazlı gruplanması için (bkz.
    /// `domain::address::parse_district`); adresten türetilemezse boştur.
    pub district: String,
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
    /// Bloğun BİTTİĞİ saat, DAHİL. `visit_hour` ile `visit_hour + span - 1`
    /// arasıdır; fahri ziyarette (`awarded_hours = 0`) `visit_hour` ile aynıdır.
    pub visit_end_hour: Option<i64>,
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
    /// Hücre (`{gün}-{saat}`) → o hücreyi kaplayan işletme id'si. Ardışık
    /// blok hücrelerinin TAMAMINI kapsar; ızgara bu hücreleri kapalı gösterip
    /// yeni bir bırakmayı reddetmelidir.
    pub occupied_by: BTreeMap<String, i64>,
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

/// Bir öğretmenin bloklarla dolu hücrelerini işletme id'sine eşler.
///
/// Girdi: `(öğretmen id, gün, başlangıç saati, bitiş saati DAHİL, işletme id)`
/// demetleri. Her blok, kapladığı TÜM ardışık hücrelere açılır.
fn occupied_cells_by_teacher(
    placements: &[(i64, i64, i64, i64, i64)],
) -> BTreeMap<i64, BTreeMap<String, i64>> {
    let mut result: BTreeMap<i64, BTreeMap<String, i64>> = BTreeMap::new();
    for &(teacher_id, day, start_hour, end_hour, company_id) in placements {
        let cells = result.entry(teacher_id).or_default();
        for hour in start_hour..=end_hour {
            cells.insert(format!("{day}-{hour}"), company_id);
        }
    }
    result
}

/// Bir `Violation`'ı panonun düz Türkçe uyarı metnine çevirir. Mevzuat
/// dayanağı varsa mevcut uyarı üslubuna uyarak parantez içinde eklenir
/// (ör. "... (OÖKY MADDE 88)."). Bu, `Violation` -> `String` dönüşümünün
/// yapıldığı TEK yer olmalı.
fn violation_to_warning(violation: &Violation) -> String {
    match violation.legal_basis {
        Some(basis) => format!("{} ({basis}).", violation.message),
        None => format!("{}.", violation.message),
    }
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

    // Havuz `settings` ayarlarından değil, döneme bağlı `term_branch_hours`
    // (bkz. migration 0005) ile şeflik projeksiyonundan hesaplanır; şeflik
    // saatleri dahil TAM havuzdur (OÖKY MADDE 88/2-ç).
    let as_of = teaching_load::current_as_of(pool, &term).await?;
    let pool_hours = teaching_load::total_pool_hours(pool, &term, as_of).await?;
    let (day_start_hour, day_end_hour) = settings::lesson_hour_bounds(&all_settings);

    let mut board = AssignmentBoard {
        term: term.clone(),
        day_start_hour,
        day_end_hour,
        pool_hours,
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
        let awarded_hours = hours.map(|h| h.awarded_hours).unwrap_or(0);

        board.companies.push(BoardCompany {
            company_id: company.id,
            company_name: company.name.clone(),
            address_text: company.address_text.clone(),
            district: company.district.clone(),
            one_way_distance_km: company.one_way_distance_km,
            student_count: company_students.len() as i64,
            student_names: company_students
                .iter()
                .map(|s| format!("{} {}", s.first_name, s.last_name))
                .collect(),
            branches: branches.into_iter().collect(),
            awarded_hours,
            is_honorary: hours.map(|h| h.is_honorary == 1).unwrap_or(false),
            hours_missing: hours.is_none(),
            workplace_days: workplace_days.into_iter().collect(),
            assigned_teacher_id: assignment.map(|a| a.teacher_id),
            visit_day: assignment.map(|a| a.visit_day),
            visit_hour: assignment.map(|a| a.visit_hour),
            visit_end_hour: assignment.map(|a| a.visit_hour + visit_span(awarded_hours) - 1),
            is_forced: assignment.map(|a| a.is_forced == 1).unwrap_or(false),
            force_reason: assignment.and_then(|a| a.force_reason.clone()),
        });
    }

    board
        .companies
        .sort_by(|a, b| a.company_name.cmp(&b.company_name));

    // --- Öğretmenler ---
    // Kapasite `teacher_load_periods` PROJEKSİYONUNDAN okunur (spec R5c);
    // eski `teachers.chief_type`/yük sütunları artık burada okunmaz, çünkü
    // yük değişiklikleri artık yalnız kapıdan (`SetTeacherLoad`) geçer ve o
    // sütunlara dokunmaz.
    let all_teachers: Vec<_> = teachers::list_with_load_as_of(pool, &term, as_of)
        .await?
        .into_iter()
        .filter(|entry| entry.teacher.is_active == 1)
        .collect();
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

    // Bloklarla dolu hücreler: ızgara bunları kapalı göstermeli ve yeni bir
    // bırakmayı bu hücrelere reddetmelidir.
    let placements: Vec<(i64, i64, i64, i64, i64)> = board
        .companies
        .iter()
        .filter_map(|c| {
            match (c.assigned_teacher_id, c.visit_day, c.visit_hour, c.visit_end_hour) {
                (Some(teacher_id), Some(day), Some(start), Some(end)) => {
                    Some((teacher_id, day, start, end, c.company_id))
                }
                _ => None,
            }
        })
        .collect();
    let occupied_by_teacher = occupied_cells_by_teacher(&placements);

    for entry in all_teachers {
        let teacher = entry.teacher;
        let capacity = teacher_capacity(&entry.load, cap);
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
            occupied_by: occupied_by_teacher.get(&teacher.id).cloned().unwrap_or_default(),
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

    // MADDE 15/2: takdir edilen toplam saat, dağıtılabilir okul havuzunu aşarsa
    // kullanıcı UYARILMALI (arayüz sayıyı kırmızıya boyaması yetmez).
    let total_awarded_hours: i64 = board.companies.iter().map(|c| c.awarded_hours).sum();
    if let Some(violation) = check_pool(total_awarded_hours, board.pool_hours) {
        board.warnings.push(violation_to_warning(&violation));
    }

    // Öğretmen bazlı kapasite ve günlük sınır denetimi TEK doğru kaynaktan
    // (`check_teacher_totals`) yapılır; kural burada ikinci kez yazılmaz.
    for teacher in &board.teachers {
        let violations = check_teacher_totals(
            teacher.teacher_id,
            &teacher.teacher_name,
            teacher.capacity,
            teacher.assigned_hours,
            &teacher.hours_per_day,
            false,
        );
        board
            .warnings
            .extend(violations.iter().map(violation_to_warning));
    }

    Ok(board)
}

#[tauri::command]
pub async fn get_assignment_board(state: State<'_, AppState>) -> AppResult<AssignmentBoard> {
    load_board(&state).await
}

/// Atanmamış işletmeler için yerleşim önerisi üretir. Hiçbir şey kaydedilmez;
/// kullanıcı öneriyi görüp uygulamaya karar verir.
#[tauri::command]
pub async fn propose_assignments(state: State<'_, AppState>) -> AppResult<AllocationProposal> {
    let board = load_board(&state).await?;

    let companies: Vec<CompanyInput> = board
        .companies
        .iter()
        // Zaten atanmış işletmeler öneriye girmez; mevcut karar korunur.
        .filter(|c| c.assigned_teacher_id.is_none())
        .map(|c| CompanyInput {
            id: c.company_id,
            name: c.company_name.clone(),
            branches: c.branches.clone(),
            student_count: c.student_count,
            awarded_hours: c.awarded_hours,
            is_honorary: c.is_honorary,
            latitude: None,
            longitude: None,
            workplace_days: c.workplace_days.iter().copied().collect(),
            one_way_distance_km: c.one_way_distance_km,
        })
        .collect();

    let teachers: Vec<TeacherInput> = board
        .teachers
        .iter()
        .map(|t| TeacherInput {
            id: t.teacher_id,
            name: t.teacher_name.clone(),
            branches: t.branches.clone(),
            capacity: t.capacity,
            free_slots: t.free_slots.iter().filter_map(|key| parse_slot_key(key)).collect(),
            already_assigned_hours: t.assigned_hours,
            // Dolu hücreler artık BLOK genişliğinde: yalnızca başlangıç
            // hücresi değil, `visit_hour..=visit_end_hour` arasının tamamı.
            used_slots: board
                .companies
                .iter()
                .filter(|c| c.assigned_teacher_id == Some(t.teacher_id))
                .filter_map(|c| match (c.visit_day, c.visit_hour, c.visit_end_hour) {
                    (Some(day), Some(start), Some(end)) => Some((day, start, end)),
                    _ => None,
                })
                .flat_map(|(day, start, end)| (start..=end).map(move |hour| Slot::new(day, hour)))
                .collect(),
            hours_by_day: t.hours_per_day.clone(),
        })
        .collect();

    Ok(propose(&companies, &teachers, board.day_end_hour))
}

/// `{gün}-{saat}` anahtarını dilime çevirir. Bozuk anahtar sessizce atlanır.
fn parse_slot_key(key: &str) -> Option<Slot> {
    let (day, hour) = key.split_once('-')?;
    Some(Slot::new(day.parse().ok()?, hour.parse().ok()?))
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
    use crate::db::init_pool;
    use crate::db::teaching_load::TermBranchHoursInput;
    use crate::db::teaching_load_test_support::{seed_teacher, TERM};
    use crate::domain::models::ChiefType;
    use chrono::NaiveDate;

    #[test]
    fn occupied_cells_by_teacher_expands_a_multi_hour_block() {
        // Öğretmen 1, 2. gün 3. saatten başlayıp 3 hücre kaplayan bir blokla dolu.
        let placements = [(1, 2, 3, 5, 100)];
        let occupied = occupied_cells_by_teacher(&placements);

        let teacher_cells = &occupied[&1];
        assert_eq!(teacher_cells.len(), 3);
        assert_eq!(teacher_cells["2-3"], 100);
        assert_eq!(teacher_cells["2-4"], 100);
        assert_eq!(teacher_cells["2-5"], 100);
    }

    /// Fahri ziyaret (başlangıç = bitiş) tek hücre kaplar.
    #[test]
    fn occupied_cells_by_teacher_handles_a_single_cell_block() {
        let placements = [(1, 1, 9, 9, 200)];
        let occupied = occupied_cells_by_teacher(&placements);

        assert_eq!(occupied[&1].len(), 1);
        assert_eq!(occupied[&1]["1-9"], 200);
    }

    #[test]
    fn occupied_cells_by_teacher_keeps_teachers_separate() {
        let placements = [(1, 1, 9, 10, 100), (2, 1, 9, 9, 200)];
        let occupied = occupied_cells_by_teacher(&placements);

        assert_eq!(occupied.len(), 2);
        assert_eq!(occupied[&1]["1-9"], 100);
        assert_eq!(occupied[&2]["1-9"], 200);
    }

    #[test]
    fn occupied_cells_by_teacher_is_empty_for_no_placements() {
        assert!(occupied_cells_by_teacher(&[]).is_empty());
    }

    /// Atama tahtasındaki işletme kartı ilçeyi taşımalı — İşletme Dağıtımı
    /// ekranı atanmamış işletmeleri ilçe bazlı gruplayacak (kaynak talep).
    #[tokio::test]
    async fn board_company_carries_the_district_field() {
        use crate::domain::models::NewCompany;

        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        companies::create(
            &pool,
            &NewCompany {
                name: "Test İşletme".into(),
                contact_first_name: String::new(),
                contact_last_name: String::new(),
                phone: String::new(),
                email: String::new(),
                address_text: "33130 Akdeniz/Mersin".into(),
                latitude: None,
                longitude: None,
                one_way_distance_km: Some(5.0),
                district: String::new(),
                notes: String::new(),
            },
        )
        .await
        .unwrap();

        let board = load_board(&AppState { pool }).await.unwrap();

        assert_eq!(board.companies.len(), 1);
        assert_eq!(board.companies[0].district, "Akdeniz");
    }

    /// Atama tahtasının saat aralığı da AYNI türetmeden gelir
    /// (`settings::lesson_hour_bounds`); "Gün Başlangıç Saati" ayarı kalktı.
    #[tokio::test]
    async fn board_hours_default_to_one_through_ten_when_max_daily_lessons_is_unset() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();

        let board = load_board(&AppState { pool }).await.unwrap();

        assert_eq!(board.day_start_hour, 1);
        assert_eq!(board.day_end_hour, 10);
    }

    #[tokio::test]
    async fn board_hours_follow_the_max_daily_lessons_setting() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        settings::set(&pool, "max_daily_lessons", "6").await.unwrap();

        let board = load_board(&AppState { pool }).await.unwrap();

        assert_eq!(board.day_start_hour, 1);
        assert_eq!(board.day_end_hour, 7);
    }

    #[test]
    fn slot_keys_round_trip() {
        assert_eq!(parse_slot_key("3-10"), Some(Slot::new(3, 10)));
        assert_eq!(parse_slot_key("1-8"), Some(Slot::new(1, 8)));
    }

    /// Bozuk anahtar panik yerine None vermeli.
    #[test]
    fn malformed_slot_keys_are_ignored() {
        assert_eq!(parse_slot_key("bozuk"), None);
        assert_eq!(parse_slot_key("a-b"), None);
        assert_eq!(parse_slot_key(""), None);
    }


    /// Atama tahtasındaki havuz sayacı da TAM havuzdur: şeflik saatleri
    /// (alan şefi 10) Σ(saat × grup)'a eklenir.
    #[tokio::test]
    async fn assignment_board_pool_includes_chief_hours() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        let row = TermBranchHoursInput {
            grade: "12/C".into(),
            branch: "Dal".into(),
            weekly_hours: 24,
            group_count: 2,
            is_group_manual: true,
        };
        teaching_load::replace_for_term(&pool, TERM, &[row]).await.unwrap();
        seed_teacher(&pool, "Alan", ChiefType::Department).await;

        let board = load_board(&AppState { pool }).await.unwrap();

        assert_eq!(board.pool_hours, 24 * 2 + 10);
    }

    /// Kilit test (spec R5c, "üç yer"in biri): öğretmen kapasitesi de
    /// projeksiyondan okunur. `change_chief_type` yalnız kapıdan yazar; eski
    /// `teachers.chief_type` sütunu değişmese bile tahtadaki kapasite
    /// yeni şefliğe göre değişmeli.
    #[tokio::test]
    async fn teacher_capacity_on_the_board_follows_the_projection_not_the_legacy_column() {
        use crate::db::teaching_load_test_support::change_chief_type_in_planning;

        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        let teacher_id = seed_teacher(&pool, "Ada", ChiefType::None).await;

        let before = load_board(&AppState { pool: pool.clone() }).await.unwrap();
        // Varsayılan ayar (migration 0001): "other" + büyükşehir => tavan 20.
        // Şefsizken bütçe (24) tavanı aştığı için kapasite tavanda KLİPLENİR: 20.
        assert_eq!(before.teachers[0].capacity, 20);

        // Dönem başından hemen sonraki bir tarih: gerçek "bugün"den önce
        // kalır, bu yüzden `current_as_of` (gerçek bugünü kullanır) her
        // koşulda bu değişikliği görür.
        change_chief_type_in_planning(&pool, teacher_id, ChiefType::Department, NaiveDate::from_ymd_opt(2026, 9, 5).unwrap()).await;

        let after = load_board(&AppState { pool: pool.clone() }).await.unwrap();
        // Bölüm şefi 10 saat düşürür: bütçe 24-10=14, artık tavanın (20)
        // ALTINDA kaldığı için kapasite tam 14'e düşer (klipleme kalkar).
        assert_eq!(after.teachers[0].capacity, 14, "bölüm şefi 10 saat düşürmeli");

        let legacy: String = sqlx::query_scalar("SELECT chief_type FROM teachers WHERE id = ?1")
            .bind(teacher_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(legacy, "none", "eski sütun donuk kalmalı; okuma ona bakmıyor");
    }
}
