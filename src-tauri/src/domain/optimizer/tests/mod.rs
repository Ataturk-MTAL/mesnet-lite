//! Motor testleri için ortak kurgusal veri üreticileri.
//!
//! Yalnız uydurma ad ve sayılar kullanılır; gerçek veritabanından hiçbir
//! değer buraya kopyalanmaz.

mod hours;
mod placement;
mod policy;
mod scale;

use super::*;
use crate::domain::scheduling::Slot;
use std::collections::{BTreeMap, BTreeSet};

pub const DAY_START: i64 = 9;
/// HARİÇ: ızgara 9..=16 → günde 8 hücre.
pub const DAY_END: i64 = 17;
pub const DEFAULT_GAP: i64 = 4;

pub fn company(id: i64) -> CompanyInput {
    CompanyInput {
        id,
        name: format!("İşletme {id}"),
        branches: vec!["Elektrik".into()],
        student_count: 4,
        workplace_days: (1..=5).collect(),
        awarded_hours: 4,
        is_honorary: false,
        max_hours: 4,
        is_locked: false,
        group_key: None,
        group_label: String::new(),
        current: None,
    }
}

pub fn teacher(id: i64) -> TeacherInput {
    TeacherInput {
        id,
        name: format!("Öğretmen {id}"),
        branches: vec!["Elektrik".into()],
        capacity: 40,
        free_slots: BTreeSet::new(),
        base_assigned_hours: 0,
        base_used_slots: BTreeSet::new(),
        base_hours_by_day: BTreeMap::new(),
    }
}

/// `day` günü `hours` aralığındaki boş hücreler.
pub fn free(day: i64, hours: std::ops::Range<i64>) -> BTreeSet<Slot> {
    hours.map(|hour| Slot::new(day, hour)).collect()
}

pub fn free_on(days: &[i64], hours: std::ops::Range<i64>) -> BTreeSet<Slot> {
    days.iter()
        .flat_map(|day| free(*day, hours.clone()))
        .collect()
}

pub fn days(list: &[i64]) -> BTreeSet<i64> {
    list.iter().copied().collect()
}

pub fn current(
    teacher_id: i64,
    day: i64,
    hour: i64,
    source: PlacementSource,
) -> Option<CurrentPlacement> {
    Some(CurrentPlacement {
        teacher_id,
        visit_day: day,
        visit_hour: hour,
        is_forced: false,
        source,
    })
}

pub struct Scenario {
    pub companies: Vec<CompanyInput>,
    pub teachers: Vec<TeacherInput>,
    pub pool: i64,
    pub mode: ProposalMode,
    pub gap: i64,
    pub day_end: i64,
}

impl Scenario {
    pub fn new(companies: Vec<CompanyInput>, teachers: Vec<TeacherInput>) -> Self {
        Self {
            companies,
            teachers,
            pool: 0,
            mode: ProposalMode::FillGaps,
            gap: DEFAULT_GAP,
            day_end: DAY_END,
        }
    }

    pub fn redistribute(mut self) -> Self {
        self.mode = ProposalMode::Redistribute;
        self
    }

    pub fn pool(mut self, pool: i64) -> Self {
        self.pool = pool;
        self
    }

    pub fn gap(mut self, gap: i64) -> Self {
        self.gap = gap;
        self
    }

    pub fn day_end(mut self, day_end: i64) -> Self {
        self.day_end = day_end;
        self
    }

    /// Çalıştırır ve çıktının kural ihlali taşımadığını DOĞRULAR: böylece her
    /// senaryo, kendi beklentisinin yanında değişmezleri de sınar.
    pub fn run(&self) -> AllocationProposal {
        let proposal = self.optimize_raw();
        assert_invariants(self, &proposal);
        proposal
    }

    fn optimize_raw(&self) -> AllocationProposal {
        optimize(&EngineInput {
            companies: &self.companies,
            teachers: &self.teachers,
            day_start_hour: DAY_START,
            day_end_hour: self.day_end,
            pool_hours: self.pool,
            mode: self.mode,
            balance_gap_hours: self.gap,
        })
    }
}

/// Bir işletmenin nihai yerleşimi: (öğretmen, gün, saat, takdir edilen saat).
/// Önerilen ve yerinde kalan atamaların ikisine de bakar.
pub fn final_of(proposal: &AllocationProposal, company_id: i64) -> Option<(i64, i64, i64, i64)> {
    let moved = proposal
        .assignments
        .iter()
        .find(|a| a.company_id == company_id)
        .map(|a| (a.teacher_id, a.visit_day, a.visit_hour, a.awarded_hours));
    moved.or_else(|| {
        proposal
            .kept
            .iter()
            .find(|k| k.company_id == company_id)
            .map(|k| (k.teacher_id, k.visit_day, k.visit_hour, k.awarded_hours))
    })
}

