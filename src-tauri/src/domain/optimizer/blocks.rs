//! Bir (işletme, öğretmen, saat) için uygun blok arama.
//!
//! Uygunluk kuralları: işletme tavanı, öğretmen kapasitesi (MADDE 15/2),
//! havuz bütçesi, günlük 8 saat (OÖKY MADDE 88), boş hücre ∩ işletme günleri
//! ve ardışık bloğun hücrelerinin boşluğu. Fahri ziyaret (0 saat) kapasite,
//! havuz ve günlük sınırdan muaftır; yalnız 1 hücre ister.

use super::problem::DAYS;
use super::state::{day_index, State};
use crate::domain::scheduling::{visit_span, MAX_HOURS_PER_DAY};

/// (gün, başlangıç saati)
pub(super) type BlockPos = (i64, i64);

impl State<'_, '_> {
    /// Bir işletmeye verilebilecek en yüksek saat (öğretmenden bağımsız üst sınır).
    pub fn max_hours_for(&self, c: usize) -> i64 {
        let budget = if self.p.spends_budget(c) {
            self.budget_left()
        } else {
            i64::MAX
        };
        self.p.ceiling[c].min(budget).min(MAX_HOURS_PER_DAY)
    }

    pub fn day_load(&self, t: usize, day: i64) -> i64 {
        day_index(day).map_or(0, |index| self.teachers[t].day_hours[index])
    }

    /// Tavan, kapasite ve havuz bu saate izin veriyor mu?
    pub fn hours_allowed(&self, t: usize, c: usize, hours: i64) -> bool {
        if hours > self.p.ceiling[c] || hours < self.p.floor[c] {
            return false;
        }
        let within_budget = !self.p.spends_budget(c) || hours <= self.budget_left();
        hours == 0 || (hours <= self.teachers[t].rem_cap && within_budget)
    }

    /// Blok hücreleri öğretmen için boş mu ve işletmenin günlerine düşüyor mu?
    pub fn cells_free(&self, t: usize, c: usize, day: i64, start: i64, span: i64) -> bool {
        let Some(mask) = self.p.block_mask(day, start, span) else {
            return false;
        };
        mask & self.p.eligible(t, c) == mask && mask & self.teachers[t].occ == 0
    }

    pub fn daily_cap_allows(&self, t: usize, day: i64, hours: i64) -> bool {
        hours == 0 || self.day_load(t, day) + hours <= MAX_HOURS_PER_DAY
    }

    /// Belirli bir (gün, başlangıç) için tüm kurallar sağlanıyor mu?
    pub fn fits_at(&self, t: usize, c: usize, pos: BlockPos, hours: i64) -> bool {
        self.hours_allowed(t, c, hours)
            && self.daily_cap_allows(t, pos.0, hours)
            && self.cells_free(t, c, pos.0, pos.1, visit_span(hours))
    }

    /// Uygun blokların en iyisi; yoksa `None`.
    ///
    /// Saatli blok: en az yüklü gün, sonra parçalanmayan (kenara ya da dolu
    /// hücreye bitişik) blok. Fahri blok tersine en yüklü güne, boşlukları
    /// bölmeden yerleşir; boş günleri tam saatli işletmelere bırakır.
    /// `prefer` aynı öğretmende mevcut bloğu öne alır (gereksiz taşıma yok).
    pub fn best_block(
        &mut self,
        t: usize,
        c: usize,
        hours: i64,
        prefer: Option<BlockPos>,
    ) -> Option<BlockPos> {
        self.evals += 1;
        if !self.hours_allowed(t, c, hours) {
            return None;
        }
        let span = visit_span(hours);
        let mut best: Option<([i64; 5], BlockPos)> = None;
        for day in 1..=DAYS as i64 {
            if !self.daily_cap_allows(t, day, hours) {
                continue;
            }
            for start in self.p.day_start..=(self.p.day_end - span) {
                if !self.cells_free(t, c, day, start, span) {
                    continue;
                }
                let key = self.block_key(t, (day, start), span, hours, prefer);
                if best.is_none_or(|(known, _)| key < known) {
                    best = Some((key, (day, start)));
                }
            }
        }
        best.map(|(_, pos)| pos)
    }

    fn block_key(
        &self,
        t: usize,
        pos: BlockPos,
        span: i64,
        hours: i64,
        prefer: Option<BlockPos>,
    ) -> [i64; 5] {
        let (day, start) = pos;
        let rank = i64::from(prefer != Some(pos));
        let load = self.day_load(t, day);
        let fragmented = i64::from(!self.touches_edge_or_occupied(t, day, start, span));
        if hours > 0 {
            [rank, load, fragmented, day, start]
        } else {
            [rank, fragmented, -load, day, start]
        }
    }

    /// Blok, ızgara kenarına ya da dolu bir hücreye en az bir yandan bitişik mi?
    fn touches_edge_or_occupied(&self, t: usize, day: i64, start: i64, span: i64) -> bool {
        let occupied = |hour: i64| {
            self.p
                .cell_bit(day, hour)
                .is_some_and(|bit| self.teachers[t].occ >> bit & 1 == 1)
        };
        let end = start + span;
        start == self.p.day_start || end == self.p.day_end || occupied(start - 1) || occupied(end)
    }

    /// En az bir blok konumu var mı? `respect_daily` kapalıysa günlük sınır
    /// yok sayılır (saat artışının neden olmadığını açıklamak için).
    pub fn any_block_position(&self, t: usize, c: usize, hours: i64, respect_daily: bool) -> bool {
        let span = visit_span(hours);
        (1..=DAYS as i64)
            .filter(|day| !respect_daily || self.daily_cap_allows(t, *day, hours))
            .any(|day| {
                (self.p.day_start..=(self.p.day_end - span))
                    .any(|start| self.cells_free(t, c, day, start, span))
            })
    }
}
