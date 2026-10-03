//! Çözümün değişken durumu ve amaç fonksiyonu (skor).
//!
//! Skor, öğretmen toplamlarından ARTIMLI olarak tutulur: bir yerleşimi
//! eklemek ya da çıkarmak O(öğretmen sayısı) sürer, böylece yerel arama
//! her adayı yerleştir → puanla → geri al diye deneyebilir.

use super::problem::{Problem, DAYS};

/// Bir işletmenin yerleşimi. `hours == 0` fahri ziyarettir ve 1 hücre kaplar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Placement {
    pub teacher: usize,
    pub day: i64,
    pub start: i64,
    pub hours: i64,
}

pub(super) type Changes = Vec<(usize, Option<Placement>)>;

/// Amaç: sözlük sırasıyla BÜYÜTÜLÜR (ürün kararı 2026-10-03).
/// 0 yerleşen · 1 −motor-fahri · 2 saat · 3 −eşitlik ihlali · 4 −bölge
/// bölünmesi · 5 −Σsaat² · 6 −Σişletme² · 7 tam dal eşleşmesi · 8 −churn.
/// Dal eşleşmesi bilerek sonda: ürün kararıyla yalnız eşitlik bozucudur,
/// OÖKY MADDE 88'in "dal uyumu" ifadesi gereği tamamen atılmadı.
pub(super) const SCORE_LEN: usize = 9;
pub(super) type Score = [i64; SCORE_LEN];

pub(super) struct TeacherState {
    pub rem_cap: i64,
    pub occ: u128,
    pub day_hours: [i64; DAYS],
    pub hours: i64,
    pub count: i64,
    groups: Vec<u32>,
    pub distinct: i64,
}

#[derive(Default)]
struct Totals {
    placed: i64,
    engine_honorary: i64,
    hours: i64,
    violation: i64,
    splits: i64,
    sum_sq_hours: i64,
    sum_sq_count: i64,
    exact: i64,
    churn: i64,
}

pub(super) struct State<'p, 'a> {
    pub p: &'p Problem<'a>,
    pub placement: Vec<Option<Placement>>,
    pub teachers: Vec<TeacherState>,
    totals: Totals,
    /// Sabit olmayan işletmelerin harcadığı saat (havuz bütçesi).
    pub movable_hours: i64,
    /// Sayaç: her aday puanlaması ve blok taraması bir birim sayılır.
    pub evals: u64,
}

pub(super) fn day_index(day: i64) -> Option<usize> {
    (1..=DAYS as i64).contains(&day).then(|| (day - 1) as usize)
}

impl<'p, 'a> State<'p, 'a> {
    /// Taban yük ve sabit yerleşimlerle başlayan durum.
    pub fn new(p: &'p Problem<'a>) -> Self {
        let teachers: Vec<TeacherState> = p
            .teachers
            .iter()
            .map(|t| {
                let mut day_hours = [0; DAYS];
                for (day, hours) in &t.base_hours_by_day {
                    if let Some(index) = day_index(*day) {
                        day_hours[index] = *hours;
                    }
                }
                TeacherState {
                    rem_cap: t.capacity - t.base_assigned_hours,
                    occ: p.slots_mask(t.base_used_slots.iter().map(|s| (s.day_of_week, s.hour))),
                    day_hours,
                    hours: t.base_assigned_hours,
                    count: 0,
                    groups: vec![0; p.group_count],
                    distinct: 0,
                }
            })
            .collect();
        let mut state = Self {
            p,
            placement: vec![None; p.companies.len()],
            teachers,
            totals: Totals::default(),
            movable_hours: 0,
            evals: 0,
        };
        state.init_totals();
        for c in 0..p.companies.len() {
            if let Some(placement) = p.fixed_placement[c] {
                state.place(c, placement);
            }
        }
        state
    }

    fn init_totals(&mut self) {
        let hours: Vec<i64> = self.teachers.iter().map(|t| t.hours).collect();
        self.totals.sum_sq_hours = hours.iter().map(|h| h * h).sum();
        for (i, a) in hours.iter().enumerate() {
            for b in &hours[i + 1..] {
                self.totals.violation += self.pair_violation(*a, *b);
            }
        }
        // Yerleşmemiş sabit-olmayan işletmelerin churn tabanı.
        self.totals.churn = self.p.movable.iter().map(|&c| self.churn_of(c, None)).sum();
    }

    pub fn score(&self) -> Score {
        let t = &self.totals;
        [
            t.placed,
            -t.engine_honorary,
            t.hours,
            -t.violation,
            -t.splits,
            -t.sum_sq_hours,
            -t.sum_sq_count,
            t.exact,
            -t.churn,
        ]
    }

    pub fn exhausted(&self) -> bool {
        self.evals >= super::MAX_EVALUATIONS
    }

    pub fn budget_left(&self) -> i64 {
        match self.p.budget {
            None => i64::MAX,
            Some(budget) => (budget - self.movable_hours).max(0),
        }
    }

