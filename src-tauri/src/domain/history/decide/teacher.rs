//! Öğretmen tarafı komutları: yük, program, ekleme, silme (spec §5.2, §5.4).

use chrono::NaiveDate;

use crate::domain::history::apply::{apply_load, apply_schedule};
use crate::domain::history::events::{EventPayload, Stream, StoredEvent, TeacherLoad, WeeklySchedule};
use crate::domain::history::impact::ImpactSummary;
use crate::domain::history::rejection::{Rejection, RejectionCode};
use crate::domain::scheduling::Slot;

use super::flags::{apply_shadow_and_future_notices, teacher_capacity_warnings, teacher_schedule_warnings};
use super::{
    command_kind, debug_opt, impact_line, labels, touched_from, with_pending, ChangeRequest, Decision, DecisionContext,
    NewChangeSet, NewTeacherProfile, PlannedEvent, RowAction,
};

pub(super) fn create_teacher(ctx: &DecisionContext, req: &ChangeRequest, teacher: &NewTeacherProfile, load: &TeacherLoad) -> Result<Decision, Rejection> {
    let d = ctx.term.resolve_effective_date(req.effective_date, ctx.today)?;
    // Yerinde oluşturma (`teachers::create_in`) `decide`'dan ÖNCE çalışır;
    // `None` sessizce `0` id'sine düşmek yerine reddedilir (R2b brief madde 3).
    let teacher_id = ctx.require_materialized_teacher()?;
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
    ctx.require_teacher(teacher_id)?;
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
    apply_shadow_and_future_notices(ctx, Stream::TeacherLoad, teacher_id, &label, d, &mut impact);

    // Yükseltilmiş `other_extra_hours`/şeflik kapasiteyi aşabilir; kural
    // yeniden yazılmaz, yalnız bayraklanır (spec §5.3, R2b brief madde 2).
    let augmented = with_pending(ctx, &[event.clone()]);
    impact.warnings.extend(teacher_capacity_warnings(&augmented, teacher_id, d));

    finish(ctx, req, d, vec![event], impact, Vec::new())
}

pub(super) fn set_teacher_schedule(ctx: &DecisionContext, req: &ChangeRequest, teacher_id: i64, slots: &[Slot]) -> Result<Decision, Rejection> {
    let d = ctx.term.resolve_effective_date(req.effective_date, ctx.today)?;
    ctx.require_teacher(teacher_id)?;
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
    apply_shadow_and_future_notices(ctx, Stream::TeacherSchedule, teacher_id, &label, d, &mut impact);

    // Boş saatler daralınca zorlanmamış ziyaret blokları dışarıda kalabilir
    // (spec §5.3, R2b brief madde 2).
    let augmented = with_pending(ctx, &[event.clone()]);
    impact.warnings.extend(teacher_schedule_warnings(&augmented, teacher_id, d));

    finish(ctx, req, d, vec![event], impact, Vec::new())
}

/// Kaynak dönemdeki (`ctx.source_schedules`, R3'te `history_context::load`
/// yalnız BU komut için doldurur) her öğretmen için, hedef dönemde `d`
/// tarihinden itibaren yeni bir `schedule_set{source:"copied"}` sürümü açar.
/// Hedefte zaten bir program varsa `d`'den itibaren üstüne yazılır — bu,
/// projeksiyonun snapshot-akış katlama kuralının kendisidir, ayrı bir kod
/// gerekmez (R2b brief madde 1).
pub(super) fn copy_schedules_from_term(ctx: &DecisionContext, req: &ChangeRequest, _from_term: &str) -> Result<Decision, Rejection> {
    let d = ctx.term.resolve_effective_date(req.effective_date, ctx.today)?;
    if ctx.source_schedules.is_empty() {
        return Err(Rejection::new(RejectionCode::FactNotTrueAtDate, "Kaynak dönemde kopyalanacak program yok.".to_string()));
    }

    let mut events = Vec::new();
    let mut impact = ImpactSummary::empty(d, ctx.term.is_planning(ctx.today));
    for (&teacher_id, schedule) in &ctx.source_schedules {
        let label = ctx.teacher_label(teacher_id);
        let prior = ctx.state_before::<WeeklySchedule>(Stream::TeacherSchedule, teacher_id, d, apply_schedule);
        let event = PlannedEvent {
            stream: Stream::TeacherSchedule,
            subject_id: teacher_id,
            effective_date: d,
            payload: EventPayload::ScheduleSet {
                schedule: schedule.clone(),
                previous_slot_count: prior.as_ref().map(|p| p.0.len() as i64),
                source: "copied".to_string(),
                labels: labels(&[("teacher", &label)]),
            },
            caused_by: None,
            revokes: None,
        };
        impact.primary.push(impact_line(&label, &event, debug_opt(prior.as_ref()), Some(format!("{schedule:?}"))));
        apply_shadow_and_future_notices(ctx, Stream::TeacherSchedule, teacher_id, &label, d, &mut impact);
        events.push(event);
    }

    let augmented = with_pending(ctx, &events);
    for &teacher_id in ctx.source_schedules.keys() {
        impact.warnings.extend(teacher_schedule_warnings(&augmented, teacher_id, d));
    }

    finish(ctx, req, d, events, impact, Vec::new())
}

