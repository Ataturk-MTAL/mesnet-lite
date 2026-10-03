//! Girdinin normalleştirilmiş, DEĞİŞMEZ görünümü.
//!
//! İşletmeler ve öğretmenler `id`'ye göre sıralanır: girdi sırası sonucu
//! etkilemesin (belirlenimcilik). Izgara bit maskeleriyle temsil edilir:
//! bir (gün, saat) hücresi tek bir bit, bir blok ardışık bitlerdir.

use super::model::{CompanyInput, EngineInput, ProposalMode, TeacherInput};
use super::state::Placement;
use crate::domain::scheduling::{visit_span, MAX_HOURS_PER_DAY};
use std::collections::BTreeMap;

/// Haftalık gün sayısı (Pazartesi–Cuma).
pub(super) const DAYS: usize = 5;

/// Izgara genişliği sınırı: `DAYS × MAX_GRID_LEN` bit `u128`'e sığmalı
/// (5 × 25 = 125 ≤ 128). Gerçek ızgara günde 8–10 hücredir.
pub(super) const MAX_GRID_LEN: i64 = 25;

pub(super) struct Problem<'a> {
    pub companies: Vec<&'a CompanyInput>,
    pub teachers: Vec<&'a TeacherInput>,
    pub day_start: i64,
    /// HARİÇ.
    pub day_end: i64,
    grid_len: i64,
    /// `[öğretmen * işletme_sayısı + işletme]`: boş saat ∩ işletme günleri.
    eligible: Vec<u128>,
    exact: Vec<bool>,
    pub ceiling: Vec<i64>,
    /// Alt sınır: kilitli işletmenin saati sabittir (`awarded..=awarded`).
    pub floor: Vec<i64>,
    pub is_fixed: Vec<bool>,
    /// Yalnız sabit olanlar; öğretmeni girdide yoksa `None`.
    pub fixed_placement: Vec<Option<Placement>>,
    pub group: Vec<Option<usize>>,
    pub group_count: usize,
    /// `None` = havuz kısıtsız.
    pub budget: Option<i64>,
    pub pool_overrun: bool,
    pub gap: i64,
    /// Sabit olmayan işletmeler, `id` sırasıyla.
    pub movable: Vec<usize>,
    /// Yerleştirme sırası: tavan↓, öğrenci↓, gün sayısı↑, id↑.
    pub construct_order: Vec<usize>,
    pub redistribute: bool,
    pub pool_hours: i64,
    pub warnings: Vec<String>,
}

impl<'a> Problem<'a> {
    pub fn new(input: &EngineInput<'a>) -> Self {
        let mut companies: Vec<&'a CompanyInput> = input.companies.iter().collect();
        companies.sort_by_key(|c| c.id);
        let mut teachers: Vec<&'a TeacherInput> = input.teachers.iter().collect();
        teachers.sort_by_key(|t| t.id);

        let mut warnings = Vec::new();
        let wanted = input.day_end_hour - input.day_start_hour;
        let grid_len = wanted.clamp(0, MAX_GRID_LEN);
        if wanted > MAX_GRID_LEN {
            warnings.push(format!(
                "Ders saati ızgarası {wanted} hücre; yalnız ilk {MAX_GRID_LEN} saat kullanıldı"
            ));
        }

        let mut problem = Self {
            companies,
            teachers,
            day_start: input.day_start_hour,
            day_end: input.day_start_hour + grid_len,
            grid_len,
            eligible: Vec::new(),
            exact: Vec::new(),
            ceiling: Vec::new(),
            floor: Vec::new(),
            is_fixed: Vec::new(),
            fixed_placement: Vec::new(),
            group: Vec::new(),
            group_count: 0,
            budget: None,
            pool_overrun: false,
            gap: input.balance_gap_hours.max(0),
            movable: Vec::new(),
            construct_order: Vec::new(),
            redistribute: input.mode == ProposalMode::Redistribute,
            pool_hours: input.pool_hours,
            warnings,
        };
        problem.fill_company_data(input.mode);
        problem.fill_matrices();
        problem.fill_budget(input.pool_hours);
        problem
    }

