//! Boyut ve sayaç sınırı testleri. Sentetik veri kurguseldir; burada üretilen
//! sözde-rastgele sayılar yalnız GİRDİYİ çeşitlendirir, motor rastgelelik
//! kullanmaz.

use super::*;

/// Doğrusal eş-merkezli üreteç: sabit çekirdek → sabit girdi.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: u64) -> i64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) % bound) as i64
    }
}

fn synthetic(company_count: i64, teacher_count: i64) -> Scenario {
    let mut rng = Lcg(43);
    let companies = (1..=company_count)
        .map(|id| {
            let hours = 1 + rng.next(8);
            CompanyInput {
                awarded_hours: hours,
                max_hours: hours,
                workplace_days: days(&[1 + rng.next(5), 1 + rng.next(5)]),
                group_key: Some(format!("g{}", rng.next(6))),
                ..company(id)
            }
        })
        .collect();
    let teachers = (1..=teacher_count)
        .map(|id| TeacherInput {
            capacity: 16,
            free_slots: free_on(&[1, 2, 3, 4, 5], 9..(12 + rng.next(5))),
            ..teacher(id)
        })
        .collect();
    Scenario::new(companies, teachers)
}

fn assert_consistent(proposal: &AllocationProposal, company_count: usize) {
    let accounted = proposal.assignments.len() + proposal.kept.len() + proposal.unassigned.len();
    assert_eq!(
        accounted, company_count,
        "her işletme tam bir listede olmalı"
    );
}

#[test]
fn medium_instance_terminates_and_accounts_for_every_company() {
    let proposal = synthetic(60, 20).run();
    assert_consistent(&proposal, 60);
}

/// 300 × 100: sayaç sınırında biter (debug derlemede ölçülen süre ≈ 0,4 sn,
/// bu yüzden `#[ignore]` gerekmedi) ve sınır uyarı olarak bildirilir.
#[test]
fn large_instance_stops_at_the_evaluation_cap() {
    let proposal = synthetic(300, 100).run();
    assert_consistent(&proposal, 300);
    assert!(
        proposal
            .warnings
            .iter()
            .any(|w| w.contains("Arama sınırına")),
        "sınıra ulaşıldığı bildirilmeli: {:?}",
        proposal.warnings
    );
}

/// Gerçek boyutta (28 × 14) arama sınıra takılmadan yakınsar.
#[test]
fn real_size_instance_converges_without_hitting_a_limit() {
    let proposal = synthetic(28, 14).run();
    assert_consistent(&proposal, 28);
    assert!(proposal.warnings.is_empty(), "{:?}", proposal.warnings);
}