pub fn hours_of(proposal: &AllocationProposal, company_id: i64) -> Option<i64> {
    final_of(proposal, company_id).map(|f| f.3)
}

pub fn teacher_of(proposal: &AllocationProposal, company_id: i64) -> Option<i64> {
    final_of(proposal, company_id).map(|f| f.0)
}

pub fn change_of(proposal: &AllocationProposal, company_id: i64) -> Option<&HourChange> {
    proposal
        .hour_changes
        .iter()
        .find(|c| c.company_id == company_id)
}

/// Her öğretmen için: hücre çakışması yok, günlük 8 saat (OÖKY MADDE 88),
/// kapasite (MADDE 15/2) ve havuz aşılmaz; önerilen bloklar boş saat ∩
/// işletme günleri içinde ve ızgarada kalır.
fn assert_invariants(scenario: &Scenario, proposal: &AllocationProposal) {
    for teacher in &scenario.teachers {
        let placed: Vec<(i64, i64, i64, i64)> = scenario
            .companies
            .iter()
            .filter_map(|c| final_of(proposal, c.id))
            .filter(|f| f.0 == teacher.id)
            .collect();
        assert_no_overlap(teacher, &placed);
        assert_load_limits(teacher, &placed);
    }
    assert_proposed_blocks_are_eligible(scenario, proposal);
    assert_pool_respected(scenario, proposal);
}

fn assert_no_overlap(teacher: &TeacherInput, placed: &[(i64, i64, i64, i64)]) {
    let mut cells = teacher.base_used_slots.clone();
    for (_, day, hour, hours) in placed {
        for cell in (*hour..*hour + (*hours).max(1)).map(|h| Slot::new(*day, h)) {
            assert!(
                cells.insert(cell),
                "{} için hücre çakışması: {cell:?}",
                teacher.name
            );
        }
    }
}

fn assert_load_limits(teacher: &TeacherInput, placed: &[(i64, i64, i64, i64)]) {
    let mut by_day = teacher.base_hours_by_day.clone();
    for (_, day, _, hours) in placed {
        *by_day.entry(*day).or_insert(0) += hours;
    }
    assert!(
        by_day.values().all(|h| *h <= 8),
        "günlük 8 saat aşıldı: {by_day:?}"
    );
    let total = teacher.base_assigned_hours + placed.iter().map(|f| f.3).sum::<i64>();
    assert!(
        total <= teacher.capacity.max(teacher.base_assigned_hours),
        "{} kapasitesi aşıldı: {total} > {}",
        teacher.name,
        teacher.capacity
    );
}

fn assert_proposed_blocks_are_eligible(scenario: &Scenario, proposal: &AllocationProposal) {
    for a in &proposal.assignments {
        let teacher = scenario
            .teachers
            .iter()
            .find(|t| t.id == a.teacher_id)
            .unwrap();
        let company = scenario
            .companies
            .iter()
            .find(|c| c.id == a.company_id)
            .unwrap();
        assert!(
            a.visit_end_hour < scenario.day_end,
            "blok ızgara dışına taştı"
        );
        assert!(
            company.workplace_days.contains(&a.visit_day),
            "işletme günü dışı"
        );
        for hour in a.visit_hour..=a.visit_end_hour {
            assert!(
                teacher.free_slots.contains(&Slot::new(a.visit_day, hour)),
                "{} için boş olmayan hücre: gün {} saat {hour}",
                teacher.name,
                a.visit_day
            );
        }
        assert!(a.awarded_hours <= company.max_hours.max(0), "tavan aşıldı");
    }
}

fn assert_pool_respected(scenario: &Scenario, proposal: &AllocationProposal) {
    let base: i64 = scenario
        .teachers
        .iter()
        .map(|t| t.base_assigned_hours)
        .sum();
    let old_total: i64 = scenario
        .companies
        .iter()
        .map(|c| c.awarded_hours)
        .sum::<i64>()
        + base;
    if scenario.pool <= 0 || old_total > scenario.pool {
        return;
    }
    assert!(
        proposal.total_hours + base <= scenario.pool,
        "havuz aşıldı: {} > {}",
        proposal.total_hours + base,
        scenario.pool
    );
}
