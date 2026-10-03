//! Tohumlu rastgele girdilerle değişmez denetimi. Üreteç yalnız GİRDİYİ
//! çeşitlendirir; motor rastgelelik kullanmaz. `Scenario::run` her çıktıda
//! değişmezleri, motor ise her kabul edilen hamlede iç toplamları denetler.

use super::*;

/// Debug derlemede birkaç saniyeyi aşmayacak sayıda tohum.
const SEEDS: u64 = 250;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: i64) -> i64 {
        (self.next() % bound as u64) as i64
    }

    fn chance(&mut self, percent: i64) -> bool {
        self.below(100) < percent
    }
}

struct Case {
    companies: Vec<CompanyInput>,
    teachers: Vec<TeacherInput>,
    pool: i64,
    gap: i64,
}

fn random_teacher(rng: &mut Rng, id: i64) -> TeacherInput {
    let mut teacher = teacher(id);
    if rng.chance(33) {
        teacher.branches = vec!["Makine".into()];
    }
    teacher.capacity = rng.below(24);
    for day in 1..=5 {
        for hour in DAY_START..DAY_END {
            if rng.chance(60) {
                teacher.free_slots.insert(Slot::new(day, hour));
            }
        }
    }
    if rng.chance(33) {
        teacher.base_assigned_hours = rng.below(6);
        let day = 1 + rng.below(5);
        teacher
            .base_hours_by_day
            .insert(day, teacher.base_assigned_hours);
    }
    teacher
}

fn random_company(rng: &mut Rng, id: i64) -> CompanyInput {
    let mut c = company(id);
    c.max_hours = rng.below(10);
    c.awarded_hours = rng.below(9);
    c.is_honorary = rng.chance(16);
    c.is_locked = rng.chance(12);
    c.student_count = rng.below(6);
    c.workplace_days = (1..=5).filter(|_| rng.chance(50)).collect();
    if rng.chance(50) {
        let group = rng.below(3);
        c.group_key = Some(format!("g{group}"));
        c.group_label = format!("Bölge {group}");
    }
    if rng.chance(33) {
        c.branches = vec!["Makine".into()];
    }
    c
}

fn random_case(seed: u64) -> Case {
    let mut rng = Rng(seed * 2_654_435_761 + 7);
    let teacher_count = 1 + rng.below(8);
    let company_count = 1 + rng.below(30);
    Case {
        teachers: (0..teacher_count)
            .map(|i| random_teacher(&mut rng, 100 + i))
            .collect(),
        companies: (0..company_count)
            .map(|i| random_company(&mut rng, 1 + i))
            .collect(),
        pool: if rng.chance(50) { 0 } else { rng.below(60) },
        gap: rng.below(6),
    }
}

/// FillGaps çıktısını mevcut atama sayıp rastgele kaynak/zorlama bayraklarıyla
/// Redistribute girdisine çevirir.
fn redistribute_input(case: &Case, first: &AllocationProposal, seed: u64) -> Vec<CompanyInput> {
    let mut rng = Rng(seed + 99);
    let mut companies = case.companies.clone();
    for c in companies.iter_mut() {
        let Some((teacher_id, day, hour, hours)) = final_of(first, c.id) else {
            continue;
        };
        c.awarded_hours = hours;
        c.current = Some(CurrentPlacement {
            teacher_id,
            visit_day: day,
            visit_hour: hour,
            is_forced: rng.chance(16),
            source: if rng.chance(33) {
                PlacementSource::Manual
            } else {
                PlacementSource::Proposal
            },
        });
        if rng.chance(25) {
            c.max_hours = rng.below(9);
        }
    }
    companies
}

#[test]
fn seeded_fuzz_keeps_every_invariant_in_both_modes() {
    for seed in 1..=SEEDS {
        let case = random_case(seed);
        let fill = Scenario::new(case.companies.clone(), case.teachers.clone())
            .pool(case.pool)
            .gap(case.gap);
        let first = fill.run();

        let companies = redistribute_input(&case, &first, seed);
        let forward = Scenario::new(companies.clone(), case.teachers.clone())
            .pool(case.pool)
            .gap(case.gap)
            .redistribute();
        let second = forward.run();

        let mut reversed = Scenario::new(companies, case.teachers.clone())
            .pool(case.pool)
            .gap(case.gap)
            .redistribute();
        reversed.companies.reverse();
        reversed.teachers.reverse();
        assert_eq!(
            reversed.run(),
            second,
            "tohum {seed}: girdi sırası sonucu etkiledi"
        );
    }
}