    fn fill_company_data(&mut self, mode: ProposalMode) {
        let grid_cap = self.grid_len.min(MAX_HOURS_PER_DAY);
        self.ceiling = self
            .companies
            .iter()
            .map(|c| {
                // Kilitli satırın saati sabittir; diğerleri tavana kadar oynar.
                // Kullanıcı-fahri bayrağı tavanı 0'a ÇAKMAZ (ürün kararı 2026-10-02).
                if c.is_locked {
                    c.awarded_hours.max(0)
                } else {
                    c.max_hours.min(grid_cap).max(0)
                }
            })
            .collect();
        self.floor = self
            .companies
            .iter()
            .zip(&self.ceiling)
            .map(|(c, ceiling)| if c.is_locked { *ceiling } else { 0 })
            .collect();
        self.is_fixed = self
            .companies
            .iter()
            .map(|c| is_fixed_company(c, mode))
            .collect();

        self.fill_groups();
        self.fixed_placement = (0..self.companies.len())
            .map(|c| self.fixed_placement_of(c))
            .collect();
        self.movable = (0..self.companies.len())
            .filter(|c| !self.is_fixed[*c])
            .collect();
        let mut order = self.movable.clone();
        order.sort_by_key(|&c| {
            let company = self.companies[c];
            (
                -self.ceiling[c],
                -company.student_count,
                company.workplace_days.len(),
                company.id,
            )
        });
        self.construct_order = order;
    }

    /// Bölge anahtarlarına sıralı dizin verir (sözlük sırası, belirlenimci).
    fn fill_groups(&mut self) {
        let mut keys: BTreeMap<&str, usize> = BTreeMap::new();
        for key in self.companies.iter().filter_map(|c| c.group_key.as_deref()) {
            keys.entry(key).or_insert(0);
        }
        for (index, slot) in keys.values_mut().enumerate() {
            *slot = index;
        }
        self.group_count = keys.len();
        self.group = self
            .companies
            .iter()
            .map(|c| c.group_key.as_deref().map(|key| keys[key]))
            .collect();
    }

    fn fixed_placement_of(&mut self, c: usize) -> Option<Placement> {
        if !self.is_fixed[c] {
            return None;
        }
        let company = self.companies[c];
        let current = company.current.as_ref()?;
        let Some(teacher) = self.teacher_index(current.teacher_id) else {
            self.warnings.push(format!(
                "{} işletmesinin sabit atamasındaki öğretmen ({}) girdide yok; öğretmen yükü hesaba katılamadı",
                company.name, current.teacher_id
            ));
            return None;
        };
        Some(Placement {
            teacher,
            day: current.visit_day,
            start: current.visit_hour,
            hours: company.awarded_hours.max(0),
        })
    }

    fn fill_matrices(&mut self) {
        let free: Vec<u128> = self
            .teachers
            .iter()
            .map(|t| self.slots_mask(t.free_slots.iter().map(|s| (s.day_of_week, s.hour))))
            .collect();
        let days: Vec<u128> = self
            .companies
            .iter()
            .map(|c| self.days_mask(c.workplace_days.iter().copied()))
            .collect();
        for (t, teacher) in self.teachers.iter().enumerate() {
            for (c, company) in self.companies.iter().enumerate() {
                self.eligible.push(free[t] & days[c]);
                self.exact.push(
                    company
                        .branches
                        .iter()
                        .any(|branch| teacher.branches.contains(branch)),
                );
            }
        }
    }

