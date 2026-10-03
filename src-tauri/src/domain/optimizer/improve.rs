//! En-iyi-iyileştirme tepe tırmanışı: sabit komşuluk ve işletme sırası,
//! yalnız KESİN skor artışı kabul. Sayaç sınırları; duvar saati yok.

use super::moves::{eject_insert, grow, relocate, shift_hour, swap, MoveCandidate};
use super::state::State;
use super::MAX_SWEEPS;
use std::collections::BTreeMap;

type Neighbourhood = fn(&mut State, usize) -> Option<MoveCandidate>;

/// Sıra tasarımdaki gibi: M4, M1, M2, M3, M5.
const NEIGHBOURHOODS: [Neighbourhood; 5] = [eject_insert, grow, relocate, swap, shift_hour];

pub(super) struct ImproveReport {
    /// İşletme → (hamleden sonraki saati, yer açılan işletme). Nedenin
    /// türetilmesinde yalnız nihai saat hâlâ aynıysa geçerlidir.
    pub made_room: BTreeMap<usize, (i64, usize)>,
    pub limit_reached: bool,
}

pub(super) fn improve(state: &mut State) -> ImproveReport {
    let mut report = ImproveReport {
        made_room: BTreeMap::new(),
        limit_reached: false,
    };
    let movable = state.p.movable.clone();
    let mut converged = false;
    for _ in 0..MAX_SWEEPS {
        let mut improved = false;
        for neighbourhood in NEIGHBOURHOODS {
            for &c in &movable {
                if state.exhausted() {
                    report.limit_reached = true;
                    return report;
                }
                let Some(candidate) = neighbourhood(state, c) else {
                    continue;
                };
                if candidate.score > state.score() {
                    commit(state, candidate, &mut report);
                    improved = true;
                }
            }
        }
        if !improved {
            converged = true;
            break;
        }
    }
    // Yakınsamadan çıkış = süpürme sınırı da bir sınırdır.
    report.limit_reached = !converged;
    report
}

fn commit(state: &mut State, candidate: MoveCandidate, report: &mut ImproveReport) {
    if let Some(room_for) = candidate.room_for {
        for (c, target) in &candidate.changes {
            let (Some(before), Some(after)) = (state.placement[*c], target) else {
                continue;
            };
            if *c != room_for && after.hours < before.hours {
                report.made_room.insert(*c, (after.hours, room_for));
            }
        }
    }
    state.apply(&candidate.changes);
}
