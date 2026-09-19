//! Öğretmen tarafı komutları: yük, program, ekleme, silme (spec §5.2, §5.4).

use chrono::NaiveDate;

use crate::domain::history::apply::{apply_load, apply_schedule};
use crate::domain::history::events::{EventPayload, Stream, StoredEvent, TeacherLoad, WeeklySchedule};
use crate::domain::history::impact::ImpactSummary;
use crate::domain::history::rejection::{Rejection, RejectionCode};
use crate::domain::scheduling::Slot;

use super::{command_kind, debug_opt, impact_line, labels, touched_from, ChangeRequest, Decision, DecisionContext, NewChangeSet, NewTeacherProfile, PlannedEvent, RowAction};

pub(super) fn create_teacher(ctx: &DecisionContext, req: &ChangeRequest, teacher: &NewTeacherProfile, load: &TeacherLoad) -> Result<Decision, Rejection> {
    let d = ctx.term.resolve_effective_date(req.effective_date, ctx.today)?;
    // Yerinde oluşturma (`teachers::create_in`) `decide`'dan ÖNCE çalışır.
    let teacher_id = ctx.materialized.teacher_id.unwrap_or_default();
    let label = format!("{} {}", teacher.first_name, teacher.last_name);

    let event = PlannedEvent {
        stream: Stream::TeacherLoad,
        subject_id: teacher_id,
        effective_date: d,
        payload: EventPayload::LoadSet { load: load.clone(), previous: None, source: "manual".to_string(), labels: labels(&[("teacher", &label)]) },
        caused_by: None,
        revokes: None,
    };
    let mut impact = ImpactSummary::empty(d, ctx.term.is_planning(ctx.today));
    impact.primary.push(impact_line(&label, &event, None, Some(format!("{load:?}"))));

    finish(ctx, req, d, vec![event], impact, Vec::new())
}

pub(super) fn set_teacher_load(ctx: &DecisionContext, req: &ChangeRequest, teacher_id: i64, load: &TeacherLoad) -> Result<Decision, Rejection> {
    let d = ctx.term.resolve_effective_date(req.effective_date, ctx.today)?;
    let label = ctx.teacher_label(teacher_id);
    let prior = ctx.state_before::<TeacherLoad>(Stream::TeacherLoad, teacher_id, d, apply_load);

    let event = PlannedEvent {
        stream: Stream::TeacherLoad,
        subject_id: teacher_id,
        effective_date: d,
        payload: EventPayload::LoadSet { load: load.clone(), previous: prior.clone(), source: "manual".to_string(), labels: labels(&[("teacher", &label)]) },
        caused_by: None,
        revokes: None,
    };
    let mut impact = ImpactSummary::empty(d, ctx.term.is_planning(ctx.today));
    impact.primary.push(impact_line(&label, &event, debug_opt(prior.as_ref()), Some(format!("{load:?}"))));

    finish(ctx, req, d, vec![event], impact, Vec::new())
}

pub(super) fn set_teacher_schedule(ctx: &DecisionContext, req: &ChangeRequest, teacher_id: i64, slots: &[Slot]) -> Result<Decision, Rejection> {
    let d = ctx.term.resolve_effective_date(req.effective_date, ctx.today)?;
    let label = ctx.teacher_label(teacher_id);
    let prior = ctx.state_before::<WeeklySchedule>(Stream::TeacherSchedule, teacher_id, d, apply_schedule);
    let schedule = WeeklySchedule(slots.iter().copied().collect());

    let event = PlannedEvent {
        stream: Stream::TeacherSchedule,
        subject_id: teacher_id,
        effective_date: d,
        payload: EventPayload::ScheduleSet {
            schedule: schedule.clone(),
            previous_slot_count: prior.as_ref().map(|p| p.0.len() as i64),
            source: "manual".to_string(),
            labels: labels(&[("teacher", &label)]),
        },
        caused_by: None,
        revokes: None,
    };
    let mut impact = ImpactSummary::empty(d, ctx.term.is_planning(ctx.today));
    impact.primary.push(impact_line(&label, &event, debug_opt(prior.as_ref()), Some(format!("{schedule:?}"))));

    finish(ctx, req, d, vec![event], impact, Vec::new())
}

/// AÇIK SORU (bkz. rapor): kaynak dönemin (`from_term`) program olayları
/// `DecisionContext` içinde YOKTUR — `db::history_context::load` yalnız
/// GÜNCEL dönemin olaylarını okur (brief, `DecisionContext.events`). Saf
/// `decide` bu yüzden kaynak veriyi göremez; kopyalama R3/R4'te ya `ctx`
/// genişletilerek ya da `change_service` düzeyinde ayrı bir sorguyla
/// çözülmelidir. Burada olaysız (no-op) bir karar döner — YANILTICI
/// olmaması için birincil satırı boş bırakır, bir uyarı YERİNE bunu raporda
/// açıkça bildiriyoruz.
pub(super) fn copy_schedules_from_term(ctx: &DecisionContext, req: &ChangeRequest, _from_term: &str) -> Result<Decision, Rejection> {
    let d = ctx.term.resolve_effective_date(req.effective_date, ctx.today)?;
    let impact = ImpactSummary::empty(d, ctx.term.is_planning(ctx.today));
    finish(ctx, req, d, Vec::new(), impact, Vec::new())
}