    /// Bütçe = max(havuz, eski toplam) − ayrılanlar − taban. Havuz aşılmışsa
    /// toplamı ARTIRMAYAN değişiklik serbesttir (uygulama kapısının
    /// `new_total <= old_total` kuralıyla tutarlı). Sabit VE kilitli
    /// işletmelerin saati ayrılır: yerleşemeseler bile kayıtta yürürlüktedir.
    fn fill_budget(&mut self, pool_hours: i64) {
        if pool_hours <= 0 {
            return;
        }
        let reserved: i64 = (0..self.companies.len())
            .filter(|c| self.is_fixed[*c] || self.companies[*c].is_locked)
            .map(|c| self.companies[c].awarded_hours.max(0))
            .sum();
        let base: i64 = self.teachers.iter().map(|t| t.base_assigned_hours).sum();
        let old_total: i64 = self.companies.iter().map(|c| c.awarded_hours).sum::<i64>() + base;
        // Havuz zaten aşılmışsa yeni saat dağıtılmaz (MADDE 15/2 toplam havuz).
        self.pool_overrun = old_total > pool_hours;
        self.budget = Some((pool_hours.max(old_total) - reserved - base).max(0));
        if self.pool_overrun {
            self.warnings.push(format!(
                "Havuz zaten aşılmış ({old_total} saat / {pool_hours}); toplam artırılmadı"
            ));
        }
    }

    /// Bu işletmenin saati havuz bütçesinden mi harcanır? Sabit ve kilitli
    /// işletmelerin saati `fill_budget`'ta baştan ayrılmıştır.
    pub fn spends_budget(&self, c: usize) -> bool {
        !self.is_fixed[c] && !self.companies[c].is_locked
    }

    pub fn teacher_index(&self, id: i64) -> Option<usize> {
        self.teachers.binary_search_by_key(&id, |t| t.id).ok()
    }

    pub fn eligible(&self, teacher: usize, company: usize) -> u128 {
        self.eligible[teacher * self.companies.len() + company]
    }

    pub fn is_exact(&self, teacher: usize, company: usize) -> bool {
        self.exact[teacher * self.companies.len() + company]
    }

    /// (gün, saat) hücresinin bit konumu; ızgara dışındaysa `None`.
    pub fn cell_bit(&self, day: i64, hour: i64) -> Option<u32> {
        let in_week = (1..=DAYS as i64).contains(&day);
        let in_day = hour >= self.day_start && hour < self.day_end;
        if !in_week || !in_day {
            return None;
        }
        Some(((day - 1) * self.grid_len + (hour - self.day_start)) as u32)
    }

    pub fn slots_mask(&self, slots: impl Iterator<Item = (i64, i64)>) -> u128 {
        slots
            .filter_map(|(day, hour)| self.cell_bit(day, hour))
            .fold(0, |mask, bit| mask | (1u128 << bit))
    }

    fn days_mask(&self, days: impl Iterator<Item = i64>) -> u128 {
        let row = (1u128 << self.grid_len) - 1;
        days.filter(|d| (1..=DAYS as i64).contains(d))
            .fold(0, |mask, day| mask | (row << ((day - 1) * self.grid_len)))
    }

    /// `span` ardışık hücrenin maskesi; gün sınırını aşıyorsa `None`.
    pub fn block_mask(&self, day: i64, start: i64, span: i64) -> Option<u128> {
        if span < 1 || start + span > self.day_end {
            return None;
        }
        let bit = self.cell_bit(day, start)?;
        Some(((1u128 << span) - 1) << bit)
    }

    /// Bir yerleşimin kapladığı hücreler. Izgara dışındaki sabit atama
    /// (ör. girdi hatası) hücre çakışması üretmez, yalnız saat yükü taşır.
    pub fn placement_mask(&self, placement: &Placement) -> u128 {
        self.block_mask(placement.day, placement.start, visit_span(placement.hours))
            .unwrap_or(0)
    }
}

/// FillGaps: ataması olan her işletme sabit. Redistribute: kilitli, zorlanmış
/// ya da elle yapılmış atama sabit; yalnız önerilmiş (`Proposal`), kilitsiz ve
/// zorlamasız olanlar oynar (ürün kararı 2026-10-03).
fn is_fixed_company(company: &CompanyInput, mode: ProposalMode) -> bool {
    use super::model::PlacementSource;
    let Some(current) = &company.current else {
        return false;
    };
    match mode {
        ProposalMode::FillGaps => true,
        ProposalMode::Redistribute => {
            company.is_locked || current.is_forced || current.source == PlacementSource::Manual
        }
    }
}