    /// Motorun KENDİ yaptığı fahri mi? Kullanıcının fahri işaretledikleri ve
    /// tavanı zaten 0 olanlar sayılmaz (ürün kararı 2026-10-03).
    fn is_engine_honorary(&self, c: usize, hours: i64) -> bool {
        hours == 0
            && !self.p.is_fixed[c]
            && self.p.ceiling[c] > 0
            && !self.p.companies[c].is_honorary
    }

    /// Hareket maliyeti: yerinden oynayan işletme + saat farkı. Yerleşmemiş
    /// ama ataması olan işletme (serbest bırakılan) 1 + eski saat öder.
    fn churn_of(&self, c: usize, placement: Option<&Placement>) -> i64 {
        if self.p.is_fixed[c] {
            return 0;
        }
        let company = self.p.companies[c];
        let old_hours = company.awarded_hours.max(0);
        let Some(pl) = placement else {
            return if company.current.is_some() {
                1 + old_hours
            } else {
                0
            };
        };
        let moved = company.current.as_ref().is_some_and(|cur| {
            let same = cur.teacher_id == self.p.teachers[pl.teacher].id
                && cur.visit_day == pl.day
                && cur.visit_hour == pl.start;
            !same
        });
        i64::from(moved) + (pl.hours - old_hours).abs()
    }

    fn pair_violation(&self, a: i64, b: i64) -> i64 {
        ((a - b).abs() - self.p.gap).max(0)
    }

    /// Öğretmenin toplam saati değişince eşitlik ihlalini ve Σsaat²'yi günceller.
    fn shift_teacher_hours(&mut self, t: usize, delta: i64) {
        let before = self.teachers[t].hours;
        let after = before + delta;
        for (j, other) in self.teachers.iter().enumerate() {
            if j == t {
                continue;
            }
            self.totals.violation +=
                self.pair_violation(after, other.hours) - self.pair_violation(before, other.hours);
        }
        self.totals.sum_sq_hours += after * after - before * before;
        self.teachers[t].hours = after;
    }

    pub fn place(&mut self, c: usize, placement: Placement) {
        debug_assert!(self.placement[c].is_none());
        self.apply_delta(c, placement, 1);
        self.placement[c] = Some(placement);
    }

    pub fn unplace(&mut self, c: usize) -> Option<Placement> {
        let placement = self.placement[c].take()?;
        self.apply_delta(c, placement, -1);
        Some(placement)
    }

    /// `c`'nin yerleşimini `target` yapar (önce varsa kaldırır).
    pub fn set(&mut self, c: usize, target: Option<Placement>) {
        self.unplace(c);
        if let Some(placement) = target {
            self.place(c, placement);
        }
    }

    /// Bir değişiklik kümesini uygular.
    pub fn apply(&mut self, changes: &Changes) {
        // Önce hepsini kaldır: hedefler birbirinin eski hücrelerine düşebilir ve
        // sıralı `set` başkasının yeni bitlerini silerdi.
        for (c, _) in changes {
            self.unplace(*c);
        }
        for (c, target) in changes {
            if let Some(placement) = target {
                self.place(*c, *placement);
            }
        }
    }

    /// Yerleştirme (`sign = 1`) ve kaldırma (`sign = -1`) AYNI artış mantığını
    /// paylaşır; iki yönün ayrı yazılması sapma üretirdi.
    fn apply_delta(&mut self, c: usize, pl: Placement, sign: i64) {
        let mask = self.p.placement_mask(&pl);
        let exact = i64::from(self.p.is_exact(pl.teacher, c));
        let honorary = i64::from(self.is_engine_honorary(c, pl.hours));
        let churn = self.churn_of(c, Some(&pl)) - self.churn_of(c, None);
        self.totals.placed += sign;
        self.totals.engine_honorary += sign * honorary;
        self.totals.hours += sign * pl.hours;
        self.totals.exact += sign * exact;
        self.totals.churn += sign * churn;
        if self.p.spends_budget(c) {
            self.movable_hours += sign * pl.hours;
        }
        self.shift_teacher_hours(pl.teacher, sign * pl.hours);

        let group = self.p.group[c];
        let teacher = &mut self.teachers[pl.teacher];
        let splits_before = (teacher.distinct - 1).max(0);
        let count_before = teacher.count;
        teacher.count += sign;
        teacher.rem_cap -= sign * pl.hours;
        if sign > 0 {
            teacher.occ |= mask;
        } else {
            teacher.occ &= !mask;
        }
        if let Some(index) = day_index(pl.day) {
            teacher.day_hours[index] += sign * pl.hours;
        }
        if let Some(g) = group {
            let entry = &mut teacher.groups[g];
            if sign > 0 {
                *entry += 1;
                teacher.distinct += i64::from(*entry == 1);
            } else {
                *entry -= 1;
                teacher.distinct -= i64::from(*entry == 0);
            }
        }
        let splits_after = (teacher.distinct - 1).max(0);
        let count_after = teacher.count;
        self.totals.splits += splits_after - splits_before;
        self.totals.sum_sq_count += count_after * count_after - count_before * count_before;
    }
}
