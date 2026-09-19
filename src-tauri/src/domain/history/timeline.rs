//! Katlama: olayların sıralanması ve tarih aralıklı durumlara indirgenmesi
//! (spec §3, "Rebuild, test kâhini olarak kullanılır").
//!
//! Aralıklar YARI AÇIKTIR: `[valid_from, valid_to)`. `valid_to` günü artık
//! yeni duruma aittir.

use std::collections::BTreeSet;

use chrono::NaiveDate;

use super::events::{EventPayload, StoredEvent};

/// Katlamaya girecek, zaten sıralanmış bir olay. `order_events`'in çıktısıdır.
#[derive(Debug, Clone, PartialEq)]
pub struct TimedEvent {
    pub id: i64,
    pub effective_date: NaiveDate,
    pub payload: EventPayload,
}

/// Geri alınmış olayları ve geri alma işaretlerini atar, açılış olaylarının
/// tarihini `term_start`'a sabitler (saklanan tarih yalnız bilgi amaçlıdır —
/// spec §5.1) ve `(effective_date, id)` sırasıyla dizer.
pub fn order_events(events: &[StoredEvent], term_start: NaiveDate) -> Vec<TimedEvent> {
    let revoked_target_ids: BTreeSet<i64> = events.iter().filter_map(|e| e.revokes).collect();

    let mut ordered: Vec<TimedEvent> = events
        .iter()
        .filter(|e| !matches!(e.payload, EventPayload::Revoked))
        .filter(|e| !revoked_target_ids.contains(&e.id))
        .map(|e| TimedEvent {
            id: e.id,
            effective_date: if e.is_opening { term_start } else { e.effective_date },
            payload: e.payload.clone(),
        })
        .collect();

    ordered.sort_by_key(|te| (te.effective_date, te.id));
    ordered
}

/// Bir öznenin tarih aralıklı durumlarından biri. `[valid_from, valid_to)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Interval<S> {
    pub valid_from: NaiveDate,
    pub valid_to: Option<NaiveDate>,
    pub state: S,
    /// Bu aralığı üreten olayın id'si — aynı gün birden çok olay varsa o
    /// günün SON (kazanan) olayıdır.
    pub source_event_id: i64,
}

/// Olayları sırayla katlar. Aynı günün olayları id sırasıyla uygulanır;
/// o günün SON durumu geçerli olur — gün içindeki ara durumlar hiç
/// görünmez, sıfır uzunluklu aralık üretilmez. Durum değişmediyse yeni
/// aralık açılmaz. `None`, bir sonraki `Some` durana kadar bir boşluk
/// bırakır (aralık listesinde hiç yer almaz).
pub fn fold_intervals<S: Clone + PartialEq>(
    events: &[TimedEvent],
    apply: impl Fn(Option<&S>, &EventPayload) -> Option<S>,
) -> Vec<Interval<S>> {
    let mut intervals: Vec<Interval<S>> = Vec::new();
    let mut current: Option<S> = None;

    for te in events {
        let next = apply(current.as_ref(), &te.payload);
        if next == current {
            continue;
        }

        let opened_today = intervals
            .last()
            .is_some_and(|last| last.valid_to.is_none() && last.valid_from == te.effective_date);

        if opened_today {
            // Bu günün daha önceki bir olayı bir aralık açmıştı; o aralık
            // hiç "kesinleşmedi" — bugünün son olayı onu geçersiz kılıyor.
            intervals.pop();
        } else if let Some(last) = intervals.last_mut() {
            if last.valid_to.is_none() {
                last.valid_to = Some(te.effective_date);
            }
        }

        if let Some(state) = next.clone() {
            intervals.push(Interval {
                valid_from: te.effective_date,
                valid_to: None,
                state,
                source_event_id: te.id,
            });
        }
        current = next;
    }

    intervals
}

/// `fold_intervals` çıktısının sorgulanabilir hâli.
#[derive(Debug, Clone, PartialEq)]
pub struct Timeline<S> {
    pub intervals: Vec<Interval<S>>,
}

impl<S: Clone> Timeline<S> {
    /// `d` gününde geçerli durum. Yarı açık aralık: `valid_to` günü dahil
    /// değildir.
    pub fn state_at(&self, d: NaiveDate) -> Option<&S> {
        self.intervals
            .iter()
            .find(|iv| iv.valid_from <= d && iv.valid_to.is_none_or(|vt| d < vt))
            .map(|iv| &iv.state)
    }