pub(super) fn delete_teacher(ctx: &DecisionContext, req: &ChangeRequest, teacher_id: i64) -> Result<Decision, Rejection> {
    ctx.require_teacher(teacher_id)?;
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
    use std::collections::BTreeSet;

    use super::super::test_support::*;
    use super::*;
    use crate::domain::history::decide::{ChangeCommand, NewTeacherProfile};
    use crate::domain::history::events::{CoordinationState, HoursState, Labels};
    use crate::domain::history::impact::WarningCode;
    use crate::domain::models::{ChiefType, EmploymentType};

    fn sample_load() -> TeacherLoad {
        TeacherLoad { base_hours: 15, max_extra_hours: 24, other_extra_hours: 0, chief_type: ChiefType::None, employment_type: EmploymentType::Tenured }
    }

    fn sample_teacher() -> NewTeacherProfile {
        NewTeacherProfile { first_name: "Ali".into(), last_name: "Veli".into(), registry_no: "1".into(), field: "Elektrik".into(), branches: vec![], is_active: true }
    }

    fn hours_state_for(awarded: i64) -> HoursState {
        HoursState { awarded_hours: awarded, max_hours_snapshot: awarded, is_honorary: false, is_locked: false, notes: String::new() }
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

    #[test]
    fn create_teacher_without_materialized_id_is_invalid_request() {
        let today = ymd(2026, 8, 1);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31))).build();
        let req = ChangeRequest {
            term: "2026-2027/1".into(),
            effective_date: None,
            document_date: None,
            reason: "yeni öğretmen".into(),
            command: ChangeCommand::CreateTeacher { teacher: sample_teacher(), load: sample_load() },
        };
        let result = create_teacher(&ctx, &req, &sample_teacher(), &sample_load());
        assert_eq!(result.unwrap_err().code, RejectionCode::InvalidRequest, "materialized.teacher_id None ise 0'a düşmek yerine reddedilmeli");
    }

    /// 24/10/4 → kapasite 10 (bölüm şefi: min(20, 24-10-4)); öğretmenin bu
    /// tarihte zaten 12 saati atanmışsa `CapacityExceeded` bayrağı çıkmalı
    /// (R2b brief madde 2, `capacity_flags` artık `decide` üzerinden çağrılır).
    #[test]
    fn set_teacher_load_flags_capacity_exceeded_via_decide() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_teacher(5, "Ali Öğretmen")
            .with_company(1, "İşletme A", Some(10.0))
            .with_event(stored_event(1, 1, Stream::TeacherLoad, 5, "2026-2027/1", ymd(2026, 9, 1), EventPayload::LoadSet { load: sample_load(), previous: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(2, 1, Stream::Coordination, 1, "2026-2027/1", ymd(2026, 9, 1), EventPayload::CoordinatorAssigned { state: CoordinationState { teacher_id: 5, visit_day: 1, visit_hour: 3, is_forced: false, force_reason: None }, from_teacher_id: None, labels: Labels(Default::default()) }, true))
            .with_event(stored_event(3, 1, Stream::CompanyHours, 1, "2026-2027/1", ymd(2026, 9, 1), EventPayload::HoursSet { state: hours_state_for(12), previous_awarded: None, labels: Labels(Default::default()) }, true))
            .build();

        let new_load = TeacherLoad { base_hours: 15, max_extra_hours: 24, other_extra_hours: 4, chief_type: ChiefType::Department, employment_type: EmploymentType::Tenured };
        let req = ChangeRequest { term: "2026-2027/1".into(), effective_date: Some(ymd(2026, 11, 5)), document_date: None, reason: "ek görev".into(), command: ChangeCommand::SetTeacherLoad { teacher_id: 5, load: new_load.clone() } };
        let decision = set_teacher_load(&ctx, &req, 5, &new_load).unwrap();

        let warning = decision.impact.warnings.iter().find(|w| w.code == WarningCode::CapacityExceeded).expect("kapasite uyarısı beklenir");
        assert_eq!(warning.from_date, ymd(2026, 11, 5));
    }

    /// Boş saat Pazartesi'den Salı'ya taşınınca, hâlâ Pazartesi'de duran
    /// zorlanmamış ziyaret bloğu artık boş saatlerin dışında kalmalı.
    #[test]
    fn set_teacher_schedule_flags_block_outside_free_slots_via_decide() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_teacher(5, "Ali Öğretmen")
            .with_company(1, "İşletme A", Some(10.0))
            .with_event(stored_event(1, 1, Stream::TeacherSchedule, 5, "2026-2027/1", ymd(2026, 9, 1), EventPayload::ScheduleSet { schedule: WeeklySchedule(BTreeSet::from([Slot::new(1, 9), Slot::new(1, 10)])), previous_slot_count: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(2, 1, Stream::Coordination, 1, "2026-2027/1", ymd(2026, 9, 1), EventPayload::CoordinatorAssigned { state: CoordinationState { teacher_id: 5, visit_day: 1, visit_hour: 9, is_forced: false, force_reason: None }, from_teacher_id: None, labels: Labels(Default::default()) }, true))
            .with_event(stored_event(3, 1, Stream::CompanyHours, 1, "2026-2027/1", ymd(2026, 9, 1), EventPayload::HoursSet { state: hours_state_for(2), previous_awarded: None, labels: Labels(Default::default()) }, true))
            .build();

        let new_slots = vec![Slot::new(2, 9)]; // Pazartesi artık boş değil
        let req = ChangeRequest { term: "2026-2027/1".into(), effective_date: Some(ymd(2026, 11, 5)), document_date: None, reason: "program".into(), command: ChangeCommand::SetTeacherSchedule { teacher_id: 5, slots: new_slots.clone() } };
        let decision = set_teacher_schedule(&ctx, &req, 5, &new_slots).unwrap();

        let warning = decision.impact.warnings.iter().find(|w| w.code == WarningCode::BlockOutsideFreeSlots).expect("blok uyarısı beklenir");
        assert_eq!(warning.from_date, ymd(2026, 11, 5));
    }

    #[test]
    fn copy_schedules_from_term_without_source_is_fact_not_true_at_date() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31))).build();
        let req = ChangeRequest {
            term: "2026-2027/1".into(),
            effective_date: Some(ymd(2026, 11, 5)),
            document_date: None,
            reason: "kopyala".into(),
            command: ChangeCommand::CopySchedulesFromTerm { from_term: "2025-2026/1".into() },
        };
        let result = copy_schedules_from_term(&ctx, &req, "2025-2026/1");
        assert_eq!(result.unwrap_err().code, RejectionCode::FactNotTrueAtDate, "kaynak dönemde program yoksa olaysız 'başarılı' dönmemeli");
    }

    #[test]
    fn copy_schedules_from_term_emits_schedule_set_for_each_teacher_and_flags() {
        let today = ymd(2026, 11, 10);
        let source_schedule = WeeklySchedule(BTreeSet::from([Slot::new(2, 9)]));
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_teacher(5, "Ali Öğretmen")
            .with_company(1, "İşletme A", Some(10.0))
            .with_event(stored_event(1, 1, Stream::Coordination, 1, "2026-2027/1", ymd(2026, 9, 1), EventPayload::CoordinatorAssigned { state: CoordinationState { teacher_id: 5, visit_day: 1, visit_hour: 9, is_forced: false, force_reason: None }, from_teacher_id: None, labels: Labels(Default::default()) }, true))
            .with_event(stored_event(2, 1, Stream::CompanyHours, 1, "2026-2027/1", ymd(2026, 9, 1), EventPayload::HoursSet { state: hours_state_for(2), previous_awarded: None, labels: Labels(Default::default()) }, true))
            .with_source_schedule(5, source_schedule.clone())
            .build();

        let req = ChangeRequest {
            term: "2026-2027/1".into(),
            effective_date: Some(ymd(2026, 11, 5)),
            document_date: None,
            reason: "kopyala".into(),
            command: ChangeCommand::CopySchedulesFromTerm { from_term: "2025-2026/1".into() },
        };
        let decision = copy_schedules_from_term(&ctx, &req, "2025-2026/1").unwrap();

        assert_eq!(decision.events.len(), 1, "bağlamdaki tek öğretmen için tek schedule_set olayı üretilmeli");
        match &decision.events[0].payload {
            EventPayload::ScheduleSet { schedule, source, .. } => {
                assert_eq!(schedule, &source_schedule);
                assert_eq!(source, "copied");
            }
            other => panic!("beklenmedik olay: {other:?}"),
        }
        assert!(
            decision.impact.warnings.iter().any(|w| w.code == WarningCode::BlockOutsideFreeSlots),
            "kopyalanan program eski koordinatörlük bloğunu artık kapsamıyor"
        );
    }

    /// Anlık görüntü akışında (`teacher_load`) geçmiş tarihli bir giriş,
    /// öznenin SONRAKİ kaydına kadar geçerlidir (spec §5.2, R2b brief
    /// madde 4). `shadowed_until` o sonraki kaydın tarihine eşit olmalı.
    #[test]
    fn backdated_load_change_reports_shadowed_until_next_record() {
        use crate::domain::history::impact::NoticeCode;

        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_teacher(5, "Ali Öğretmen")
            .with_event(stored_event(1, 1, Stream::TeacherLoad, 5, "2026-2027/1", ymd(2026, 9, 1), EventPayload::LoadSet { load: sample_load(), previous: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(2, 2, Stream::TeacherLoad, 5, "2026-2027/1", ymd(2026, 12, 1), EventPayload::LoadSet { load: sample_load(), previous: None, source: "manual".into(), labels: Labels(Default::default()) }, false))
            .build();

        let mut new_load = sample_load();
        new_load.other_extra_hours = 2;
        let req = ChangeRequest { term: "2026-2027/1".into(), effective_date: Some(ymd(2026, 11, 5)), document_date: None, reason: "geçmiş tarihli düzeltme".into(), command: ChangeCommand::SetTeacherLoad { teacher_id: 5, load: new_load.clone() } };
        let decision = set_teacher_load(&ctx, &req, 5, &new_load).unwrap();

        assert_eq!(decision.impact.shadowed_until, Some(ymd(2026, 12, 1)));
        assert!(decision.impact.notices.iter().any(|n| n.code == NoticeCode::Shadowed));
    }

    /// Yürürlük tarihi bugünden SONRAYSA `futureDated` bildirimi eklenir
    /// (R2b brief madde 4).
    #[test]
    fn future_date_adds_future_dated_notice() {
        use crate::domain::history::impact::NoticeCode;

        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_teacher(5, "Ali Öğretmen")
            .with_event(stored_event(1, 1, Stream::TeacherLoad, 5, "2026-2027/1", ymd(2026, 9, 1), EventPayload::LoadSet { load: sample_load(), previous: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .build();

        let mut new_load = sample_load();
        new_load.other_extra_hours = 2;
        let req = ChangeRequest { term: "2026-2027/1".into(), effective_date: Some(ymd(2026, 12, 15)), document_date: None, reason: "gelecek tarihli".into(), command: ChangeCommand::SetTeacherLoad { teacher_id: 5, load: new_load.clone() } };
        let decision = set_teacher_load(&ctx, &req, 5, &new_load).unwrap();

        assert!(decision.impact.notices.iter().any(|n| n.code == NoticeCode::FutureDated));
    }
}
