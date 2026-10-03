//! Nihai durumdan `AllocationProposal` kurar. Listeler işletme/öğretmen `id`
//! sırasındadır; çıktı girdi sırasından bağımsızdır.

use super::explain::{hour_reason, unassigned_reason};
use super::improve::ImproveReport;
use super::model::*;
use super::problem::Problem;
use super::state::{Placement, State};
use crate::domain::scheduling::Block;
use std::collections::BTreeMap;

pub(super) fn build(p: &Problem, state: &mut State, report: &ImproveReport) -> AllocationProposal {
    let mut out = AllocationProposal {
        mode: if p.redistribute {
            ProposalMode::Redistribute
        } else {
            ProposalMode::FillGaps
        },
        assignments: Vec::new(),
        kept: Vec::new(),
        released: Vec::new(),
        hour_changes: Vec::new(),
        group_splits: group_splits(p, state),
        unassigned: Vec::new(),
        teacher_loads: teacher_loads(p, state),
        placed_count: 0,
        total_hours: 0,
        pool_hours: p.pool_hours,
        pool_remaining: None,
        warnings: p.warnings.clone(),
    };
    for c in 0..p.companies.len() {
        match (p.is_fixed[c], state.placement[c]) {
            (true, _) => keep_fixed(p, c, &mut out),
            (false, Some(placement)) => place_movable(p, state, c, placement, report, &mut out),
            (false, None) => leave_unplaced(p, c, &mut out),
        }
    }
    let base: i64 = p.teachers.iter().map(|t| t.base_assigned_hours).sum();
    out.pool_remaining = (p.pool_hours > 0).then(|| p.pool_hours - base - out.total_hours);
    if report.limit_reached {
        out.warnings
            .push("Arama sınırına ulaşıldı; öneri en iyi çözüm olmayabilir".into());
    }
    out
}

fn kept_entry(company: &CompanyInput, hours: i64) -> Option<KeptAssignment> {
    let current = company.current.as_ref()?;
    Some(KeptAssignment {
        company_id: company.id,
        company_name: company.name.clone(),
        teacher_id: current.teacher_id,
        visit_day: current.visit_day,
        visit_hour: current.visit_hour,
        awarded_hours: hours,
        is_locked: company.is_locked,
        is_forced: current.is_forced,
    })
}

fn keep_fixed(p: &Problem, c: usize, out: &mut AllocationProposal) {
    let company = p.companies[c];
    let hours = company.awarded_hours.max(0);
    out.kept.extend(kept_entry(company, hours));
    out.placed_count += 1;
    out.total_hours += hours;
}

fn place_movable(
    p: &Problem,
    state: &mut State,
    c: usize,
    placement: Placement,
    report: &ImproveReport,
    out: &mut AllocationProposal,
) {
    let company = p.companies[c];
    let teacher = p.teachers[placement.teacher];
    out.placed_count += 1;
    out.total_hours += placement.hours;

    let unchanged = company.current.as_ref().is_some_and(|cur| {
        cur.teacher_id == teacher.id
            && cur.visit_day == placement.day
            && cur.visit_hour == placement.start
    });
    if unchanged {
        out.kept.extend(kept_entry(company, placement.hours));
    } else {
        out.assignments.push(proposed_assignment(p, c, placement));
    }
    if placement.hours != company.awarded_hours {
        let room = report.made_room.get(&c);
        let (reason_code, reason) = hour_reason(state, c, placement, room);
        out.hour_changes.push(HourChange {
            company_id: company.id,
            company_name: company.name.clone(),
            old_hours: company.awarded_hours,
            new_hours: placement.hours,
            reason_code,
            reason,
        });
    }
}

fn proposed_assignment(p: &Problem, c: usize, placement: Placement) -> ProposedAssignment {
    let company = p.companies[c];
    let teacher = p.teachers[placement.teacher];
    let block = Block::from_start(placement.day, placement.start, placement.hours);
    ProposedAssignment {
        company_id: company.id,
        company_name: company.name.clone(),
        teacher_id: teacher.id,
        teacher_name: teacher.name.clone(),
        awarded_hours: placement.hours,
        previous_hours: company.awarded_hours,
        visit_day: placement.day,
        visit_hour: placement.start,
        visit_end_hour: block.end_hour,
        exact_branch_match: p.is_exact(placement.teacher, c),
        group_key: company.group_key.clone(),
        previous: company.current.clone(),
    }
}

fn leave_unplaced(p: &Problem, c: usize, out: &mut AllocationProposal) {
    let company = p.companies[c];
    let (reason_code, reason) = unassigned_reason(p, c);
    out.unassigned.push(UnassignedCompany {
        company_id: company.id,
        company_name: company.name.clone(),
        reason_code,
        reason,
        was_assigned: company.current.is_some(),
    });
    // Saat kayıtta kalırsa havuzu uygulama anında aşırabilir; kilitli değilse
    // saat havuza geri döner. Kilitli işletmenin saati aynen kalır.
    if !company.is_locked && company.awarded_hours > 0 {
        out.hour_changes.push(HourChange {
            company_id: company.id,
            company_name: company.name.clone(),
            old_hours: company.awarded_hours,
            new_hours: 0,
            reason_code: HourChangeReason::Unplaced,
            reason: format!(
                "Yerleştirilemedi; saati havuza geri döndü ({} → 0)",
                company.awarded_hours
            ),
        });
    }
    if let Some(previous) = &company.current {
        out.released.push(ReleasedAssignment {
            company_id: company.id,
            company_name: company.name.clone(),
            previous: previous.clone(),
        });
    }
}

fn teacher_loads(p: &Problem, state: &State) -> Vec<TeacherLoadSummary> {
    p.teachers
        .iter()
        .zip(&state.teachers)
        .map(|(teacher, load)| TeacherLoadSummary {
            teacher_id: teacher.id,
            teacher_name: teacher.name.clone(),
            hours: load.hours,
            capacity: teacher.capacity,
            company_count: load.count,
            distinct_groups: load.distinct,
        })
        .collect()
}

/// İki ya da daha çok bölgeye yayılan her öğretmen (grupsuzlar sayılmaz).
fn group_splits(p: &Problem, state: &State) -> Vec<GroupSplit> {
    let mut splits = Vec::new();
    for (t, load) in state.teachers.iter().enumerate() {
        if load.distinct < 2 {
            continue;
        }
        let mut by_key: BTreeMap<&str, GroupSplitPart> = BTreeMap::new();
        for (c, company) in p.companies.iter().enumerate() {
            let on_teacher = state.placement[c].is_some_and(|pl| pl.teacher == t);
            let Some(key) = company.group_key.as_deref().filter(|_| on_teacher) else {
                continue;
            };
            by_key
                .entry(key)
                .or_insert_with(|| GroupSplitPart {
                    group_key: key.to_string(),
                    group_label: company.group_label.clone(),
                    company_names: Vec::new(),
                })
                .company_names
                .push(company.name.clone());
        }
        splits.push(GroupSplit {
            teacher_id: p.teachers[t].id,
            teacher_name: p.teachers[t].name.clone(),
            groups: by_key.into_values().collect(),
        });
    }
    splits
}
