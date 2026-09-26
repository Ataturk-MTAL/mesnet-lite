use crate::db::assignments::NewAssignment;
use crate::db::{
    assignments, availability, class_days, companies, company_hours, settings, students, teachers,
    teaching_load, AppState,
};
use crate::domain::allocation::{
    propose, AllocationProposal, CompanyInput, TeacherInput,
};
use crate::domain::history::decide::{ChangeCommand, ChangeRequest, CoordinatorRow};
use crate::domain::scheduling::{visit_span, Slot, MAX_HOURS_PER_DAY};
use crate::domain::terms::{parse_date, today_local};
use crate::domain::validation::{check_pool, check_teacher_totals, Violation};
use crate::domain::workload::{statutory_cap, teacher_capacity, InstitutionType};
use crate::error::AppResult;
use crate::services::change_service::commit_legacy_change;
use chrono::NaiveDate;
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
    // `list` (süzülmüş): dağıtım havuzu bir yönetim ekranıdır, pasif işletme yeniden atanamaz.
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

/// Atama artık tarihçe kapısından (`change_service::execute_change`) yazılır:
/// eski `assignments::assign`, canlı okunan `assignments` tablosuna yazıyordu
/// ama tarihçe yalnız `coordination_periods` projeksiyonunu güncelliyordu —
/// ikisi hiç eşleşmiyordu (brief teşhisi). Slot çakışması, müsaitlik ve
/// zorlama-gerekçesi kontrolleri `decide::company::assign_coordinators` ve
/// `change_input::validate_command`de yaşar; burada ikinci bir kopyası
/// TUTULMAZ (DRY).
#[tauri::command]
pub async fn assign_company(
    state: State<'_, AppState>,
    input: NewAssignment,
    effective_date: Option<String>,
    reason: Option<String>,
) -> AppResult<AssignmentBoard> {
    // `today`, komut sınırında BİR kez hesaplanır (bkz. `services::company_merge`
    // içindeki aynı desen): asıl yazma adımı `today`yi parametre alır, testler
    // gerçek takvim gününe bağlı kalmadan sabit bir "bugün" ile sınayabilir.
    assign_company_for_term(&state, input, effective_date, reason, today_local()).await?;
    load_board(&state).await
}

pub(crate) async fn assign_company_for_term(
    state: &AppState,
    input: NewAssignment,
    effective_date: Option<String>,
    reason: Option<String>,
    today: NaiveDate,
) -> AppResult<()> {
    let term = settings::get_active_term(&state.pool).await?;
    let request = coordinator_request(&term, &input, effective_date, reason)?;
    commit_legacy_change(&state.pool, request, today).await
}

fn coordinator_request(
    term: &str,
    input: &NewAssignment,
    effective_date: Option<String>,
    reason: Option<String>,
) -> AppResult<ChangeRequest> {
    let effective_date = effective_date.map(|raw| parse_date(&raw)).transpose()?;
    let row = CoordinatorRow {
        company_id: input.company_id,
        teacher_id: input.teacher_id,
        visit_day: input.visit_day,
        visit_hour: input.visit_hour,
        is_forced: input.is_forced,
        force_reason: input.force_reason.clone(),
    };
    Ok(ChangeRequest {
        term: term.to_string(),
        effective_date,
        document_date: None,
        reason: reason.unwrap_or_default(),
        command: ChangeCommand::AssignCoordinators { rows: vec![row] },
    })
}

/// Eski `assignments::unassign` atama YOKSA sessizce geçerdi (idempotent);
/// `decide::company::end_coordination` ise önceki durum yoksa `FactNotTrueAtDate`
/// ile REDDEDER. Kaybı önlemek için (brief: "doğrulama kaybı yok") aynı
/// idempotentliği burada koruruz: açık bir atama yoksa kapıya hiç girmeden
/// panoyu olduğu gibi döneriz.
#[tauri::command]
pub async fn unassign_company(
    state: State<'_, AppState>,
    company_id: i64,
    effective_date: Option<String>,
    reason: Option<String>,
) -> AppResult<AssignmentBoard> {
    unassign_company_for_term(&state, company_id, effective_date, reason, today_local()).await?;
    load_board(&state).await
}

async fn unassign_company_for_term(
    state: &AppState,
    company_id: i64,
    effective_date: Option<String>,
    reason: Option<String>,
    today: NaiveDate,
) -> AppResult<()> {
    let term = settings::get_active_term(&state.pool).await?;
    if assignments::get_for_company(&state.pool, company_id, &term).await?.is_some() {
        let effective_date = effective_date.map(|raw| parse_date(&raw)).transpose()?;
        let request = ChangeRequest {
            term,
            effective_date,
            document_date: None,
            reason: reason.unwrap_or_default(),
            command: ChangeCommand::EndCoordination { company_id },
        };
        commit_legacy_change(&state.pool, request, today).await?;
    }
    Ok(())
}

/// Dönemdeki tüm atamaları siler. Geri alınamaz. `ClearCoordination` tasarım
/// gereği YALNIZ planlama evresinde çalışır (spec §5); dönem başladıysa bu,
/// eski davranıştan (her zaman izinliydi) BİLİNÇLİ bir kısıtlamadır — brief
/// "dönem başladıysa servis ne diyorsa o".
#[tauri::command]
pub async fn clear_assignments(
    state: State<'_, AppState>,
    effective_date: Option<String>,
    reason: Option<String>,
) -> AppResult<AssignmentBoard> {
    clear_assignments_for_term(&state, effective_date, reason, today_local()).await?;
    load_board(&state).await
}

async fn clear_assignments_for_term(
    state: &AppState,
    effective_date: Option<String>,
    reason: Option<String>,
    today: NaiveDate,
) -> AppResult<()> {
    let term = settings::get_active_term(&state.pool).await?;
    let effective_date = effective_date.map(|raw| parse_date(&raw)).transpose()?;
    let request = ChangeRequest {
        term,
        effective_date,
        document_date: None,
        reason: reason.unwrap_or_default(),
        command: ChangeCommand::ClearCoordination,
    };
    commit_legacy_change(&state.pool, request, today).await
}
#[cfg(test)]
#[path = "assignment_commands_tests.rs"]
mod tests;
