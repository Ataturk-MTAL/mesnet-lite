//! Dağıtım motorunun girdisini panodan ve veritabanından kurar (Issue #43).
//!
//! Motor saf alan kodudur ve veritabanını bilmez; "hangi işletme, hangi tavan,
//! hangi taban yük" kararları burada, komut sınırında verilir.

use super::{parse_slot_key, AssignmentBoard, BoardCompany, BoardTeacher};
use crate::db::read_at::ReadAt;
use crate::db::company_hours::CompanyTermHours;
use crate::db::{assignments, companies, company_hours, hour_rules, settings, AppState};
use crate::domain::history::policy::cap_for;
use crate::domain::hour_rules::HourRule;
use crate::domain::optimizer::{
    CompanyInput, CurrentPlacement, EngineInput, PlacementSource, ProposalMode, TeacherInput,
};
use crate::domain::scheduling::{Block, Slot};
use crate::error::{AppError, AppResult};
use std::collections::{BTreeMap, BTreeSet};

/// Haftanın geçerli gün aralığı (Pazartesi = 1 … Cuma = 5).
const VALID_DAYS: std::ops::RangeInclusive<i64> = 1..=5;

/// Motor girdisinin SAHİBİ olan hâli: `EngineInput` dilim ödünç aldığı için
/// komut, verileri burada tutup `engine_input` ile ödünç verir.
#[derive(Debug)]
pub(super) struct ProposalInput {
    pub companies: Vec<CompanyInput>,
    pub teachers: Vec<TeacherInput>,
    pub day_start_hour: i64,
    pub day_end_hour: i64,
    pub pool_hours: i64,
    pub balance_gap_hours: i64,
}

impl ProposalInput {
    pub fn engine_input(&self, mode: ProposalMode) -> EngineInput<'_> {
        EngineInput {
            companies: &self.companies,
            teachers: &self.teachers,
            day_start_hour: self.day_start_hour,
            day_end_hour: self.day_end_hour,
            pool_hours: self.pool_hours,
            mode,
            balance_gap_hours: self.balance_gap_hours,
        }
    }

    /// Panodaki TÜM aktif işletmeleri (atanmış ya da değil) ve aktif
    /// öğretmenleri motor girdisine çevirir; çıkmadan önce `validate_input`.
    pub async fn load(state: &AppState, board: &AssignmentBoard) -> AppResult<Self> {
        let pool = &state.pool;
        let all_settings = settings::get_all(pool).await?;
        let rules = hour_rules::list(pool).await?;
        let hours_by_company: BTreeMap<i64, CompanyTermHours> =
            company_hours::list(pool, &board.term, &ReadAt::Latest)
                .await?
                .into_iter()
                .map(|row| (row.company_id, row))
                .collect();
        let round_trip_by_company: BTreeMap<i64, Option<f64>> = companies::list(pool)
            .await?
            .iter()
            .map(|company| (company.id, company.round_trip_distance_km()))
            .collect();

        let companies = board
            .companies
            .iter()
            .map(|card| {
                let round_trip = round_trip_by_company.get(&card.company_id).copied().flatten();
                company_input(card, hours_by_company.get(&card.company_id), &rules, round_trip)
            })
            .collect::<AppResult<Vec<_>>>()?;

        let base = base_loads(state, board, &hours_by_company).await?;
        let teachers = board
            .teachers
            .iter()
            .map(|teacher| teacher_input(teacher, base.get(&teacher.teacher_id)))
            .collect::<AppResult<Vec<_>>>()?;

        let input = Self {
            companies,
            teachers,
            day_start_hour: board.day_start_hour,
            day_end_hour: board.day_end_hour,
            pool_hours: board.pool_hours,
            balance_gap_hours: settings::balance_gap_hours(&all_settings),
        };
        validate_input(&input)?;
        Ok(input)
    }
}

/// Motora giden saat tavanı: kayıttaki anlık görüntü ile CANLI kuralın
/// küçüğü. Anlık görüntü karar anındaki tavandır; sonradan öğrenci azalmış ya
/// da kural sıkılaşmış olabilir, bu yüzden canlı kural yalnız aşağı çeker.
/// Canlı tavan bilinmiyorsa (`cap_for` → `None`: mesafe yok) anlık görüntü;
/// kayıt hiç yoksa canlı tavan, o da yoksa 0 (saat verilemez).
pub(super) fn ceiling_for(snapshot: Option<i64>, live: Option<i64>) -> i64 {
    match (snapshot, live) {
        (Some(snapshot), Some(live)) => snapshot.min(live),
        (Some(snapshot), None) => snapshot,
        (None, Some(live)) => live,
        (None, None) => 0,
    }
}

/// `coordination_periods.source` metnini motorun kaynağına eşler. Sütunda
/// CHECK kısıtı var; başka bir değer şemanın bozulduğu anlamına gelir.
fn placement_source(raw: &str) -> AppResult<PlacementSource> {
    match raw {
        "manual" => Ok(PlacementSource::Manual),
        "proposal" => Ok(PlacementSource::Proposal),
        other => Err(AppError::Database(format!(
            "Bilinmeyen atama kaynağı \"{other}\" (yalnız manual/proposal olabilir). \
             Bu bir program hatasıdır; lütfen bildirin."
        ))),
    }
}

fn current_placement(card: &BoardCompany) -> AppResult<Option<CurrentPlacement>> {
    let (Some(teacher_id), Some(visit_day), Some(visit_hour)) =
        (card.assigned_teacher_id, card.visit_day, card.visit_hour)
    else {
        return Ok(None);
    };
    let raw_source = card.assignment_source.as_deref().ok_or_else(|| {
        inconsistent(format!("işletme {} atanmış ama atama kaynağı yok", card.company_id))
    })?;
    let source = placement_source(raw_source)?;
    Ok(Some(CurrentPlacement { teacher_id, visit_day, visit_hour, is_forced: card.is_forced, source }))
}