    /// `d` ve sonrasındaki tüm değişim tarihleri (`valid_from` ve
    /// `valid_to` değerlerinin birleşimi), sıralı ve tekil.
    pub fn change_dates_from(&self, d: NaiveDate) -> Vec<NaiveDate> {
        let mut dates: Vec<NaiveDate> = self
            .intervals
            .iter()
            .flat_map(|iv| std::iter::once(iv.valid_from).chain(iv.valid_to))
            .filter(|date| *date >= d)
            .collect();
        dates.sort();
        dates.dedup();
        dates
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::history::events::{Labels, Stream};

    fn placed(to: i64) -> EventPayload {
        EventPayload::StudentPlaced { to_company_id: to, from_company_id: None, source: "manual".into(), labels: Labels(Default::default()) }
    }

    fn left() -> EventPayload {
        EventPayload::StudentLeft { from_company_id: 0, labels: Labels(Default::default()) }
    }

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn stored(id: i64, effective_date: NaiveDate, payload: EventPayload) -> StoredEvent {
        StoredEvent {
            id,
            change_set_id: id,
            stream: Stream::Placement,
            subject_id: 1,
            term: "2026-2027/1".into(),
            effective_date,
            payload,
            caused_by: None,
            revokes: None,
            is_opening: false,
        }
    }

    fn apply_placement(prev: Option<&i64>, e: &EventPayload) -> Option<i64> {
        crate::domain::history::apply::apply_placement(prev, e)
    }

    #[test]
    fn fold_produces_half_open_sorted_intervals() {
        let events = order_events(
            &[
                stored(1, ymd(2026, 9, 1), placed(10)),
                stored(2, ymd(2026, 11, 3), placed(20)),
            ],
            ymd(2026, 9, 1),
        );
        let intervals = fold_intervals(&events, apply_placement);
        assert_eq!(intervals.len(), 2);
        assert_eq!(intervals[0].valid_from, ymd(2026, 9, 1));
        assert_eq!(intervals[0].valid_to, Some(ymd(2026, 11, 3)));
        assert_eq!(intervals[1].valid_from, ymd(2026, 11, 3));
        assert_eq!(intervals[1].valid_to, None);
    }

    #[test]
    fn same_day_later_recorded_event_wins() {
        let events = order_events(
            &[
                stored(5, ymd(2026, 11, 3), placed(10)),
                stored(6, ymd(2026, 11, 3), placed(20)),
            ],
            ymd(2026, 9, 1),
        );
        let intervals = fold_intervals(&events, apply_placement);
        assert_eq!(intervals.len(), 1, "gün içindeki ara durum görünmemeli");
        assert_eq!(intervals[0].state, 20);
        assert_eq!(intervals[0].source_event_id, 6);
    }

    #[test]
    fn unchanged_state_opens_no_new_interval() {
        let events = order_events(
            &[
                stored(1, ymd(2026, 9, 1), placed(10)),
                stored(2, ymd(2026, 10, 1), placed(10)), // aynı işletme, gerçek değişim yok
            ],
            ymd(2026, 9, 1),
        );
        let intervals = fold_intervals(&events, apply_placement);
        assert_eq!(intervals.len(), 1);
        assert_eq!(intervals[0].source_event_id, 1, "durum değişmedi, aralık açılmadı");
    }

    #[test]
    fn none_leaves_a_gap() {
        let events = order_events(
            &[
                stored(1, ymd(2026, 9, 1), placed(10)),
                stored(2, ymd(2026, 11, 3), left()),
                stored(3, ymd(2026, 12, 1), placed(30)),
            ],
            ymd(2026, 9, 1),
        );
        let timeline = Timeline { intervals: fold_intervals(&events, apply_placement) };
        assert_eq!(timeline.state_at(ymd(2026, 11, 15)), None, "ayrılıştan sonra, yeni yerleşimden önce boşluk");
        assert_eq!(timeline.state_at(ymd(2026, 10, 1)), Some(&10));
        assert_eq!(timeline.state_at(ymd(2026, 12, 1)), Some(&30));
    }

    #[test]
    fn state_at_valid_to_is_excluded() {
        let events = order_events(
            &[
                stored(1, ymd(2026, 9, 1), placed(10)),
                stored(2, ymd(2026, 11, 3), placed(20)),
            ],
            ymd(2026, 9, 1),
        );
        let timeline = Timeline { intervals: fold_intervals(&events, apply_placement) };
        assert_eq!(timeline.state_at(ymd(2026, 11, 2)), Some(&10));
        assert_eq!(timeline.state_at(ymd(2026, 11, 3)), Some(&20), "valid_to günü YENİ duruma aittir");
    }

    #[test]
    fn revoked_events_and_markers_are_dropped() {
        let mut revoke_marker = stored(3, ymd(2026, 11, 5), EventPayload::Revoked);
        revoke_marker.revokes = Some(2);
        let events = order_events(
            &[
                stored(1, ymd(2026, 9, 1), placed(10)),
                stored(2, ymd(2026, 11, 3), placed(20)),
                revoke_marker,
            ],
            ymd(2026, 9, 1),
        );
        assert_eq!(events.len(), 1, "hem hedef hem işaret düşmeli");
        assert_eq!(events[0].id, 1);
    }

    #[test]
    fn opening_events_are_placed_at_term_start() {
        let mut opening = stored(1, ymd(2026, 9, 20), placed(10));
        opening.is_opening = true; // saklanan tarih 9/20 olsa da göç günü değil dönem başı sayılır
        let events = order_events(&[opening], ymd(2026, 9, 1));
        assert_eq!(events[0].effective_date, ymd(2026, 9, 1));
    }

    // --- Özellik testleri: elle yazılmış, tohumlu bir LCG. Yeni crate yok. ---

    /// Basit bir doğrusal eşlenik üreteç (LCG); testler arası tekrarlanabilir
    /// olsun diye sabit bir tohumla başlatılır.
    struct Lcg(u64);

    impl Lcg {
        fn new(seed: u64) -> Self {
            Lcg(seed)
        }

        fn next_u64(&mut self) -> u64 {
            // Numerical Recipes sabitleri; kriptografik değil, yalnız test
            // girdisi üretmek için yeterli.
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0
        }

        fn range(&mut self, bound: u64) -> u64 {
            self.next_u64() % bound
        }
    }

    /// Rastgele bir öğrenci yerleştirme/nakil/ayrılış tarihçesi üretir.
    /// Her olayın (date, id) çifti tekildir.
    fn random_events(lcg: &mut Lcg, count: usize, base: NaiveDate) -> Vec<StoredEvent> {
        (1..=count as i64)
            .map(|id| {
                let day_offset = lcg.range(90) as i64; // ~3 aylık pencere
                let date = base + chrono::Days::new(day_offset as u64);
                let payload = if lcg.range(4) == 0 {
                    left()
                } else {
                    placed(1 + lcg.range(5) as i64)
                };
                stored(id, date, payload)
            })
            .collect()
    }

    /// Bir olay dizisini rastgele karıştırır (Fisher–Yates, LCG ile).
    fn shuffled<T>(mut items: Vec<T>, lcg: &mut Lcg) -> Vec<T> {
        for i in (1..items.len()).rev() {
            let j = lcg.range((i + 1) as u64) as usize;
            items.swap(i, j);
        }
        items
    }

    /// P1: Aralıklar çakışmaz, sıralıdır, her `source_event_id` en fazla bir
    /// kez geçer.
    #[test]
    fn p1_intervals_never_overlap_and_are_sorted() {
        let mut lcg = Lcg::new(1);
        let base = ymd(2026, 9, 1);
        for _ in 0..200 {
            let raw = random_events(&mut lcg, 8, base);
            let ordered = order_events(&raw, base);
            let intervals = fold_intervals(&ordered, apply_placement);

            for w in intervals.windows(2) {
                assert!(w[0].valid_from < w[1].valid_from, "sıralı olmalı");
                assert!(
                    w[0].valid_to.is_some_and(|vt| vt <= w[1].valid_from),
                    "çakışmamalı: {:?} / {:?}",
                    w[0],
                    w[1]
                );
            }

            let mut source_ids: Vec<i64> = intervals.iter().map(|iv| iv.source_event_id).collect();
            let before = source_ids.len();
            source_ids.sort();
            source_ids.dedup();
            assert_eq!(source_ids.len(), before, "her source_event_id en fazla bir kez geçmeli");
        }
    }

    /// P2: Farklı `(date, id)` değerli 6 olayın 720 permütasyonunun HEPSİ
    /// aynı katlama sonucunu üretir — giriş sırası sonucu etkilemez.
    #[test]
    fn p2_all_720_permutations_of_six_events_agree() {
        let base = ymd(2026, 9, 1);
        let canonical: Vec<StoredEvent> = vec![
            stored(1, ymd(2026, 9, 5), placed(1)),
            stored(2, ymd(2026, 9, 20), placed(2)),
            stored(3, ymd(2026, 10, 1), left()),
            stored(4, ymd(2026, 10, 1), placed(3)), // aynı gün, id 3'ten sonra kazanır
            stored(5, ymd(2026, 11, 12), placed(4)),
            stored(6, ymd(2026, 12, 25), left()),
        ];
        let baseline = fold_intervals(&order_events(&canonical, base), apply_placement);

        let mut count = 0;
        for_each_permutation(canonical, &mut |perm| {
            count += 1;
            let result = fold_intervals(&order_events(perm, base), apply_placement);
            assert_eq!(result, baseline, "giriş sırası sonucu değiştirmemeli");
        });
        assert_eq!(count, 720, "6! = 720 permütasyonun hepsi denenmeli");
    }

    /// Heap algoritması: elle yazılmış, dışarıdan bir crate gerektirmeyen
    /// permütasyon üreteci.
    fn for_each_permutation<T: Clone>(mut items: Vec<T>, visit: &mut impl FnMut(&[T])) {
        fn heap<T: Clone>(k: usize, items: &mut Vec<T>, visit: &mut impl FnMut(&[T])) {
            if k == 1 {
                visit(items);
                return;
            }
            for i in 0..k {
                heap(k - 1, items, visit);
                if k % 2 == 0 {
                    items.swap(i, k - 1);
                } else {
                    items.swap(0, k - 1);
                }
            }
        }
        let n = items.len();
        heap(n, &mut items, visit);
    }

    /// P3: `state_at(x)`, tarihi `x` veya daha önce olan son olay
    /// uygulandıktan sonraki durumdur.
    #[test]
    fn p3_state_at_matches_replay_up_to_date() {
        let mut lcg = Lcg::new(7);
        let base = ymd(2026, 9, 1);
        for _ in 0..200 {
            let raw = random_events(&mut lcg, 8, base);
            let ordered = order_events(&raw, base);
            let timeline = Timeline { intervals: fold_intervals(&ordered, apply_placement) };

            let query_offset = lcg.range(120) as i64;
            let query = base + chrono::Days::new(query_offset as u64);

            let prefix: Vec<&TimedEvent> = ordered.iter().filter(|te| te.effective_date <= query).collect();
            let mut expected: Option<i64> = None;
            for te in &prefix {
                expected = apply_placement(expected.as_ref(), &te.payload);
            }

            assert_eq!(timeline.state_at(query).copied(), expected);
        }
    }

    /// P4 (ay değişmezliği): Tarihi ≥ 2026-11-01 olan olaylar eklenince,
    /// `x ≤ 2026-10-31` için `state_at(x)` değişmez — geçmiş ayın puantajı
    /// zaten gönderildiği için asla kaymamalıdır (spec §2 madde 3).
    #[test]
    fn p4_events_from_november_never_change_october_state() {
        let mut lcg = Lcg::new(42);
        let base = ymd(2026, 9, 1);
        let november = ymd(2026, 11, 1);

        for _ in 0..200 {
            let before: Vec<StoredEvent> = random_events(&mut lcg, 5, base)
                .into_iter()
                .filter(|e| e.effective_date < november)
                .collect();
            let before_ordered = order_events(&before, base);
            let before_timeline = Timeline { intervals: fold_intervals(&before_ordered, apply_placement) };

            let mut all = before.clone();
            let mut extra = random_events(&mut lcg, 4, november);
            for (offset, e) in extra.iter_mut().enumerate() {
                e.id = 1000 + offset as i64; // önceki olaylarla çakışmayan id
            }
            all.extend(extra);
            let all_ordered = order_events(&shuffled(all, &mut lcg), base);
            let all_timeline = Timeline { intervals: fold_intervals(&all_ordered, apply_placement) };

            let query = base + chrono::Days::new(lcg.range(60) as u64); // Eylül–Ekim aralığında
            assert_eq!(
                before_timeline.state_at(query).copied(),
                all_timeline.state_at(query).copied(),
                "Kasım'a eklenen olaylar Ekim'in durumunu değiştirmemeli"
            );
        }
    }
}
