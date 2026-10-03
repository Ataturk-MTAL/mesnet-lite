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
    for (c, target) in &candidate.changes {
        // Bu hamle işletmenin saatini ya da yerini değiştiriyor: önceki
        // `MadeRoomFor` kaydı bayatlar (sonradan aynı saate dönülse bile).
        report.made_room.remove(c);
        let (Some(before), Some(after), Some(room_for)) =
            (state.placement[*c], target, candidate.room_for)
        else {
            continue;
        };
        if *c != room_for && after.hours < before.hours {
            report.made_room.insert(*c, (after.hours, room_for));
        }
    }
    state.apply(&candidate.changes);
    #[cfg(test)]
    state.assert_consistent("hamle sonrası");
}

#[cfg(test)]
mod tests {
    use super::super::problem::Problem;
    use super::super::state::Placement;
    use super::super::tests::{company, free, teacher};
    use super::super::{EngineInput, ProposalMode, TeacherInput};
    use super::*;

    /// Başka bir hamle işletmenin saatini değiştirince eski `MadeRoomFor`
    /// kaydı düşer; sonradan aynı saate dönülse bile bayat kayıt geri gelmez.
    #[test]
    fn made_room_record_is_dropped_when_another_move_changes_the_hours() {
        let companies = [company(1), company(2)];
        let teachers: [TeacherInput; 1] = [TeacherInput {
            free_slots: free(1, 9..17),
            ..teacher(1)
        }];
        let input = EngineInput {
            companies: &companies,
            teachers: &teachers,
            day_start_hour: 9,
            day_end_hour: 17,
            pool_hours: 0,
            mode: ProposalMode::FillGaps,
            balance_gap_hours: 4,
        };
        let problem = Problem::new(&input);
        let mut state = State::new(&problem);
        let at = |start, hours| Placement {
            teacher: 0,
            day: 1,
            start,
            hours,
        };
        state.place(0, at(9, 2));
        let mut report = ImproveReport {
            made_room: BTreeMap::from([(0, (2, 1))]),
            limit_reached: false,
        };

        let grow = MoveCandidate {
            score: state.score(),
            changes: vec![(0, Some(at(9, 3)))],
            room_for: None,
        };
        commit(&mut state, grow, &mut report);
        let back = MoveCandidate {
            score: state.score(),
            changes: vec![(0, Some(at(9, 2)))],
            room_for: None,
        };
        commit(&mut state, back, &mut report);

        assert!(report.made_room.is_empty());
    }
}
