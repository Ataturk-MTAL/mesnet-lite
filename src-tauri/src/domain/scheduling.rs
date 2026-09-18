use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// OÖKY MADDE 88: "bir öğretmene aynı gün için 8 saatten fazla ek ders görevi verilmez"
pub const MAX_HOURS_PER_DAY: i64 = 8;

/// Haftalık tek bir saat dilimi. Gün 1 = Pazartesi … 5 = Cuma.
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

/// Bir atamanın yerleştirilebileceği dilimler.
///
/// İki kümenin kesişimidir:
/// 1. Öğretmenin boş saatleri
/// 2. İşletmedeki öğrencilerin sınıflarının işletmede bulunduğu günler
///
/// Kesişim boşsa öğretmen o işletmeyi ziyaret edemez; kullanıcı ya başka
/// öğretmen seçer ya da "zorla ekle" ile kural dışına çıkar.
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

/// Aynı gündeki dilimleri sayar.
pub fn hours_on_day(slots: &BTreeSet<Slot>, day_of_week: i64) -> i64 {
    slots.iter().filter(|s| s.day_of_week == day_of_week).count() as i64
}

/// Günlük 8 saat sınırını aşan günler (OÖKY MADDE 88).
pub fn days_over_daily_cap(slots: &BTreeSet<Slot>) -> Vec<i64> {
    let days: BTreeSet<i64> = slots.iter().map(|s| s.day_of_week).collect();
    days.into_iter()
        .filter(|day| hours_on_day(slots, *day) > MAX_HOURS_PER_DAY)
        .collect()
}