fn company_input(
    card: &BoardCompany,
    hours: Option<&CompanyTermHours>,
    rules: &[HourRule],
    round_trip_km: Option<f64>,
) -> AppResult<CompanyInput> {
    let live_cap = cap_for(rules, round_trip_km, card.student_count);
    Ok(CompanyInput {
        id: card.company_id,
        name: card.company_name.clone(),
        branches: card.branches.clone(),
        student_count: card.student_count,
        workplace_days: card.workplace_days.iter().copied().collect(),
        awarded_hours: card.awarded_hours,
        is_honorary: card.is_honorary,
        max_hours: ceiling_for(hours.map(|row| row.max_hours_snapshot), live_cap),
        is_locked: hours.is_some_and(|row| row.is_locked == 1),
        group_key: card.group_key.clone(),
        group_label: card.group_label.clone(),
        current: current_placement(card)?,
    })
}

/// Bir öğretmenin motor girdisinde OLMAYAN işletmelerden (ör. pasif) gelen yükü.
#[derive(Debug, Default)]
struct BaseLoad {
    hours: i64,
    used_slots: BTreeSet<Slot>,
    hours_by_day: BTreeMap<i64, i64>,
}

/// Girdide olmayan işletmelerin atamalarından öğretmen başına taban yük.
/// Girdideki işletmelerin atamaları BİLİNÇLİ olarak dışarıda tutulur: motor
/// onları sabit atama olarak kendisi ekler, ikisi de sayılırsa yük çift olur.
async fn base_loads(
    state: &AppState,
    board: &AssignmentBoard,
    hours_by_company: &BTreeMap<i64, CompanyTermHours>,
) -> AppResult<BTreeMap<i64, BaseLoad>> {
    let in_input: BTreeSet<i64> = board.companies.iter().map(|c| c.company_id).collect();
    let rows = assignments::list(&state.pool, &board.term, &ReadAt::Latest).await?;
    let mut result: BTreeMap<i64, BaseLoad> = BTreeMap::new();
    for row in rows.iter().filter(|row| !in_input.contains(&row.company_id)) {
        let hours = hours_by_company.get(&row.company_id).map_or(0, |h| h.awarded_hours);
        let block = Block::from_start(row.visit_day, row.visit_hour, hours);
        let load = result.entry(row.teacher_id).or_default();
        load.hours += hours;
        load.used_slots.extend(block.cells());
        *load.hours_by_day.entry(row.visit_day).or_default() += hours;
    }
    Ok(result)
}

fn teacher_input(teacher: &BoardTeacher, base: Option<&BaseLoad>) -> AppResult<TeacherInput> {
    // Anahtarları panonun kendisi üretir; bozuk biri programlama hatasıdır,
    // sessizce atlanırsa öğretmenin boş saati görünmeden kaybolurdu.
    let free_slots = teacher
        .free_slots
        .iter()
        .map(|key| {
            parse_slot_key(key).ok_or_else(|| {
                AppError::Database(format!(
                    "Öğretmenin boş saat anahtarı çözülemedi: \"{key}\". Bu bir program hatasıdır; lütfen bildirin."
                ))
            })
        })
        .collect::<AppResult<BTreeSet<Slot>>>()?;
    let empty = BaseLoad::default();
    let base = base.unwrap_or(&empty);
    Ok(TeacherInput {
        id: teacher.teacher_id,
        name: teacher.teacher_name.clone(),
        branches: teacher.branches.clone(),
        capacity: teacher.capacity,
        free_slots,
        base_assigned_hours: base.hours,
        base_used_slots: base.used_slots.clone(),
        base_hours_by_day: base.hours_by_day.clone(),
    })
}

fn inconsistent(detail: String) -> AppError {
    AppError::Database(format!(
        "Dağıtım girdisi tutarsız: {detail}. Veriler veritabanından geldiği için bu bir \
         program hatasıdır; lütfen bildirin."
    ))
}

fn check_day(day: i64, owner: &str) -> AppResult<()> {
    if VALID_DAYS.contains(&day) {
        return Ok(());
    }
    Err(inconsistent(format!("{owner} için gün {day}, 1–5 aralığı dışında")))
}

fn check_unique(ids: impl Iterator<Item = i64>, what: &str) -> AppResult<()> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(inconsistent(format!("{what} kimliği {id} birden çok kez var")));
        }
    }
    Ok(())
}

/// Motorun doğrulamadığı sınır koşulları: kimlikler tekil, günler 1–5.
/// İhlal kullanıcı hatası değil veri bozukluğudur; sessiz geçilmez.
pub(super) fn validate_input(input: &ProposalInput) -> AppResult<()> {
    check_unique(input.companies.iter().map(|c| c.id), "işletme")?;
    check_unique(input.teachers.iter().map(|t| t.id), "öğretmen")?;
    for company in &input.companies {
        let owner = format!("işletme {}", company.id);
        for day in &company.workplace_days {
            check_day(*day, &owner)?;
        }
        if let Some(current) = &company.current {
            check_day(current.visit_day, &owner)?;
        }
    }
    for teacher in &input.teachers {
        let owner = format!("öğretmen {}", teacher.id);
        let slots = teacher.free_slots.iter().chain(&teacher.base_used_slots);
        for slot in slots {
            check_day(slot.day_of_week, &owner)?;
        }
        for day in teacher.base_hours_by_day.keys() {
            check_day(*day, &owner)?;
        }
    }
    Ok(())
}
