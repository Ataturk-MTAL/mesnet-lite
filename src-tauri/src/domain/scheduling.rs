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

/// Birden çok bloğun birlikte kapladığı hücreler — bir öğretmenin dolu
/// hücre haritası.
pub fn occupied_cells(blocks: &[Block]) -> BTreeSet<Slot> {
    blocks.iter().flat_map(Block::cells).collect()
}

/// Bir işletme için ardışık boş hücrelerden oluşan bir BLOK arar.
///
/// Eskiden tek bir boş hücre yeterliydi; artık `awarded_hours` kadar ARDIŞIK
/// hücrenin hepsi boş olmalı, hiçbiri başka bir bloğa ait olmamalı ve blok
/// ızgaranın gün sonunu aşmamalı.
///
/// `eligible`: öğretmenin boş VE işletmenin gün kısıtına uyan tekil hücreler.
/// `occupied`: öğretmenin başka atamalarının bloklarıyla dolu hücreleri.
/// `day_end_hour`: ızgaranın bitişi, HARİÇ (ayarlardan).
pub fn pick_visit_block(
    eligible: &BTreeSet<Slot>,
    occupied: &BTreeSet<Slot>,
    hours_by_day: &BTreeMap<i64, i64>,
    awarded_hours: i64,
    day_end_hour: i64,
) -> Option<Block> {
    eligible
        .iter()
        .filter(|slot| !occupied.contains(slot))
        .filter_map(|slot| {
            let block = Block::from_start(slot.day_of_week, slot.hour, awarded_hours);
            if block.exceeds_day_end(day_end_hour) {
                return None;
            }

            let cells = block.cells();
            let fits = cells
                .iter()
                .all(|cell| eligible.contains(cell) && !occupied.contains(cell));
            if !fits {
                return None;
            }

            let current = hours_by_day.get(&slot.day_of_week).copied().unwrap_or(0);
            if current + awarded_hours > MAX_HOURS_PER_DAY {
                return None;
            }

            Some(block)
        })
        .min_by_key(|block| {
            let load = hours_by_day.get(&block.day_of_week).copied().unwrap_or(0);
            (load, block.day_of_week, block.start_hour)
        })
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

    fn eligible_range(day: i64, hours: std::ops::Range<i64>) -> BTreeSet<Slot> {
        hours.map(|h| Slot::new(day, h)).collect()
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

    #[test]
    fn occupied_cells_unions_every_block() {
        let blocks = vec![Block::from_start(1, 9, 2), Block::from_start(2, 13, 3)];
        assert_eq!(
            occupied_cells(&blocks),
            slots(&[(1, 9), (1, 10), (2, 13), (2, 14), (2, 15)])
        );
    }

    // --- pick_visit_block ---

    #[test]
    fn picks_a_block_that_fits_entirely_in_free_hours() {
        let eligible = eligible_range(1, 8..17);
        let block =
            pick_visit_block(&eligible, &BTreeSet::new(), &BTreeMap::new(), 6, 17).unwrap();
        assert_eq!(block, Block::from_start(1, 8, 6));
    }

    /// Yalnızca 2 saat boş; 6 saatlik blok sığmaz.
    #[test]
    fn rejects_a_start_when_the_span_is_not_fully_free() {
        let eligible = slots(&[(1, 9), (1, 10)]);
        assert_eq!(
            pick_visit_block(&eligible, &BTreeSet::new(), &BTreeMap::new(), 6, 17),
            None
        );
    }

    /// 12. saat dolu olduğu için 6 saatlik bloğun HER olası başlangıcı çakışır.
    #[test]
    fn rejects_a_block_that_would_overlap_another_assignment() {
        let eligible = eligible_range(1, 8..17);
        let occupied = slots(&[(1, 12)]);
        assert_eq!(
            pick_visit_block(&eligible, &occupied, &BTreeMap::new(), 6, 17),
            None
        );
    }

    /// İlk 4 saat dolu; 3 saatlik blok ancak 12'den sonra sığar.
    #[test]
    fn moves_to_the_next_valid_start_when_earlier_ones_overlap() {
        let eligible = eligible_range(1, 8..17);
        let occupied = eligible_range(1, 8..12);
        let block = pick_visit_block(&eligible, &occupied, &BTreeMap::new(), 3, 17).unwrap();
        assert_eq!(block.start_hour, 12);
    }

    /// 8. saat dolu; 9'dan başlayan 8 saatlik blok 9-16 olurdu ve hücrelerin
    /// hepsi boş olsa bile ızgara 16'da (HARİÇ) bittiği için sığmaz.
    #[test]
    fn rejects_a_block_that_would_exceed_the_grids_day_end() {
        let eligible = eligible_range(1, 8..17); // 8..16 arası boş
        let occupied = slots(&[(1, 8)]);
        assert_eq!(
            pick_visit_block(&eligible, &occupied, &BTreeMap::new(), 8, 16),
            None
        );
    }

    /// 8'den başlayan blok (8-15) ızgara sınırının içinde kaldığı için sığar.
    #[test]
    fn accepts_a_block_that_ends_exactly_before_the_grids_day_end() {
        let eligible = eligible_range(1, 8..17);
        let block =
            pick_visit_block(&eligible, &BTreeSet::new(), &BTreeMap::new(), 8, 16).unwrap();
        assert_eq!(block.start_hour, 8);
        assert_eq!(block.end_hour, 15);
    }

    #[test]
    fn daily_cap_still_applies_to_the_whole_block() {
        let eligible = eligible_range(1, 8..17);
        let hours_by_day = BTreeMap::from([(1, 6)]);
        // 6 + 4 = 10 > 8, günlük sınırı aşar.
        assert_eq!(
            pick_visit_block(&eligible, &BTreeSet::new(), &hours_by_day, 4, 17),
            None
        );
    }

    #[test]
    fn honorary_block_fits_a_single_free_cell_even_on_a_full_day() {
        let eligible = slots(&[(1, 9)]);
        let hours_by_day = BTreeMap::from([(1, 8)]);
        let block = pick_visit_block(&eligible, &BTreeSet::new(), &hours_by_day, 0, 17).unwrap();
        assert_eq!(block, Block::from_start(1, 9, 0));
    }

    #[test]
    fn prefers_the_least_loaded_day_for_the_whole_block() {
        let eligible: BTreeSet<Slot> = eligible_range(1, 8..17)
            .into_iter()
            .chain(eligible_range(2, 8..17))
            .collect();
        let hours_by_day = BTreeMap::from([(1, 6)]);
        let block = pick_visit_block(&eligible, &BTreeSet::new(), &hours_by_day, 2, 17).unwrap();
        assert_eq!(block.day_of_week, 2, "hiç yükü olmayan gün önce");
    }

    #[test]
    fn pick_visit_block_returns_none_when_nothing_is_eligible() {
        assert_eq!(
            pick_visit_block(&BTreeSet::new(), &BTreeSet::new(), &BTreeMap::new(), 2, 17),
            None
        );
    }

    /// Günlük sınırı aşacak gün elenir; blok başka güne düşer.
    ///
    /// Eskiden `pick_visit_slot` için yazılmıştı — kural (günlük sınırı aşan
    /// günün elenip başka güne düşülmesi) blok modelinde de geçerli, o yüzden
    /// silinmedi, `pick_visit_block` üzerinden uyarlandı.
    #[test]
    fn rejects_a_day_that_would_exceed_the_daily_cap_and_falls_back_to_another_day() {
        let eligible: BTreeSet<Slot> = eligible_range(1, 9..13)
            .into_iter()
            .chain(eligible_range(2, 9..13))
            .collect();
        // 1. gün zaten 6 saat dolu; 4 saatlik blok oraya sığmaz, 2. güne düşer.
        let hours_by_day = BTreeMap::from([(1, 6)]);
        let block = pick_visit_block(&eligible, &BTreeSet::new(), &hours_by_day, 4, 17).unwrap();
        assert_eq!(block.day_of_week, 2);
        assert_eq!(block.start_hour, 9);
    }

    /// Hiçbir gün sığdıramıyorsa None döner; zorlama kullanıcının kararıdır.
    ///
    /// Eskiden `pick_visit_slot` için yazılmıştı — kural (tüm günler günlük
    /// sınırı aşınca None dönmesi) blok modelinde de geçerli, o yüzden
    /// silinmedi, `pick_visit_block` üzerinden uyarlandı.
    #[test]
    fn returns_none_when_no_day_can_absorb_the_hours() {
        let eligible = slots(&[(1, 9), (2, 9)]);
        let hours_by_day = BTreeMap::from([(1, 8), (2, 8)]);
        assert_eq!(
            pick_visit_block(&eligible, &BTreeSet::new(), &hours_by_day, 1, 17),
            None
        );
    }
}