/// Uygun dilimler arasından istenen sayıda dilim seçer.
///
/// Aynı güne bitişik saatler tercih edilir: koordinatör tek gidişte birden çok
/// saat harcayabilsin diye. Günlük 8 saat sınırı aşılmaz.
///
/// `already_used`, öğretmenin başka işletmelerde kullandığı dilimlerdir; aynı
/// dilim iki işletmeye verilemez ve günlük sayım bu dilimleri de kapsar.
pub fn pick_slots(
    eligible: &BTreeSet<Slot>,
    already_used: &BTreeSet<Slot>,
    wanted_hours: i64,
) -> Vec<Slot> {
    if wanted_hours <= 0 {
        return Vec::new();
    }

    let free: Vec<Slot> = eligible.difference(already_used).copied().collect();
    let days: BTreeSet<i64> = free.iter().map(|s| s.day_of_week).collect();

    // Günleri en uzun bitişik bloğa göre sırala: önce çok saat veren gün.
    let mut day_order: Vec<i64> = days.into_iter().collect();
    day_order.sort_by_key(|day| {
        let count = free.iter().filter(|s| s.day_of_week == *day).count() as i64;
        // Çoktan aza; eşitlikte haftanın erken günü.
        (-count, *day)
    });

    let mut picked: Vec<Slot> = Vec::new();
    let mut used_per_day: std::collections::BTreeMap<i64, i64> = already_used
        .iter()
        .fold(std::collections::BTreeMap::new(), |mut acc, slot| {
            *acc.entry(slot.day_of_week).or_insert(0) += 1;
            acc
        });

    for day in day_order {
        if picked.len() as i64 >= wanted_hours {
            break;
        }

        let mut hours_today: Vec<i64> = free
            .iter()
            .filter(|s| s.day_of_week == day)
            .map(|s| s.hour)
            .collect();
        hours_today.sort_unstable();

        for hour in hours_today {
            if picked.len() as i64 >= wanted_hours {
                break;
            }
            let on_day = used_per_day.entry(day).or_insert(0);
            if *on_day >= MAX_HOURS_PER_DAY {
                break;
            }
            picked.push(Slot::new(day, hour));
            *on_day += 1;
        }
    }

    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(pairs: &[(i64, i64)]) -> BTreeSet<Slot> {
        pairs.iter().map(|(d, h)| Slot::new(*d, *h)).collect()
    }

    #[test]
    fn eligible_slots_is_intersection_of_availability_and_workplace_days() {
        let free = slots(&[(1, 9), (1, 10), (2, 9), (3, 14)]);
        let workplace_days = BTreeSet::from([1, 3]);

        let eligible = eligible_slots(&free, &workplace_days);

        assert_eq!(eligible, slots(&[(1, 9), (1, 10), (3, 14)]));
    }

    /// Öğretmen boşsa ama sınıf o gün işletmede değilse ziyaret edilemez.
    #[test]
    fn eligible_slots_is_empty_when_days_do_not_overlap() {
        let free = slots(&[(1, 9), (2, 10)]);
        let workplace_days = BTreeSet::from([4, 5]);

        assert!(eligible_slots(&free, &workplace_days).is_empty());
    }

    #[test]
    fn days_over_daily_cap_flags_only_days_above_eight_hours() {
        let nine_hours: Vec<(i64, i64)> = (8..17).map(|h| (1, h)).collect();
        let mut all = slots(&nine_hours);
        all.extend(slots(&[(2, 9), (2, 10)]));

        assert_eq!(days_over_daily_cap(&all), vec![1]);
    }

    #[test]
    fn exactly_eight_hours_is_allowed() {
        let eight_hours: Vec<(i64, i64)> = (8..16).map(|h| (1, h)).collect();
        assert!(days_over_daily_cap(&slots(&eight_hours)).is_empty());
    }

    #[test]
    fn pick_slots_returns_requested_count() {
        let eligible = slots(&[(1, 9), (1, 10), (1, 11), (3, 14)]);
        let picked = pick_slots(&eligible, &BTreeSet::new(), 2);

        assert_eq!(picked.len(), 2);
    }

    /// Aynı güne bitişik saatler tercih edilmeli: tek gidişte çok saat.
    #[test]
    fn pick_slots_prefers_the_day_with_most_free_hours() {
        let eligible = slots(&[(1, 9), (3, 13), (3, 14), (3, 15)]);
        let picked = pick_slots(&eligible, &BTreeSet::new(), 3);

        assert_eq!(picked.len(), 3);
        assert!(
            picked.iter().all(|s| s.day_of_week == 3),
            "en çok boş saati olan gün seçilmeliydi: {picked:?}"
        );
    }

    /// Başka işletmeye verilmiş dilim yeniden kullanılamaz.
    #[test]
    fn pick_slots_skips_already_used_slots() {
        let eligible = slots(&[(1, 9), (1, 10)]);
        let used = slots(&[(1, 9)]);

        let picked = pick_slots(&eligible, &used, 2);

        assert_eq!(picked, vec![Slot::new(1, 10)], "yalnızca boş dilim kalmıştı");
    }

    /// Günlük 8 saat sınırı aşılmamalı; talep karşılanamazsa eksik döner.
    #[test]
    fn pick_slots_respects_the_daily_cap() {
        let ten_hours: Vec<(i64, i64)> = (8..18).map(|h| (1, h)).collect();
        let picked = pick_slots(&slots(&ten_hours), &BTreeSet::new(), 10);

        assert_eq!(picked.len(), MAX_HOURS_PER_DAY as usize);
    }

    /// Öğretmenin o gün başka işletmede kullandığı saatler de sınıra dahildir.
    #[test]
    fn daily_cap_counts_slots_already_used_on_that_day() {
        let eligible = slots(&[(1, 16), (1, 17)]);
        let used: Vec<(i64, i64)> = (8..16).map(|h| (1, h)).collect();

        let picked = pick_slots(&eligible, &slots(&used), 2);

        assert!(picked.is_empty(), "gün zaten 8 saatle dolmuştu");
    }

    #[test]
    fn pick_slots_returns_empty_for_non_positive_request() {
        let eligible = slots(&[(1, 9)]);
        assert!(pick_slots(&eligible, &BTreeSet::new(), 0).is_empty());
        assert!(pick_slots(&eligible, &BTreeSet::new(), -3).is_empty());
    }

    /// Yeterli dilim yoksa bulunabildiği kadarı döner; sessizce uydurulmaz.
    #[test]
    fn pick_slots_returns_fewer_when_not_enough_eligible() {
        let eligible = slots(&[(1, 9)]);
        assert_eq!(pick_slots(&eligible, &BTreeSet::new(), 5).len(), 1);
    }
}
