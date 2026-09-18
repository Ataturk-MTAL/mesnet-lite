use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// OÖKY MADDE 88: "bir öğretmene aynı gün için 8 saatten fazla ek ders görevi verilmez"
pub const MAX_HOURS_PER_DAY: i64 = 8;

/// Haftalık tek bir ders saati hücresi. Gün 1 = Pazartesi … 5 = Cuma.
///
/// Bir işletme TEK bir hücreye yerleşir; taşıdığı ek ders saati ayrı bir
/// büyüklüktür (`company_term_hours.awarded_hours`). Ziyaretin yeri ile
/// tahakkuk eden saat aynı şey değildir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    pub day_of_week: i64,
    pub hour: i64,
}

impl Slot {
    pub fn new(day_of_week: i64, hour: i64) -> Self {
        Self { day_of_week, hour }
    }
}

/// Bir işletmenin yerleştirilebileceği hücreler.
///
/// İki kümenin kesişimidir:
/// 1. Öğretmenin boş saatleri
/// 2. İşletmedeki öğrencilerin sınıflarının işletmede bulunduğu günler
///
/// Kesişim boşsa öğretmen o işletmeyi ziyaret edemez; kullanıcı ya başka
/// öğretmen seçer ya da gerekçeyle zorlar.
pub fn eligible_slots(
    teacher_free_slots: &BTreeSet<Slot>,
    workplace_days: &BTreeSet<i64>,
) -> BTreeSet<Slot> {
    teacher_free_slots
        .iter()
        .copied()
        .filter(|slot| workplace_days.contains(&slot.day_of_week))
        .collect()
}

/// Günlük 8 saat sınırını aşan günler (OÖKY MADDE 88).
///
/// Girdi, gün → o güne düşen toplam EK DERS SAATİ haritasıdır; hücre sayısı
/// değil. Tek bir hücrede 8 saatlik bir işletme durabilir.
pub fn days_over_daily_cap(hours_by_day: &BTreeMap<i64, i64>) -> Vec<i64> {
    hours_by_day
        .iter()
        .filter(|(_, hours)| **hours > MAX_HOURS_PER_DAY)
        .map(|(day, _)| *day)
        .collect()
}

