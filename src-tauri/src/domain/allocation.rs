use crate::domain::scheduling::{pick_slots, Slot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Dağıtım motorunun girdisindeki bir işletme.
#[derive(Debug, Clone)]
pub struct CompanyInput {
    pub id: i64,
    pub name: String,
    pub branch: String,
    pub student_count: i64,
    /// Kural tablosundan gelen tavan; kural yoksa None ve işletme atlanır.
    pub max_hours: Option<i64>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// Öğrencilerin sınıflarının işletmede bulunduğu günlerin birleşimi.
    pub workplace_days: BTreeSet<i64>,
    pub one_way_distance_km: Option<f64>,
}

/// Dağıtım motorunun girdisindeki bir öğretmen.
#[derive(Debug, Clone)]
pub struct TeacherInput {
    pub id: i64,
    pub name: String,
    pub branches: Vec<String>,
    pub capacity: i64,
    pub free_slots: BTreeSet<Slot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedAssignment {
    pub company_id: i64,
    pub company_name: String,
    pub teacher_id: i64,
    pub teacher_name: String,
    pub awarded_hours: i64,
    pub max_hours: i64,
    pub slots: Vec<Slot>,
    /// Dal tam eşleşti mi, yoksa yakın alan olarak mı atandı?
    pub exact_branch_match: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnassignedCompany {
    pub company_id: i64,
    pub company_name: String,
    /// Türkçe gerekçe; kullanıcı ne yapması gerektiğini buradan anlar.
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AllocationProposal {
    pub assignments: Vec<ProposedAssignment>,
    pub unassigned: Vec<UnassignedCompany>,
}

/// İki nokta arasındaki düz çizgi farkı — yalnızca SIRALAMA sinyalidir.
/// Resmî mesafe değildir; saat tavanına giren tek değer
/// `one_way_distance_km * 2`'dir ve bu fonksiyondan etkilenmez.
fn planar_distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    (dx * dx + dy * dy).sqrt()
}

/// Bir öğretmene atanmış işletmelerin coğrafi merkezi.
fn centroid(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    if points.is_empty() {
        return None;
    }
    let count = points.len() as f64;
    let sum = points
        .iter()
        .fold((0.0, 0.0), |acc, p| (acc.0 + p.0, acc.1 + p.1));
    Some((sum.0 / count, sum.1 / count))
}

struct TeacherState<'a> {
    input: &'a TeacherInput,
    remaining_capacity: i64,
    used_slots: BTreeSet<Slot>,
    assigned_points: Vec<(f64, f64)>,
}

/// İşletmeleri koordinatör öğretmenlere dağıtır.
///
/// Sıra (spec §9):
/// 1. Uygunluk filtresi — dal eşleşmesi; yoksa "yakın alan" ikinci öncelik
/// 2. Sıralama — öğrenci sayısı azalan, eşitlikte okula uzaklık azalan
/// 3. Yerleştirme — kalan kapasitesi tavanı karşılayan ve mevcut işletmelerinin
///    coğrafi merkezine en yakın öğretmen
/// 4. Dilim yerleştirme — uygun dilimlerden `awarded_hours` kadar
///
/// Öneri `awarded_hours = max_hours` ile başlar; kullanıcı aşağı çeker.
/// Yerleştirilemeyen işletmeler sessizce düşmez, gerekçesiyle raporlanır.
pub fn propose(companies: &[CompanyInput], teachers: &[TeacherInput]) -> AllocationProposal {
    let mut proposal = AllocationProposal::default();

    if teachers.is_empty() {
        for company in companies {
            proposal.unassigned.push(UnassignedCompany {
                company_id: company.id,
                company_name: company.name.clone(),
                reason: "Aktif öğretmen yok".into(),
            });
        }
        return proposal;
    }

    let mut states: Vec<TeacherState> = teachers
        .iter()
        .map(|input| TeacherState {
            input,
            remaining_capacity: input.capacity,
            used_slots: BTreeSet::new(),
            assigned_points: Vec::new(),
        })
        .collect();

    // Sıralama: çok öğrencili ve uzak işletmeler önce yerleşsin.
    let mut ordered: Vec<&CompanyInput> = companies.iter().collect();
    ordered.sort_by(|a, b| {
        b.student_count.cmp(&a.student_count).then(
            b.one_way_distance_km
                .unwrap_or(0.0)
                .total_cmp(&a.one_way_distance_km.unwrap_or(0.0)),
        )
    });

    for company in ordered {
        let Some(max_hours) = company.max_hours else {
            proposal.unassigned.push(UnassignedCompany {
                company_id: company.id,
                company_name: company.name.clone(),
                reason: "Mesafe ve öğrenci sayısına uyan saat kuralı yok".into(),
            });
            continue;
        };

        if company.workplace_days.is_empty() {
            proposal.unassigned.push(UnassignedCompany {
                company_id: company.id,
                company_name: company.name.clone(),
                reason: "Öğrencilerin sınıfı için işletme günü tanımlanmamış".into(),
            });
            continue;
        }

        let company_point = match (company.latitude, company.longitude) {
            (Some(lat), Some(lon)) => Some((lat, lon)),
            _ => None,
        };

        // Aday seçimi: önce tam dal eşleşmesi, sonra yakın alan.
        let mut best: Option<(usize, Vec<Slot>, bool, f64)> = None;

        for exact_only in [true, false] {
            for (index, state) in states.iter().enumerate() {
                let matches_branch = state.input.branches.iter().any(|b| b == &company.branch);
                if exact_only && !matches_branch {
                    continue;
                }
                if !exact_only && matches_branch {
                    // Zaten ilk turda değerlendirildi.
                    continue;
                }
                if state.remaining_capacity < max_hours {
                    continue;
                }

                let eligible: BTreeSet<Slot> = state
                    .input
                    .free_slots
                    .iter()
                    .copied()
                    .filter(|slot| company.workplace_days.contains(&slot.day_of_week))
                    .collect();

                let picked = pick_slots(&eligible, &state.used_slots, max_hours);
                if (picked.len() as i64) < max_hours {
                    continue;
                }

                // Kümeleme sinyali: öğretmenin mevcut işletmelerinin merkezine yakınlık.
                let proximity = match (company_point, centroid(&state.assigned_points)) {
                    (Some(point), Some(center)) => planar_distance(point, center),
                    // Henüz işletmesi olmayan öğretmen tercih edilsin diye 0.
                    _ => 0.0,
                };

                let is_better = match &best {
                    None => true,
                    Some((_, _, _, best_proximity)) => proximity < *best_proximity,
                };
                if is_better {
                    best = Some((index, picked, matches_branch, proximity));
                }
            }

            if best.is_some() {
                break;
            }
        }

        match best {
            Some((index, slots, exact_branch_match, _)) => {
                let state = &mut states[index];
                state.remaining_capacity -= max_hours;
                state.used_slots.extend(slots.iter().copied());
                if let Some(point) = company_point {
                    state.assigned_points.push(point);
                }

                proposal.assignments.push(ProposedAssignment {
                    company_id: company.id,
                    company_name: company.name.clone(),
                    teacher_id: state.input.id,
                    teacher_name: state.input.name.clone(),
                    awarded_hours: max_hours,
                    max_hours,
                    slots,
                    exact_branch_match,
                });
            }
            None => {
                let any_capacity = states.iter().any(|s| s.remaining_capacity >= max_hours);
                let reason = if any_capacity {
                    format!(
                        "Uygun gün/saat bulunamadı ({max_hours} saat gerekiyordu)"
                    )
                } else {
                    format!("Hiçbir öğretmende {max_hours} saatlik kapasite kalmadı")
                };
                proposal.unassigned.push(UnassignedCompany {
                    company_id: company.id,
                    company_name: company.name.clone(),
                    reason,
                });
            }
        }
    }

    // Çıktı okunabilir olsun: öğretmen sonra işletme adına göre.
    proposal.assignments.sort_by(|a, b| {
        a.teacher_name
            .cmp(&b.teacher_name)
            .then(a.company_name.cmp(&b.company_name))
    });

    proposal
}

/// Öğretmen başına toplam takdir saati (öneri üzerinden).
pub fn hours_by_teacher(proposal: &AllocationProposal) -> BTreeMap<i64, i64> {
    proposal
        .assignments
        .iter()
        .fold(BTreeMap::new(), |mut acc, assignment| {
            *acc.entry(assignment.teacher_id).or_insert(0) += assignment.awarded_hours;
            acc
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(pairs: &[(i64, i64)]) -> BTreeSet<Slot> {
        pairs.iter().map(|(d, h)| Slot::new(*d, *h)).collect()
    }

    fn company(id: i64, students: i64, max_hours: Option<i64>) -> CompanyInput {
        CompanyInput {
            id,
            name: format!("İşletme {id}"),
            branch: "Elektronik Haberleşme".into(),
            student_count: students,
            max_hours,
            latitude: Some(36.8),
            longitude: Some(34.6),
            workplace_days: BTreeSet::from([1, 2]),
            one_way_distance_km: Some(5.0),
        }
    }

    fn teacher(id: i64, capacity: i64) -> TeacherInput {
        let all_week: Vec<(i64, i64)> = (1..=5).flat_map(|d| (8..17).map(move |h| (d, h))).collect();
        TeacherInput {
            id,
            name: format!("Öğretmen {id}"),
            branches: vec!["Elektronik Haberleşme".into()],
            capacity,
            free_slots: slots(&all_week),
        }
    }

    #[test]
    fn assigns_a_company_to_an_eligible_teacher() {
        let proposal = propose(&[company(1, 1, Some(4))], &[teacher(10, 20)]);

        assert_eq!(proposal.assignments.len(), 1);
        assert!(proposal.unassigned.is_empty());

        let assignment = &proposal.assignments[0];
        assert_eq!(assignment.teacher_id, 10);
        assert_eq!(assignment.awarded_hours, 4);
        assert_eq!(assignment.slots.len(), 4);
        assert!(assignment.exact_branch_match);
    }

    /// Öneri tavanla başlar; kullanıcı aşağı çeker.
    #[test]
    fn proposal_starts_at_the_maximum_hours() {
        let proposal = propose(&[company(1, 1, Some(8))], &[teacher(10, 20)]);
        assert_eq!(proposal.assignments[0].awarded_hours, 8);
        assert_eq!(proposal.assignments[0].max_hours, 8);
    }

    /// Kapasite tükenince kalan işletmeler gerekçesiyle raporlanır.
    #[test]
    fn reports_companies_that_exceed_remaining_capacity() {
        let companies = vec![company(1, 2, Some(8)), company(2, 1, Some(8))];
        let proposal = propose(&companies, &[teacher(10, 10)]);

        assert_eq!(proposal.assignments.len(), 1);
        assert_eq!(proposal.unassigned.len(), 1);
        assert!(proposal.unassigned[0].reason.contains("kapasite"));
    }

    /// Saat kuralı olmayan işletme atlanır ama sessizce düşmez.
    #[test]
    fn company_without_hour_rule_is_reported() {
        let proposal = propose(&[company(1, 1, None)], &[teacher(10, 20)]);

        assert!(proposal.assignments.is_empty());
        assert_eq!(proposal.unassigned.len(), 1);
        assert!(proposal.unassigned[0].reason.contains("saat kuralı"));
    }

    /// Sınıfın işletme günü tanımlı değilse yerleştirme yapılamaz.
    #[test]
    fn company_without_workplace_days_is_reported() {
        let mut target = company(1, 1, Some(4));
        target.workplace_days = BTreeSet::new();

        let proposal = propose(&[target], &[teacher(10, 20)]);

        assert_eq!(proposal.unassigned.len(), 1);
        assert!(proposal.unassigned[0].reason.contains("işletme günü"));
    }

    #[test]
    fn all_companies_are_reported_when_there_are_no_teachers() {
        let proposal = propose(&[company(1, 1, Some(4)), company(2, 1, Some(4))], &[]);

        assert!(proposal.assignments.is_empty());
        assert_eq!(proposal.unassigned.len(), 2);
        assert!(proposal.unassigned[0].reason.contains("öğretmen yok"));
    }

    /// Dal eşleşmesi olan öğretmen, olmayana tercih edilir (OÖKY MADDE 88).
    #[test]
    fn exact_branch_match_is_preferred_over_near_field() {
        let mut other_branch = teacher(10, 20);
        other_branch.branches = vec!["Endüstriyel Bakım Onarım".into()];
        let exact = teacher(11, 20);

        let proposal = propose(&[company(1, 1, Some(4))], &[other_branch, exact]);

        assert_eq!(proposal.assignments[0].teacher_id, 11);
        assert!(proposal.assignments[0].exact_branch_match);
    }

    /// Dal eşleşmesi yoksa yakın alan öğretmenine atanır ve işaretlenir.
    #[test]
    fn falls_back_to_near_field_and_marks_it() {
        let mut only_other = teacher(10, 20);
        only_other.branches = vec!["Endüstriyel Bakım Onarım".into()];

        let proposal = propose(&[company(1, 1, Some(4))], &[only_other]);

        assert_eq!(proposal.assignments.len(), 1);
        assert!(!proposal.assignments[0].exact_branch_match);
    }

    /// Aynı dilim iki işletmeye verilemez.
    #[test]
    fn slots_are_not_reused_across_companies() {
        let companies = vec![company(1, 1, Some(4)), company(2, 1, Some(4))];
        let proposal = propose(&companies, &[teacher(10, 20)]);

        assert_eq!(proposal.assignments.len(), 2);
        let first: BTreeSet<Slot> = proposal.assignments[0].slots.iter().copied().collect();
        let second: BTreeSet<Slot> = proposal.assignments[1].slots.iter().copied().collect();
        assert!(first.is_disjoint(&second), "dilimler çakışmamalı");
    }

    /// Yerleştirilen dilimler yalnızca işletme günlerine düşmeli.
    #[test]
    fn picked_slots_fall_only_on_workplace_days() {
        let mut target = company(1, 1, Some(3));
        target.workplace_days = BTreeSet::from([4]);

        let proposal = propose(&[target], &[teacher(10, 20)]);

        assert!(proposal.assignments[0]
            .slots
            .iter()
            .all(|s| s.day_of_week == 4));
    }

    /// Öğretmenin boş saati yoksa yerleştirme yapılamaz.
    #[test]
    fn teacher_without_free_slots_cannot_take_a_company() {
        let mut busy = teacher(10, 20);
        busy.free_slots = BTreeSet::new();

        let proposal = propose(&[company(1, 1, Some(4))], &[busy]);

        assert!(proposal.assignments.is_empty());
        assert_eq!(proposal.unassigned.len(), 1);
        assert!(proposal.unassigned[0].reason.contains("gün/saat"));
    }

    #[test]
    fn hours_by_teacher_sums_per_teacher() {
        let companies = vec![company(1, 1, Some(4)), company(2, 1, Some(4))];
        let proposal = propose(&companies, &[teacher(10, 20)]);

        let totals = hours_by_teacher(&proposal);
        assert_eq!(totals[&10], 8);
    }

    /// Öğrenci sayısı yüksek işletme önce yerleşir.
    #[test]
    fn companies_with_more_students_are_placed_first() {
        let small = company(1, 1, Some(8));
        let big = company(2, 5, Some(8));

        // Kapasite yalnızca birine yeter.
        let proposal = propose(&[small, big], &[teacher(10, 8)]);

        assert_eq!(proposal.assignments.len(), 1);
        assert_eq!(proposal.assignments[0].company_id, 2, "çok öğrencili önce");
    }
}
