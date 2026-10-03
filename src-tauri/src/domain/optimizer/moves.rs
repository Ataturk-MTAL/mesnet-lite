//! Yerel arama komşulukları. Her biri durumu DEĞİŞTİRMEDEN geri bırakır ve
//! en iyi adayı (tam skorla) döner; kabul kararı `improve` içindedir.

use super::blocks::BlockPos;
use super::construct::{best_at, best_placement, Candidate};
use super::state::{Changes, Placement, Score, State};

pub(super) struct MoveCandidate {
    pub score: Score,
    pub changes: Changes,
    /// M4: `Some(u)` ise başkalarının saatini kısan hamle `u` için yer açtı.
    pub room_for: Option<usize>,
}

impl MoveCandidate {
    fn single(c: usize, found: Candidate) -> Self {
        Self {
            score: found.score,
            changes: vec![(c, Some(found.placement))],
            room_for: None,
        }
    }
}

/// M1: saati artır (aynı ya da başka öğretmende), ya da yerleşmemişi yerleştir.
pub(super) fn grow(state: &mut State, c: usize) -> Option<MoveCandidate> {
    let old = state.placement[c];
    if old.is_some_and(|o| o.hours >= state.p.ceiling[c]) {
        return None;
    }
    let min_hours = old.map_or(0, |o| o.hours + 1);
    state.unplace(c);
    let found = best_placement(state, c, min_hours, i64::MAX, old);
    state.set(c, old);
    found.map(|f| MoveCandidate::single(c, f))
}

/// M2: aynı saatle en iyi (öğretmen, blok)'a taşı.
pub(super) fn relocate(state: &mut State, c: usize) -> Option<MoveCandidate> {
    let old = state.placement[c]?;
    state.unplace(c);
    let found = best_at(state, c, old.hours, Some(old)).filter(|f| f.placement != old);
    state.set(c, Some(old));
    found.map(|f| MoveCandidate::single(c, f))
}

/// Çift hamlelerde bir işletmenin hedefi.
struct Target {
    teacher: usize,
    hours: i64,
    prefer: Option<BlockPos>,
}

/// `a` ve `b` önce kaldırılır, hedeflerine yerleştirilip puanlanır, sonra eski
/// hâline döner.
fn evaluate_pair(
    state: &mut State,
    (a, target_a): (usize, Target),
    (b, target_b): (usize, Target),
) -> Option<MoveCandidate> {
    let (old_a, old_b) = (state.placement[a], state.placement[b]);
    state.unplace(a);
    state.unplace(b);
    let result = place_pair(state, (a, target_a), (b, target_b));
    state.set(a, old_a);
    state.set(b, old_b);
    result
}

fn place_pair(
    state: &mut State,
    (a, target_a): (usize, Target),
    (b, target_b): (usize, Target),
) -> Option<MoveCandidate> {
    state.evals += 1;
    let first = placement_for(state, a, &target_a)?;
    state.place(a, first);
    let second = placement_for(state, b, &target_b);
    let result = second.map(|second| {
        state.place(b, second);
        let score = state.score();
        state.unplace(b);
        MoveCandidate {
            score,
            changes: vec![(a, Some(first)), (b, Some(second))],
            room_for: None,
        }
    });
    state.unplace(a);
    result
}

fn placement_for(state: &mut State, c: usize, target: &Target) -> Option<Placement> {
    let (day, start) = state.best_block(target.teacher, c, target.hours, target.prefer)?;
    Some(Placement {
        teacher: target.teacher,
        day,
        start,
        hours: target.hours,
    })
}

fn better(best: &mut Option<MoveCandidate>, candidate: Option<MoveCandidate>) {
    let Some(candidate) = candidate else {
        return;
    };
    if best
        .as_ref()
        .is_none_or(|known| candidate.score > known.score)
    {
        *best = Some(candidate);
    }
}

/// M3: iki işletmenin öğretmenlerini değiş tokuş et (saatler sabit).
pub(super) fn swap(state: &mut State, a: usize) -> Option<MoveCandidate> {
    let old_a = state.placement[a]?;
    let p = state.p;
    let mut best = None;
    for &b in &p.movable {
        let Some(old_b) = state.placement[b] else {
            continue;
        };
        if b == a || old_b.teacher == old_a.teacher {
            continue;
        }
        let to_b_teacher = Target {
            teacher: old_b.teacher,
            hours: old_a.hours,
            prefer: None,
        };
        let to_a_teacher = Target {
            teacher: old_a.teacher,
            hours: old_b.hours,
            prefer: None,
        };
        better(
            &mut best,
            evaluate_pair(state, (a, to_b_teacher), (b, to_a_teacher)),
        );
    }
    best
}

/// M5: `a`dan bir saat al, `b`ye ver (toplam sabit; yalnız dağılım değişir).
pub(super) fn shift_hour(state: &mut State, a: usize) -> Option<MoveCandidate> {
    let old_a = state.placement[a].filter(|o| o.hours > 0)?;
    let p = state.p;
    let mut best = None;
    for &b in &p.movable {
        let Some(old_b) = state.placement[b] else {
            continue;
        };
        if b == a || old_b.hours >= p.ceiling[b] {
            continue;
        }
        let give = Target {
            teacher: old_a.teacher,
            hours: old_a.hours - 1,
            prefer: Some((old_a.day, old_a.start)),
        };
        let take = Target {
            teacher: old_b.teacher,
            hours: old_b.hours + 1,
            prefer: Some((old_b.day, old_b.start)),
        };
        better(&mut best, evaluate_pair(state, (a, give), (b, take)));
    }
    best
}

/// M4: komşu `n`yi çıkar, `u`yu en yüksek uygun saatle yerleştir, `n`yi en
/// yüksek uygun saatle (0 dahil) geri koy. Saat kısma yalnız burada ve M5'te
/// olur; kabul kararı skora bağlıdır.
pub(super) fn eject_insert(state: &mut State, u: usize) -> Option<MoveCandidate> {
    let old_u = state.placement[u];
    if old_u.is_some_and(|o| o.hours >= state.p.ceiling[u]) {
        return None;
    }
    let p = state.p;
    let mut best = None;
    for &n in &p.movable {
        let Some(old_n) = state.placement[n] else {
            continue;
        };
        if n == u || p.eligible(old_n.teacher, u) == 0 {
            continue;
        }
        state.unplace(u);
        state.unplace(n);
        better(
            &mut best,
            insert_then_reinsert(state, (u, old_u), (n, old_n)),
        );
        state.set(u, old_u);
        state.set(n, Some(old_n));
    }
    best
}

fn insert_then_reinsert(
    state: &mut State,
    (u, old_u): (usize, Option<Placement>),
    (n, old_n): (usize, Placement),
) -> Option<MoveCandidate> {
    let min_hours = old_u.map_or(0, |o| o.hours + 1);
    let top = state.max_hours_for(u);
    for hours in (min_hours..=top).rev() {
        let Some(first) = best_at(state, u, hours, old_u) else {
            continue;
        };
        state.place(u, first.placement);
        let second = best_placement(state, n, 0, i64::MAX, Some(old_n));
        state.unplace(u);
        if let Some(second) = second {
            return Some(MoveCandidate {
                score: second.score,
                changes: vec![(u, Some(first.placement)), (n, Some(second.placement))],
                room_for: Some(u),
            });
        }
    }
    None
}