pub(super) fn delete_teacher(ctx: &DecisionContext, req: &ChangeRequest, teacher_id: i64) -> Result<Decision, Rejection> {
    let load_events = ctx.events_for(Stream::TeacherLoad, teacher_id);
    let schedule_events = ctx.events_for(Stream::TeacherSchedule, teacher_id);
    let live = |e: &&StoredEvent| !matches!(e.payload, EventPayload::Revoked);
    let has_history = load_events.iter().any(|e| !e.is_opening && live(&e)) || schedule_events.iter().any(|e| !e.is_opening && live(&e));
    if has_history {
        return Err(Rejection::new(
            RejectionCode::HasHistory,
            format!("{}: açılış dışında geçmişi var; silinemez, pasif yapılabilir.", ctx.teacher_label(teacher_id)),
        ));
    }

    let d = ctx.term.start;
    let mut events: Vec<PlannedEvent> = load_events
        .iter()
        .chain(schedule_events.iter())
        .filter(|e| e.is_opening && live(e))
        .map(|e| PlannedEvent { stream: e.stream, subject_id: teacher_id, effective_date: d, payload: EventPayload::Revoked, caused_by: None, revokes: Some(e.id) })
        .collect();
    events.sort_by_key(|e| e.revokes);

    let mut impact = ImpactSummary::empty(d, ctx.term.is_planning(ctx.today));
    impact.primary.push(crate::domain::history::impact::ImpactLine {
        kind: "teacher_deleted".to_string(),
        stream: Stream::TeacherLoad.as_str().to_string(),
        subject_id: teacher_id,
        subject_label: ctx.teacher_label(teacher_id),
        effective_date: d,
        before: Some("kayıtlı".to_string()),
        after: None,
    });

    finish(ctx, req, d, events, impact, vec![RowAction::DeleteTeacher(teacher_id)])
}

fn finish(
    ctx: &DecisionContext,
    req: &ChangeRequest,
    effective_date: NaiveDate,
    events: Vec<PlannedEvent>,
    impact: ImpactSummary,
    row_actions: Vec<RowAction>,
) -> Result<Decision, Rejection> {
    let touched = touched_from(&events, &req.term);
    let _ = ctx;
    Ok(Decision {
        change_set: NewChangeSet {
            term: req.term.clone(),
            kind: command_kind(&req.command).to_string(),
            effective_date,
            document_date: req.document_date,
            reason: req.reason.clone(),
            revokes_change_set_id: None,
        },
        events,
        impact,
        row_actions,
        touched,
    })
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use crate::domain::history::decide::ChangeCommand;
    use crate::domain::history::events::Labels;
    use crate::domain::models::{ChiefType, EmploymentType};

    fn sample_load() -> TeacherLoad {
        TeacherLoad { base_hours: 15, max_extra_hours: 24, other_extra_hours: 0, chief_type: ChiefType::None, employment_type: EmploymentType::Tenured }
    }

    #[test]
    fn set_teacher_load_carries_previous_state() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_teacher(5, "Ali Öğretmen")
            .with_event(stored_event(1, 1, Stream::TeacherLoad, 5, "2026-2027/1", ymd(2026, 9, 1), EventPayload::LoadSet { load: sample_load(), previous: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .build();

        let mut new_load = sample_load();
        new_load.other_extra_hours = 4;
        let req = ChangeRequest { term: "2026-2027/1".into(), effective_date: Some(ymd(2026, 11, 5)), document_date: None, reason: "test".into(), command: ChangeCommand::SetTeacherLoad { teacher_id: 5, load: new_load.clone() } };
        let decision = set_teacher_load(&ctx, &req, 5, &new_load).unwrap();
        match &decision.events[0].payload {
            EventPayload::LoadSet { previous, .. } => assert_eq!(previous, &Some(sample_load())),
            other => panic!("beklenmedik olay: {other:?}"),
        }
    }

    #[test]
    fn delete_teacher_with_only_opening_events_revokes_them() {
        let today = ymd(2026, 8, 1);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_teacher(5, "Ali Öğretmen")
            .with_event(stored_event(1, 1, Stream::TeacherLoad, 5, "2026-2027/1", ymd(2026, 9, 1), EventPayload::LoadSet { load: sample_load(), previous: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .build();

        let req = ChangeRequest { term: "2026-2027/1".into(), effective_date: None, document_date: None, reason: "sil".into(), command: ChangeCommand::DeleteTeacher { teacher_id: 5 } };
        let decision = delete_teacher(&ctx, &req, 5).unwrap();
        assert_eq!(decision.row_actions, vec![RowAction::DeleteTeacher(5)]);
        assert_eq!(decision.events.len(), 1);
        assert!(matches!(decision.events[0].payload, EventPayload::Revoked));
    }
}
