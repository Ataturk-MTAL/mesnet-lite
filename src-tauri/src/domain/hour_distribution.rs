use serde::{Deserialize, Serialize};

/// Otomatik dağıtıma giren tek bir işletme.
#[derive(Debug, Clone)]
pub struct DistributionCandidate {
    pub company_id: i64,
    /// Kural tablosundan gelen tavan. Kural yoksa 0 ve işletme dağıtıma girmez.
    pub max_hours: i64,
    pub student_count: i64,
    /// Kilitli satırlar korunur; havuzdan düşülür ama yeniden hesaplanmaz.
    pub is_locked: bool,
    /// Kilitli satırın hâlihazırdaki takdiri.
    pub current_awarded: i64,
    /// Kullanıcı bu işletmeyi zaten fahri işaretlediyse dağıtıma girmez.
    pub is_honorary: bool,
}

/// Otomatik dağıtımın bir işletme için sonucu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DistributionResult {
    pub company_id: i64,
    pub awarded_hours: i64,
    /// Havuz yetmediği için fahriye bırakıldıysa true.
    pub is_honorary: bool,
    /// Kilitli olduğu için dokunulmadıysa true.
    pub was_locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DistributionOutcome {
    pub results: Vec<DistributionResult>,
    /// Kilitli satırların toplamı.
    pub locked_hours: i64,
    /// Dağıtılan toplam (kilitliler dahil).
    pub distributed_hours: i64,
    /// Havuzdan artan saat.
    pub leftover_hours: i64,
    /// Havuz yetmediği için fahriye bırakılan işletme sayısı.
    pub honorary_count: i64,
    /// Kullanıcıya gösterilecek Türkçe uyarılar.
    pub warnings: Vec<String>,
}

/// Havuz aşım denetiminin TEK doğruluk yeri (kullanıcı kuralı: "havuz
/// aşılamaz"). Hem doğrudan kaydetme yolu (`commands::hours_commands::save_hours`)
/// hem tarihçe kapısı (`domain::history::decide::company::set_company_hours`)
/// BURADAN geçer — ikisi ayrı kopya tutarsa biri unutulur (DRY).
///
/// `pool_hours <= 0` özel durumdur: OÖKY MADDE 88/2-ç projeksiyonu bu dönem
/// için hiç girilmemiş demektir; bu durumda aşım YARGILANAMAZ, sessizce
/// izin verilir (bkz. `HoursBoard`'daki "tanımlanmamış" uyarısı — havuz
/// tanımlanana dek tavan denetimi zaten pasiftir).
///
/// `old_total <= new_total` iken bile, göçten kalma bir veri zaten havuzu
/// aşmışsa (`old_total` de aşkın) ve bu değişiklik aşımı ARTIRMIYORSA
/// (`new_total <= old_total`) kilitlenmez — yalnız aşımı BÜYÜTEN değişiklik
/// reddedilir. Sınıra TAM eşitlemek (`new_total == pool_hours`) kabul edilir.
pub fn pool_overrun_reason(old_total: i64, new_total: i64, pool_hours: i64) -> Option<String> {
    if pool_hours <= 0 || new_total <= pool_hours || new_total <= old_total {
        return None;
    }
    Some(format!(
        "Toplam takdir edilen saat ({new_total}) ders yükü havuzunu ({pool_hours}) {} saat aşıyor.",
        new_total - pool_hours
    ))
}

/// Bir işletmenin dağıtımdaki ağırlığı: tavan × öğrenci sayısı.
///
/// Öğrencisi olmayan işletme de bir ağırlık taşımalı (koordinatör yine gidiyor),
/// bu yüzden öğrenci sayısı en az 1 sayılır.
fn weight(candidate: &DistributionCandidate) -> i64 {
    candidate.max_hours * candidate.student_count.max(1)
}

