use crate::domain::scheduling::{eligible_slots, pick_visit_block, Block, Slot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Dağıtım motorunun girdisindeki bir işletme.
///
/// Saat burada GİRDİDİR, çıktı değil: takdir İşletme Saat Ayarları ekranında
/// yapılmıştır. Motor yalnızca "kim, ne zaman gidecek" sorusunu çözer.
#[derive(Debug, Clone)]
pub struct CompanyInput {
    pub id: i64,
    pub name: String,
    /// İşletmedeki öğrencilerin dalları.
    pub branches: Vec<String>,
    pub student_count: i64,
    /// Takdir edilen haftalık ek ders saati. Fahri ziyarette 0.
    pub awarded_hours: i64,
    pub is_honorary: bool,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// Öğrencilerin sınıflarının işletmede bulunduğu günler.
    pub workplace_days: BTreeSet<i64>,
    pub one_way_distance_km: Option<f64>,
}

/// Dağıtım motorunun girdisindeki bir öğretmen.
#[derive(Debug, Clone)]
pub struct TeacherInput {
    pub id: i64,
    pub name: String,
    pub branches: Vec<String>,
    /// Koordinatörlük kapasitesi (MADDE 15/2 tavanı eksi şeflik ve diğer ek dersler).
    pub capacity: i64,
    pub free_slots: BTreeSet<Slot>,
    /// Hâlihazırda atanmış saat — motor boş sayfadan başlamayabilir.
    pub already_assigned_hours: i64,
    /// Hâlihazırda dolu hücreler.
    pub used_slots: BTreeSet<Slot>,
    /// Gün → o güne düşen mevcut saat.
    pub hours_by_day: BTreeMap<i64, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedAssignment {
    pub company_id: i64,
    pub company_name: String,
    pub teacher_id: i64,
    pub teacher_name: String,
    /// İşletmenin takdir edilen saati; motor değiştirmez.
    pub awarded_hours: i64,
    pub visit_day: i64,
    pub visit_hour: i64,
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
///
/// Coğrafi olarak doğru bir mesafe değildir (enlem ve boylam dereceleri farklı
/// uzunluğa karşılık gelir) ve olması da gerekmez: burada sadece hangi
/// öğretmenin aday olacağı seçilir. Saat tavanına giren tek mesafe
/// `one_way_distance_km × 2`'dir ve bu fonksiyondan etkilenmez.
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
    hours_by_day: BTreeMap<i64, i64>,
    assigned_points: Vec<(f64, f64)>,
}

/// İşletmeleri koordinatör öğretmenlere dağıtır.
///
/// Sıra:
/// 1. Uygunluk filtresi — dal eşleşmesi; yoksa "yakın alan" ikinci öncelik (OÖKY MADDE 88)
/// 2. Sıralama — saat yükü azalan, eşitlikte öğrenci sayısı azalan
/// 3. Yerleştirme — kapasitesi yeten, uygun boş hücresi olan ve mevcut
///    işletmelerinin coğrafi merkezine en yakın öğretmen
///
/// Yerleştirilemeyen işletmeler sessizce düşmez, gerekçesiyle raporlanır.
///
/// `day_end_hour`: ızgaranın bitişi, HARİÇ (ayarlardan). Bir işletmenin
/// takdir edilen saati kadar ARDIŞIK boş hücreye ihtiyacı vardır; artık tek
/// bir boş hücre yeterli değildir.
pub fn propose(
    companies: &[CompanyInput],
    teachers: &[TeacherInput],
    day_end_hour: i64,
) -> AllocationProposal {
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
            remaining_capacity: input.capacity - input.already_assigned_hours,
            used_slots: input.used_slots.clone(),
            hours_by_day: input.hours_by_day.clone(),
            assigned_points: Vec::new(),
        })
        .collect();

    // Ağır işletmeler önce yerleşsin: küçükler artan boşluklara sığar.
    let mut ordered: Vec<&CompanyInput> = companies.iter().collect();
    ordered.sort_by(|a, b| {
        b.awarded_hours
            .cmp(&a.awarded_hours)
            .then(b.student_count.cmp(&a.student_count))
            .then(a.id.cmp(&b.id))
    });

    for company in ordered {
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
        let mut best: Option<(usize, Block, bool, f64)> = None;

        for exact_only in [true, false] {
            for (index, state) in states.iter().enumerate() {
                let matches_branch = company
                    .branches
                    .iter()
                    .any(|branch| state.input.branches.contains(branch));

                if exact_only && !matches_branch {
                    continue;
                }
                if !exact_only && matches_branch {
                    // Zaten ilk turda değerlendirildi.
                    continue;
                }
                // Fahri ziyaret kapasite yemez; 0 saat taşır.
                if state.remaining_capacity < company.awarded_hours {
                    continue;
                }

                let eligible = eligible_slots(&state.input.free_slots, &company.workplace_days);
                let Some(block) = pick_visit_block(
                    &eligible,
                    &state.used_slots,
                    &state.hours_by_day,
                    company.awarded_hours,
                    day_end_hour,
                ) else {
                    continue;
                };

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
                    best = Some((index, block, matches_branch, proximity));
                }
            }

            if best.is_some() {
                break;
            }
        }

        match best {
            Some((index, block, exact_branch_match, _)) => {
                let state = &mut states[index];
                state.remaining_capacity -= company.awarded_hours;
                state.used_slots.extend(block.cells());
                *state.hours_by_day.entry(block.day_of_week).or_insert(0) +=
                    company.awarded_hours;
                if let Some(point) = company_point {
                    state.assigned_points.push(point);
                }

                proposal.assignments.push(ProposedAssignment {
                    company_id: company.id,
                    company_name: company.name.clone(),
                    teacher_id: state.input.id,
                    teacher_name: state.input.name.clone(),
                    awarded_hours: company.awarded_hours,
                    visit_day: block.day_of_week,
                    visit_hour: block.start_hour,
                    exact_branch_match,
                });
            }
            None => {
                let any_capacity = states
                    .iter()
                    .any(|s| s.remaining_capacity >= company.awarded_hours);
                let reason = if any_capacity {
                    format!(
                        "Uygun gün/saat bulunamadı ({} saat, günlük 8 saat sınırı veya boş saat yok)",
                        company.awarded_hours
                    )
                } else {
                    format!(
                        "Hiçbir öğretmende {} saatlik kapasite kalmadı",
                        company.awarded_hours
                    )
                };
                proposal.unassigned.push(UnassignedCompany {
                    company_id: company.id,
                    company_name: company.name.clone(),
                    reason,
                });
            }
        }
    }

    // Çıktı okunabilir olsun: öğretmen, sonra gün ve saat.
    proposal.assignments.sort_by(|a, b| {
        a.teacher_name
            .cmp(&b.teacher_name)
            .then(a.visit_day.cmp(&b.visit_day))
            .then(a.visit_hour.cmp(&b.visit_hour))
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

    fn company(id: i64, awarded_hours: i64) -> CompanyInput {
        CompanyInput {
            id,
            name: format!("İşletme {id}"),
            branches: vec!["Elektronik Haberleşme".into()],
            student_count: 1,
            awarded_hours,
            is_honorary: false,
            latitude: Some(36.8),
            longitude: Some(34.6),
            workplace_days: BTreeSet::from([1, 2]),
            one_way_distance_km: Some(5.0),
        }
    }

    fn teacher(id: i64, capacity: i64) -> TeacherInput {
        let all_week: Vec<(i64, i64)> =
            (1..=5).flat_map(|d| (8..17).map(move |h| (d, h))).collect();
        TeacherInput {
            id,
            name: format!("Öğretmen {id}"),
            branches: vec!["Elektronik Haberleşme".into()],
            capacity,
            free_slots: slots(&all_week),
            already_assigned_hours: 0,
            used_slots: BTreeSet::new(),
            hours_by_day: BTreeMap::new(),
        }
    }

    #[test]
    fn assigns_a_company_to_one_cell() {
        let proposal = propose(&[company(1, 4)], &[teacher(10, 20)], 17);

        assert_eq!(proposal.assignments.len(), 1);
        assert!(proposal.unassigned.is_empty());

        let assignment = &proposal.assignments[0];
        assert_eq!(assignment.teacher_id, 10);
        assert_eq!(assignment.awarded_hours, 4);
        assert!((1..=5).contains(&assignment.visit_day));
        assert!(assignment.exact_branch_match);
    }

    /// Motor takdir edilen saati DEĞİŞTİRMEZ; o karar takdir ekranında verilir.
    #[test]
    fn proposal_preserves_the_awarded_hours() {
        let proposal = propose(&[company(1, 7)], &[teacher(10, 20)], 17);
        assert_eq!(proposal.assignments[0].awarded_hours, 7);
    }

    /// Günlük 8 saat sınırı: iki adet 5 saatlik işletme aynı güne konulamaz.
    #[test]
    fn two_companies_do_not_exceed_the_daily_cap_on_one_day() {
        let mut first = company(1, 5);
        let mut second = company(2, 5);
        // Her ikisi de yalnızca 1. günde ziyaret edilebilir.
        first.workplace_days = BTreeSet::from([1]);
        second.workplace_days = BTreeSet::from([1]);

        let proposal = propose(&[first, second], &[teacher(10, 20)], 17);

        assert_eq!(proposal.assignments.len(), 1, "biri yerleşebilmeli");
        assert_eq!(proposal.unassigned.len(), 1);
        assert!(proposal.unassigned[0].reason.contains("gün/saat"));
    }

    /// Farklı günlere dağılabiliyorsa ikisi de yerleşir.
    #[test]
    fn heavy_companies_spread_across_days() {
        let proposal = propose(&[company(1, 5), company(2, 5)], &[teacher(10, 20)], 17);

        assert_eq!(proposal.assignments.len(), 2);
        let days: BTreeSet<i64> = proposal.assignments.iter().map(|a| a.visit_day).collect();
        assert_eq!(days.len(), 2, "aynı güne yığılmamalı");
    }

    /// Kapasite tükenince kalan işletmeler gerekçesiyle raporlanır.
    #[test]
    fn reports_companies_that_exceed_remaining_capacity() {
        let proposal = propose(&[company(1, 8), company(2, 8)], &[teacher(10, 10)], 17);

        assert_eq!(proposal.assignments.len(), 1);
        assert_eq!(proposal.unassigned.len(), 1);
        assert!(proposal.unassigned[0].reason.contains("kapasite"));
    }

    /// Hâlihazırda atanmış saat kapasiteden düşülür.
    #[test]
    fn existing_assignments_reduce_remaining_capacity() {
        let mut busy = teacher(10, 20);
        busy.already_assigned_hours = 18;

        let proposal = propose(&[company(1, 4)], &[busy], 17);

        assert!(proposal.assignments.is_empty());
        assert!(proposal.unassigned[0].reason.contains("kapasite"));
    }

    /// Dolu hücreye ikinci işletme konulamaz.
    #[test]
    fn occupied_cells_are_not_reused() {
        let mut teacher_with_one_slot = teacher(10, 20);
        teacher_with_one_slot.free_slots = slots(&[(1, 9)]);
        teacher_with_one_slot.used_slots = slots(&[(1, 9)]);

        let proposal = propose(&[company(1, 2)], &[teacher_with_one_slot], 17);

        assert!(proposal.assignments.is_empty());
        assert_eq!(proposal.unassigned.len(), 1);
    }

    /// Sınıfın işletme günü tanımlı değilse yerleştirme yapılamaz.
    #[test]
    fn company_without_workplace_days_is_reported() {
        let mut target = company(1, 4);
        target.workplace_days = BTreeSet::new();

        let proposal = propose(&[target], &[teacher(10, 20)], 17);

        assert_eq!(proposal.unassigned.len(), 1);
        assert!(proposal.unassigned[0].reason.contains("işletme günü"));
    }

    #[test]
    fn all_companies_are_reported_when_there_are_no_teachers() {
        let proposal = propose(&[company(1, 4), company(2, 4)], &[], 17);

        assert!(proposal.assignments.is_empty());
        assert_eq!(proposal.unassigned.len(), 2);
        assert!(proposal.unassigned[0].reason.contains("öğretmen yok"));
    }

    /// Dal eşleşmesi olan öğretmen tercih edilir (OÖKY MADDE 88).
    #[test]
    fn exact_branch_match_is_preferred_over_near_field() {
        let mut other_branch = teacher(10, 20);
        other_branch.branches = vec!["Endüstriyel Bakım Onarım".into()];
        let exact = teacher(11, 20);

        let proposal = propose(&[company(1, 4)], &[other_branch, exact], 17);

        assert_eq!(proposal.assignments[0].teacher_id, 11);
        assert!(proposal.assignments[0].exact_branch_match);
    }

    /// Dal eşleşmesi yoksa yakın alan öğretmenine atanır ve işaretlenir.
    #[test]
    fn falls_back_to_near_field_and_marks_it() {
        let mut only_other = teacher(10, 20);
        only_other.branches = vec!["Endüstriyel Bakım Onarım".into()];

        let proposal = propose(&[company(1, 4)], &[only_other], 17);

        assert_eq!(proposal.assignments.len(), 1);
        assert!(!proposal.assignments[0].exact_branch_match);
    }

    /// Fahri ziyaret 0 saat taşır; kapasitesi dolu öğretmene bile verilebilir.
    #[test]
    fn honorary_visit_fits_a_teacher_with_no_remaining_capacity() {
        let mut honorary = company(1, 0);
        honorary.is_honorary = true;

        let mut full = teacher(10, 20);
        full.already_assigned_hours = 20;

        let proposal = propose(&[honorary], &[full], 17);

        assert_eq!(proposal.assignments.len(), 1);
        assert_eq!(proposal.assignments[0].awarded_hours, 0);
    }

    /// Öğretmenin boş saati yoksa yerleştirme yapılamaz.
    #[test]
    fn teacher_without_free_slots_cannot_take_a_company() {
        let mut busy = teacher(10, 20);
        busy.free_slots = BTreeSet::new();

        let proposal = propose(&[company(1, 4)], &[busy], 17);

        assert!(proposal.assignments.is_empty());
        assert!(proposal.unassigned[0].reason.contains("gün/saat"));
    }

    #[test]
    fn hours_by_teacher_sums_per_teacher() {
        let proposal = propose(&[company(1, 4), company(2, 3)], &[teacher(10, 20)], 17);
        assert_eq!(hours_by_teacher(&proposal)[&10], 7);
    }

    /// Ağır işletme önce yerleşir; kapasite yalnızca birine yetiyorsa o kazanır.
    #[test]
    fn heavier_company_is_placed_first() {
        let proposal = propose(&[company(1, 2), company(2, 8)], &[teacher(10, 8)], 17);

        assert_eq!(proposal.assignments.len(), 1);
        assert_eq!(proposal.assignments[0].company_id, 2);
    }
}