/// Bir işletme için tek bir ziyaret hücresi seçer.
///
/// Kurallar:
/// - Hücre uygun olmalı (öğretmen boş + öğrenciler o gün işletmede)
/// - Hücre başka bir işletmeye verilmemiş olmalı
/// - O güne eklenecek saat günlük 8 saat sınırını aşmamalı
///
/// Yükü en az olan gün tercih edilir; böylece saatler haftaya yayılır ve
/// günlük sınıra çarpma olasılığı düşer. Eşitlikte haftanın erken günü ve
/// erken ders saati seçilir — sonuç belirlenimcidir.
pub fn pick_visit_slot(
    eligible: &BTreeSet<Slot>,
    used_slots: &BTreeSet<Slot>,
    hours_by_day: &BTreeMap<i64, i64>,
    awarded_hours: i64,
) -> Option<Slot> {
    eligible
        .iter()
        .filter(|slot| !used_slots.contains(slot))
        .filter(|slot| {
            let current = hours_by_day.get(&slot.day_of_week).copied().unwrap_or(0);
            current + awarded_hours <= MAX_HOURS_PER_DAY
        })
        .min_by_key(|slot| {
            let load = hours_by_day.get(&slot.day_of_week).copied().unwrap_or(0);
            (load, slot.day_of_week, slot.hour)
        })
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(pairs: &[(i64, i64)]) -> BTreeSet<Slot> {
        pairs.iter().map(|(d, h)| Slot::new(*d, *h)).collect()
    }

    fn load(pairs: &[(i64, i64)]) -> BTreeMap<i64, i64> {
        pairs.iter().copied().collect()
    }

    #[test]
    fn eligible_slots_is_intersection_of_availability_and_workplace_days() {
        let free = slots(&[(1, 9), (1, 10), (2, 9), (3, 14)]);
        let workplace_days = BTreeSet::from([1, 3]);

        assert_eq!(
            eligible_slots(&free, &workplace_days),
            slots(&[(1, 9), (1, 10), (3, 14)])
        );
    }

    /// Öğretmen boşsa ama sınıf o gün işletmede değilse ziyaret edilemez.
    #[test]
    fn eligible_slots_is_empty_when_days_do_not_overlap() {
        let free = slots(&[(1, 9), (2, 10)]);
        assert!(eligible_slots(&free, &BTreeSet::from([4, 5])).is_empty());
    }

    /// Günlük sınır SAAT toplamına bakar, hücre sayısına değil.
    #[test]
    fn daily_cap_counts_awarded_hours_not_cells() {
        // Tek hücrede 9 saatlik bir işletme sınırı aşar.
        assert_eq!(days_over_daily_cap(&load(&[(1, 9)])), vec![1]);
        // 7 saat aşmaz.
        assert!(days_over_daily_cap(&load(&[(2, 7)])).is_empty());
    }

    #[test]
    fn exactly_eight_hours_is_allowed() {
        assert!(days_over_daily_cap(&load(&[(1, MAX_HOURS_PER_DAY)])).is_empty());
    }

    #[test]
    fn picks_a_slot_when_the_day_has_room() {
        let eligible = slots(&[(1, 9), (1, 10)]);
        let picked = pick_visit_slot(&eligible, &BTreeSet::new(), &BTreeMap::new(), 6);

        assert_eq!(picked, Some(Slot::new(1, 9)), "erken saat tercih edilmeli");
    }

    /// Başka işletmeye verilmiş hücre yeniden kullanılamaz.
    #[test]
    fn skips_slots_already_used_by_another_company() {
        let eligible = slots(&[(1, 9), (1, 10)]);
        let used = slots(&[(1, 9)]);

        assert_eq!(
            pick_visit_slot(&eligible, &used, &BTreeMap::new(), 4),
            Some(Slot::new(1, 10))
        );
    }

    /// Günlük sınırı aşacak gün elenir.
    #[test]
    fn rejects_a_day_that_would_exceed_the_daily_cap() {
        let eligible = slots(&[(1, 9), (2, 9)]);
        // 1. gün zaten 6 saat dolu; 4 saatlik işletme oraya sığmaz.
        let picked = pick_visit_slot(&eligible, &BTreeSet::new(), &load(&[(1, 6)]), 4);

        assert_eq!(picked, Some(Slot::new(2, 9)));
    }

    /// Hiçbir gün sığdıramıyorsa None döner; zorlama kullanıcının kararıdır.
    #[test]
    fn returns_none_when_no_day_can_absorb_the_hours() {
        let eligible = slots(&[(1, 9), (2, 9)]);
        let picked = pick_visit_slot(&eligible, &BTreeSet::new(), &load(&[(1, 8), (2, 8)]), 1);

        assert_eq!(picked, None);
    }

    /// Yükü en az olan gün tercih edilir; saatler haftaya yayılır.
    #[test]
    fn prefers_the_least_loaded_day() {
        let eligible = slots(&[(1, 9), (2, 9), (3, 9)]);
        let picked = pick_visit_slot(&eligible, &BTreeSet::new(), &load(&[(1, 6), (2, 2)]), 2);

        assert_eq!(picked, Some(Slot::new(3, 9)), "hiç yükü olmayan gün önce");
    }

    /// Fahri ziyaret 0 saat taşır; dolu bir güne bile yerleşebilir.
    #[test]
    fn honorary_visit_fits_even_on_a_full_day() {
        let eligible = slots(&[(1, 9)]);
        let picked = pick_visit_slot(&eligible, &BTreeSet::new(), &load(&[(1, 8)]), 0);

        assert_eq!(picked, Some(Slot::new(1, 9)));
    }

    #[test]
    fn returns_none_when_nothing_is_eligible() {
        assert_eq!(
            pick_visit_slot(&BTreeSet::new(), &BTreeSet::new(), &BTreeMap::new(), 2),
            None
        );
    }
}