/// Ders yükü havuzunu işletmelere ağırlığa göre dağıtır.
///
/// Kurallar:
/// - Kilitli satırlara dokunulmaz; havuzdan önce onlar düşülür.
/// - Hiçbir işletme kendi tavanını aşamaz.
/// - Havuz yetmezse EN DÜŞÜK ağırlıklı işletmeler fahriye bırakılır (0 saat).
/// - Artan saat kalırsa tavanına ulaşmamış işletmelere sırayla dağıtılır.
///
/// Sonuç yalnızca öneridir; kaydetmek kullanıcının işidir.
pub fn distribute(candidates: &[DistributionCandidate], pool_hours: i64) -> DistributionOutcome {
    let mut outcome = DistributionOutcome::default();

    let locked_hours: i64 = candidates
        .iter()
        .filter(|c| c.is_locked)
        .map(|c| c.current_awarded)
        .sum();
    outcome.locked_hours = locked_hours;

    // Kilitli satırlar olduğu gibi korunur.
    for candidate in candidates.iter().filter(|c| c.is_locked) {
        outcome.results.push(DistributionResult {
            company_id: candidate.company_id,
            awarded_hours: candidate.current_awarded,
            is_honorary: candidate.is_honorary,
            was_locked: true,
        });
    }

    let mut remaining = pool_hours - locked_hours;

    if remaining < 0 {
        outcome.warnings.push(format!(
            "Kilitli satırların toplamı ders yükü havuzunu {} saat aşıyor. \
             Dağıtılacak saat kalmadı — bazı kilitleri açın ya da havuzu artırın.",
            -remaining
        ));
        remaining = 0;
    }

    // Kullanıcının fahri işaretlediği kilitsiz satırlar 0 saatle geçer.
    for candidate in candidates.iter().filter(|c| !c.is_locked && c.is_honorary) {
        outcome.results.push(DistributionResult {
            company_id: candidate.company_id,
            awarded_hours: 0,
            is_honorary: true,
            was_locked: false,
        });
    }

    // Kural tavanı olmayan işletmeler dağıtıma giremez; 0 saatle bırakılır.
    let without_rule: Vec<&DistributionCandidate> = candidates
        .iter()
        .filter(|c| !c.is_locked && c.max_hours <= 0 && !c.is_honorary)
        .collect();
    if !without_rule.is_empty() {
        outcome.warnings.push(format!(
            "{} işletmenin saat kuralı bulunamadı; 0 saatle bırakıldı.",
            without_rule.len()
        ));
        for candidate in without_rule {
            outcome.results.push(DistributionResult {
                company_id: candidate.company_id,
                awarded_hours: 0,
                is_honorary: false,
                was_locked: false,
            });
        }
    }

    // Dağıtıma girecekler: kilitli olmayan, kural tavanı olan, fahri olmayan.
    let mut open: Vec<&DistributionCandidate> = candidates
        .iter()
        .filter(|c| !c.is_locked && c.max_hours > 0 && !c.is_honorary)
        .collect();

    // Ağırlığı yüksek işletme önce doyar; eşitlikte küçük id (belirlenimcilik).
    open.sort_by(|a, b| weight(b).cmp(&weight(a)).then(a.company_id.cmp(&b.company_id)));

    let total_max: i64 = open.iter().map(|c| c.max_hours).sum();
    let mut awarded: Vec<(i64, i64)> = Vec::with_capacity(open.len());

    if remaining >= total_max {
        // Havuz herkese tavanını verecek kadar büyük.
        for candidate in &open {
            awarded.push((candidate.company_id, candidate.max_hours));
        }
        remaining -= total_max;

        if remaining > 0 {
            outcome.warnings.push(format!(
                "Havuzun {remaining} saati dağıtılamadı: tüm işletmeler mesafe tavanına ulaştı."
            ));
        }
    } else {
        // Havuz yetmiyor: ağırlığa göre orantılı pay, sonra artan saatleri sırayla ver.
        let total_weight: i64 = open.iter().map(|c| weight(c)).sum();

        if total_weight > 0 {
            for candidate in &open {
                let share = remaining * weight(candidate) / total_weight;
                awarded.push((candidate.company_id, share.min(candidate.max_hours)));
            }

            // Tamsayı bölmesinden artan saatler: ağırlık sırasına göre birer birer.
            let mut used: i64 = awarded.iter().map(|(_, hours)| *hours).sum();
            let mut index = 0;
            while used < remaining && index < awarded.len() {
                let cap = open[index].max_hours;
                if awarded[index].1 < cap {
                    awarded[index].1 += 1;
                    used += 1;
                } else {
                    index += 1;
                }
            }
            remaining -= used;
        }
    }

    // Payı 0 çıkanlar fahriye bırakılır: öğretmen gider, ücret doğmaz.
    let mut honorary_count = 0;
    for (company_id, hours) in awarded {
        let is_honorary = hours == 0;
        if is_honorary {
            honorary_count += 1;
        }
        outcome.results.push(DistributionResult {
            company_id,
            awarded_hours: hours,
            is_honorary,
            was_locked: false,
        });
    }

    if honorary_count > 0 {
        outcome.warnings.push(format!(
            "Havuz tüm işletmelere yetmedi: {honorary_count} işletme fahri ziyarete bırakıldı \
             (en düşük ağırlıktan başlanarak). Fahri satırlar 0 saatle kaydedilir."
        ));
    }

    outcome.honorary_count = honorary_count;
    outcome.leftover_hours = remaining.max(0);
    outcome.distributed_hours = outcome.results.iter().map(|r| r.awarded_hours).sum();

    // Sonuç sırası girdi sırasından bağımsız olmalı ki arayüz kararlı görünsün.
    outcome.results.sort_by_key(|r| r.company_id);
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(company_id: i64, max_hours: i64, student_count: i64) -> DistributionCandidate {
        DistributionCandidate {
            company_id,
            max_hours,
            student_count,
            is_locked: false,
            current_awarded: 0,
            is_honorary: false,
        }
    }

    fn hours_for(outcome: &DistributionOutcome, company_id: i64) -> i64 {
        outcome
            .results
            .iter()
            .find(|r| r.company_id == company_id)
            .unwrap()
            .awarded_hours
    }

    /// Havuz bolsa herkes tavanını alır.
    #[test]
    fn everyone_reaches_their_cap_when_pool_is_large() {
        let candidates = vec![candidate(1, 8, 1), candidate(2, 4, 1)];
        let outcome = distribute(&candidates, 100);

        assert_eq!(hours_for(&outcome, 1), 8);
        assert_eq!(hours_for(&outcome, 2), 4);
        assert_eq!(outcome.distributed_hours, 12);
        assert_eq!(outcome.leftover_hours, 88);
        assert!(outcome.warnings.iter().any(|w| w.contains("dağıtılamadı")));
    }

    /// Havuz tam yeterse artan kalmaz ve uyarı çıkmaz.
    #[test]
    fn exact_pool_produces_no_leftover_warning() {
        let candidates = vec![candidate(1, 8, 1), candidate(2, 4, 1)];
        let outcome = distribute(&candidates, 12);

        assert_eq!(outcome.leftover_hours, 0);
        assert!(outcome.warnings.is_empty());
    }

    /// Havuz yetmezse ağırlığa göre paylaşılır: tavan × öğrenci sayısı.
    #[test]
    fn scarce_pool_is_shared_by_weight() {
        // Ağırlıklar: 8×3 = 24 ve 8×1 = 8, toplam 32
        let candidates = vec![candidate(1, 8, 3), candidate(2, 8, 1)];
        let outcome = distribute(&candidates, 8);

        assert_eq!(outcome.distributed_hours, 8);
        assert!(
            hours_for(&outcome, 1) > hours_for(&outcome, 2),
            "çok öğrencili işletme daha fazla almalı"
        );
    }

    /// Hiçbir işletme kendi tavanını aşamaz.
    #[test]
    fn no_company_exceeds_its_own_cap() {
        let candidates = vec![candidate(1, 2, 10), candidate(2, 8, 1)];
        let outcome = distribute(&candidates, 50);

        assert_eq!(hours_for(&outcome, 1), 2);
        assert_eq!(hours_for(&outcome, 2), 8);
    }

    /// Payı sıfır çıkan işletme fahriye bırakılır ve uyarı verilir.
    #[test]
    fn zero_share_companies_become_honorary() {
        let candidates = vec![candidate(1, 8, 20), candidate(2, 8, 1), candidate(3, 8, 1)];
        let outcome = distribute(&candidates, 2);

        assert!(outcome.honorary_count > 0);
        assert!(outcome
            .warnings
            .iter()
            .any(|w| w.contains("fahri ziyarete bırakıldı")));

        for result in &outcome.results {
            if result.awarded_hours == 0 {
                assert!(result.is_honorary, "0 saat alan satır fahri işaretlenmeli");
            }
        }
    }

    /// Kilitli satıra dokunulmaz ve havuzdan düşülür.
    #[test]
    fn locked_rows_are_preserved_and_deducted_from_the_pool() {
        let mut locked = candidate(1, 8, 1);
        locked.is_locked = true;
        locked.current_awarded = 6;

        let outcome = distribute(&[locked, candidate(2, 8, 1)], 10);

        assert_eq!(hours_for(&outcome, 1), 6, "kilitli satır değişmemeli");
        assert_eq!(outcome.locked_hours, 6);
        // Kalan 4 saat ikinci işletmeye gider.
        assert_eq!(hours_for(&outcome, 2), 4);
    }

    /// Kilitliler havuzu aşarsa uyarı verilir ve dağıtılacak saat kalmaz.
    #[test]
    fn locked_rows_exceeding_the_pool_are_reported() {
        let mut locked = candidate(1, 8, 1);
        locked.is_locked = true;
        locked.current_awarded = 15;

        let outcome = distribute(&[locked, candidate(2, 8, 1)], 10);

        assert!(outcome
            .warnings
            .iter()
            .any(|w| w.contains("havuzunu 5 saat aşıyor")));
        assert_eq!(hours_for(&outcome, 2), 0);
    }

    /// Kullanıcının fahri işaretlediği satır dağıtıma girmez.
    #[test]
    fn user_marked_honorary_rows_stay_at_zero() {
        let mut honorary = candidate(1, 8, 5);
        honorary.is_honorary = true;

        let outcome = distribute(&[honorary, candidate(2, 8, 1)], 100);

        assert_eq!(hours_for(&outcome, 1), 0);
        assert_eq!(hours_for(&outcome, 2), 8);
    }

    /// Saat kuralı olmayan işletme 0 saatle bırakılır ve raporlanır.
    #[test]
    fn companies_without_an_hour_rule_are_reported() {
        let outcome = distribute(&[candidate(1, 0, 2), candidate(2, 8, 1)], 100);

        assert_eq!(hours_for(&outcome, 1), 0);
        assert!(outcome
            .warnings
            .iter()
            .any(|w| w.contains("saat kuralı bulunamadı")));
    }

    /// Öğrencisi olmayan işletme de ağırlık taşır; koordinatör yine gidiyor.
    #[test]
    fn company_without_students_still_gets_a_share() {
        let candidates = vec![candidate(1, 8, 0), candidate(2, 8, 1)];
        let outcome = distribute(&candidates, 8);

        assert!(hours_for(&outcome, 1) > 0);
    }

    /// Sonuç girdi sırasından bağımsız olmalı.
    #[test]
    fn result_order_is_stable_regardless_of_input_order() {
        let forward = distribute(&[candidate(1, 8, 1), candidate(2, 8, 1)], 8);
        let reversed = distribute(&[candidate(2, 8, 1), candidate(1, 8, 1)], 8);

        assert_eq!(forward.results, reversed.results);
    }

    #[test]
    fn empty_input_produces_empty_outcome() {
        let outcome = distribute(&[], 20);
        assert!(outcome.results.is_empty());
        assert_eq!(outcome.distributed_hours, 0);
    }

    // --- `pool_overrun_reason`: kullanıcı kuralı "havuz aşılamaz" ---

    /// 90 → 110, havuz 100: aşımı ARTIRAN değişiklik reddedilir.
    #[test]
    fn increasing_the_total_above_the_pool_is_rejected() {
        let reason = pool_overrun_reason(90, 110, 100);
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("110"));
    }

    /// Tam havuza eşitlemek (sınır dahil) kabul edilir.
    #[test]
    fn reaching_the_pool_exactly_is_accepted() {
        assert!(pool_overrun_reason(90, 100, 100).is_none());
    }

    /// Havuz tanımlanmamışsa (`0`) aşım hiç yargılanmaz.
    #[test]
    fn undefined_pool_never_rejects() {
        assert!(pool_overrun_reason(0, 1_000_000, 0).is_none());
    }

    /// Aşımı azaltan bir düzenleme (100 → 80, havuz 50) kabul edilir.
    #[test]
    fn decreasing_the_overrun_is_accepted() {
        assert!(pool_overrun_reason(100, 80, 50).is_none());
    }

    /// Göçten kalma veri zaten aşkınken (60 > 50), aşımı ARTIRMAYAN bir
    /// düzenleme (60 → 60 ya da altı) kilitlenmez.
    #[test]
    fn a_pre_existing_overrun_does_not_lock_out_unrelated_edits() {
        assert!(pool_overrun_reason(60, 60, 50).is_none(), "aşım büyümüyorsa reddedilmemeli");
        assert!(pool_overrun_reason(60, 55, 50).is_none(), "aşımı azaltan düzenleme serbest olmalı");
    }

    /// Aynı göçten kalma durumda aşımı DAHA DA BÜYÜTEN bir düzenleme reddedilir.
    #[test]
    fn a_pre_existing_overrun_still_rejects_a_further_increase() {
        assert!(pool_overrun_reason(60, 65, 50).is_some());
    }
}
