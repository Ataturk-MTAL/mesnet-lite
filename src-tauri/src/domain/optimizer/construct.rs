//! Başlangıç çözümü: ağır işletmeler önce, her biri için saat önce, sonra
//! tam skorla en iyi öğretmen.

use super::state::{Placement, Score, State};

/// Aday yerleşimi tam skorla karşılaştırma anahtarı: skor, sonra aynı skorda
/// daha az yüklü gün, erken gün/saat ve düşük öğretmen sırası (belirlenimci
/// eşitlik bozma).
type Key = (Score, [i64; 4]);

pub(super) struct Candidate {
    pub placement: Placement,
    pub score: Score,
}

pub(super) fn construct(state: &mut State) {
    if state.p.redistribute {
        seed_current_placements(state);
    }
    for c in state.p.construct_order.clone() {
        if state.placement[c].is_some() {
            continue;
        }
        if let Some(found) = best_placement(state, c, 0, i64::MAX, None) {
            state.place(c, found.placement);
        }
    }
}

/// Redistribute: mevcut öneriler hâlâ geçerliyse oldukları yerde başlar;
/// böylece arama yalnızca gerçekten kazandıran taşımaları yapar.
fn seed_current_placements(state: &mut State) {
    for c in state.p.construct_order.clone() {
        let company = state.p.companies[c];
        let Some(current) = &company.current else {
            continue;
        };
        let Some(t) = state.p.teacher_index(current.teacher_id) else {
            continue;
        };
        let pos = (current.visit_day, current.visit_hour);
        let wanted = company.awarded_hours.min(state.max_hours_for(c)).max(0);
        let fitting = (0..=wanted).rev().find(|h| state.fits_at(t, c, pos, *h));
        if let Some(hours) = fitting {
            state.place(
                c,
                Placement {
                    teacher: t,
                    day: pos.0,
                    start: pos.1,
                    hours,
                },
            );
        }
    }
}

/// `c` için `min_hours..=max_hours` aralığındaki EN YÜKSEK uygun saatte,
/// skoru en iyi (öğretmen, blok). `c` durumda yerleşmemiş olmalı.
/// Saat önce gelir: skorun saat terimi eşitlik terimlerinden üstündür.
pub(super) fn best_placement(
    state: &mut State,
    c: usize,
    min_hours: i64,
    max_hours: i64,
    prefer: Option<Placement>,
) -> Option<Candidate> {
    let top = state.max_hours_for(c).min(max_hours);
    (min_hours..=top)
        .rev()
        .find_map(|hours| best_at(state, c, hours, prefer))
}

/// Verilen saatte tüm öğretmenler arasında en iyi yerleşim.
pub(super) fn best_at(
    state: &mut State,
    c: usize,
    hours: i64,
    prefer: Option<Placement>,
) -> Option<Candidate> {
    let mut best: Option<(Key, Candidate)> = None;
    for t in 0..state.p.teachers.len() {
        let wanted = prefer.filter(|p| p.teacher == t).map(|p| (p.day, p.start));
        let Some((day, start)) = state.best_block(t, c, hours, wanted) else {
            continue;
        };
        let load = state.day_load(t, day);
        let placement = Placement {
            teacher: t,
            day,
            start,
            hours,
        };
        state.place(c, placement);
        let score = state.score();
        state.unplace(c);
        state.evals += 1;
        let key = (score, [-load, -day, -start, -(t as i64)]);
        if best.as_ref().is_none_or(|(known, _)| key > *known) {
            best = Some((key, Candidate { placement, score }));
        }
    }
    best.map(|(_, candidate)| candidate)
}
