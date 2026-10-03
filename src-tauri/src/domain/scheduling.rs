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

/// Bir atamanın kapladığı hücre sayısı. Fahri ziyaret (`awarded_hours = 0`)
/// bile TAM olarak 1 hücre kaplar; ekranda gösterecek bir yeri olmalı.
pub fn visit_span(awarded_hours: i64) -> i64 {
    awarded_hours.max(1)
}

/// Bir atamanın ardışık hücrelerden oluşan BLOĞU.
///
/// Kapsam `start_hour` ile `start_hour + span - 1` arasıdır, her iki uç
/// DAHİL. Eskiden bir işletme tek bir hücre kaplardı; artık takdir edilen
/// saat kadar ardışık hücre kaplıyor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Block {
    pub day_of_week: i64,
    pub start_hour: i64,
    /// DAHİL üst sınır.
    pub end_hour: i64,
}

impl Block {
    pub fn from_start(day_of_week: i64, start_hour: i64, awarded_hours: i64) -> Self {
        Self {
            day_of_week,
            start_hour,
            end_hour: start_hour + visit_span(awarded_hours) - 1,
        }
    }

    /// Bloğun kapladığı tüm hücreler.
    pub fn cells(&self) -> BTreeSet<Slot> {
        (self.start_hour..=self.end_hour)
            .map(|hour| Slot::new(self.day_of_week, hour))
            .collect()
    }

    /// İki blok aynı öğretmende aynı anda duramaz mı? Yalnızca aynı günde ve
    /// hücre aralıkları kesişiyorsa true döner.
    pub fn overlaps(&self, other: &Block) -> bool {
        self.day_of_week == other.day_of_week
            && self.start_hour <= other.end_hour
            && other.start_hour <= self.end_hour
    }

    /// Izgaranın `day_end_hour` (HARİÇ) sınırını aşıyor mu?
    pub fn exceeds_day_end(&self, day_end_hour: i64) -> bool {
        self.end_hour >= day_end_hour
    }
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

    // --- visit_span ---

    #[test]
    fn honorary_visit_span_is_exactly_one_hour() {
        assert_eq!(visit_span(0), 1);
    }

    #[test]
    fn span_matches_awarded_hours_when_positive() {
        assert_eq!(visit_span(6), 6);
    }

    // --- Block ---

    #[test]
    fn block_end_hour_covers_the_full_span() {
        let block = Block::from_start(2, 9, 6);
        assert_eq!(block.start_hour, 9);
        assert_eq!(block.end_hour, 14, "9,10,11,12,13,14 = 6 hücre");
    }

    #[test]
    fn honorary_block_occupies_a_single_cell() {
        let block = Block::from_start(1, 9, 0);
        assert_eq!(block.end_hour, 9);
        assert_eq!(block.cells(), slots(&[(1, 9)]));
    }

    #[test]
    fn block_cells_lists_every_hour_in_the_span() {
        let block = Block::from_start(3, 10, 3);
        assert_eq!(block.cells(), slots(&[(3, 10), (3, 11), (3, 12)]));
    }

    #[test]
    fn overlapping_blocks_on_the_same_day_are_detected() {
        let a = Block::from_start(1, 9, 4); // 9-12
        let b = Block::from_start(1, 12, 2); // 12-13, paylaşılan hücre 12
        assert!(a.overlaps(&b));
    }

    #[test]
    fn adjacent_blocks_that_do_not_share_a_cell_do_not_overlap() {
        let a = Block::from_start(1, 9, 4); // 9-12
        let b = Block::from_start(1, 13, 2); // 13-14
        assert!(!a.overlaps(&b));
    }

    #[test]
    fn blocks_on_different_days_never_overlap_even_with_the_same_hours() {
        let a = Block::from_start(1, 9, 4);
        let b = Block::from_start(2, 9, 4);
        assert!(!a.overlaps(&b));
    }

    #[test]
    fn block_within_grid_does_not_exceed_day_end() {
        let block = Block::from_start(1, 9, 8); // 9-16
        assert!(!block.exceeds_day_end(17));
    }

    #[test]
    fn block_reaching_the_grid_boundary_exceeds_day_end() {
        let block = Block::from_start(1, 9, 9); // 9-17, 17 hariç sınırını yiyor
        assert!(block.exceeds_day_end(17));
    }
}
