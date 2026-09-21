//! Öğrenci tarafı komutları: yerleştirme, nakil, ayrılış, ekleme, silme
//! (spec §5.2, §5.3, §5.4).

use chrono::NaiveDate;

use crate::domain::history::apply::apply_placement;
use crate::domain::history::events::{EventPayload, Stream, StoredEvent};
use crate::domain::history::impact::{ImpactSummary, NoticeCode, ImpactNotice};
use crate::domain::history::rejection::{Rejection, RejectionCode};
use crate::domain::history::timeline::order_events;

use super::company::build_policy_events;
use super::{
    command_kind, impact_line, labels, touched_from, with_pending, ChangeRequest, Decision, DecisionContext,
    NewChangeSet, NewStudentInput, PlannedEvent, RowAction, TransferTarget,
};

pub(super) fn create_student(
    ctx: &DecisionContext,
    req: &ChangeRequest,
    student: &NewStudentInput,
    company_id: Option<i64>,
) -> Result<Decision, Rejection> {
    let d = ctx.term.resolve_effective_date(req.effective_date, ctx.today)?;
    // Yerinde oluşturma (`students::create_in`) `decide`'dan ÖNCE, aynı
    // transaction'da çalışır; `None` sessizce `0` id'sine düşmek yerine
    // reddedilir (R2b brief madde 3) — bu her zaman çalışır, `company_id`
    // dalından bağımsızdır.
    let student_id = ctx.require_materialized_student()?;
    let label = format!("{} {}", student.first_name, student.last_name);

    let mut events = Vec::new();
    let mut impact = ImpactSummary::empty(d, ctx.term.is_planning(ctx.today));

    if let Some(company_id) = company_id {
        append_initial_placement(ctx, student_id, &label, company_id, d, &mut events, &mut impact)?;
    }

    let touched = touched_from(&events, &req.term);
    Ok(Decision {
        change_set: NewChangeSet {
            term: req.term.clone(),
            kind: command_kind(&req.command).to_string(),
            effective_date: d,
            document_date: req.document_date,
            reason: req.reason.clone(),
            revokes_change_set_id: None,
        },
        events,
        impact,
        row_actions: Vec::new(),
        touched,
    })
}

/// Yeni öğrencinin İLK yerleştirmesi ve zincir etkisi. Pasif bir işletme
/// yeni atama hedefi olamaz (spec §5.4, R2b brief madde 3).
fn append_initial_placement(
    ctx: &DecisionContext,
    student_id: i64,
    label: &str,
    company_id: i64,
    d: NaiveDate,
    events: &mut Vec<PlannedEvent>,
    impact: &mut ImpactSummary,
) -> Result<(), Rejection> {
    ctx.require_active_company(company_id)?;
    let primary = PlannedEvent {
        stream: Stream::Placement,
        subject_id: student_id,
        effective_date: d,
        payload: EventPayload::StudentPlaced {
            to_company_id: company_id,
            from_company_id: None,
            source: "manual".to_string(),
            labels: labels(&[("student", label)]),
        },
        caused_by: None,
        revokes: None,
    };
    impact.primary.push(impact_line(label, &primary, None, Some(ctx.company_label(company_id))));
    events.push(primary);

    let augmented = with_pending(ctx, events.as_slice());
    let (cascade, warnings, notices) = build_policy_events(&augmented, company_id, d, 0);
    for e in &cascade {
        impact.automatic.push(impact_line(&ctx.company_label(company_id), e, None, None));
    }
    events.extend(cascade);
    impact.warnings.extend(warnings);
    impact.notices.extend(notices);
    Ok(())
}

/// Yeni öğrenci ataması `create_student`in initial placement'ı DIŞINDA, halihazırda
/// var olan bir öğrenciyi ilk kez bir işletmeye yerleştirir. `to`, nakil
/// komutuyla AYNI `resolve_transfer_target`i paylaşır (DRY): böylece bu ilk
/// yerleştirme de nakil gibi ya var olan bir işletmeyi ya da yerinde
/// oluşturulmuş yeni bir işletmeyi hedef alabilir.
pub(super) fn place_student(ctx: &DecisionContext, req: &ChangeRequest, student_id: i64, to: &TransferTarget) -> Result<Decision, Rejection> {
    let d = ctx.term.resolve_effective_date(req.effective_date, ctx.today)?;
    ctx.require_student(student_id)?;
    let (company_id, company_created) = resolve_transfer_target(ctx, to)?;
    let label = ctx.student_label(student_id);
    let candidate = EventPayload::StudentPlaced { to_company_id: company_id, from_company_id: None, source: "manual".to_string(), labels: labels(&[("student", &label)]) };
    validate_placement_change(ctx, student_id, d, None, &candidate)?;

    let primary = PlannedEvent { stream: Stream::Placement, subject_id: student_id, effective_date: d, payload: candidate, caused_by: None, revokes: None };
    let mut impact = ImpactSummary::empty(d, ctx.term.is_planning(ctx.today));
    impact.primary.push(impact_line(&label, &primary, None, Some(ctx.company_label(company_id))));
    let mut events = vec![primary];

    let augmented = with_pending(ctx, &events);
    let (cascade, warnings, notices) = build_policy_events(&augmented, company_id, d, 0);
    for e in &cascade {
        impact.automatic.push(impact_line(&ctx.company_label(company_id), e, None, None));
    }
    events.extend(cascade);
    impact.warnings.extend(warnings);
    impact.notices.extend(notices);

    // Nakildeki `NewCompanyNeedsSetup` bildirimiyle AYNI gerekçe (DRY): yeni
    // işletme kurulduysa kullanıcı saat/atama girmesi gerektiğini görmeli.
    if company_created {
        impact.notices.push(ImpactNotice {
            code: NoticeCode::NewCompanyNeedsSetup,
            message: format!("{}: yeni işletme oluşturuldu; saat ve atama elle girilmeli", ctx.company_label(company_id)),
            subject_label: ctx.company_label(company_id),
            date: d,
        });
    }

    finish(ctx, req, d, events, impact, Vec::new())
}

pub(super) fn transfer_student(
    ctx: &DecisionContext,
    req: &ChangeRequest,
    student_id: i64,
    from_company_id: i64,
    to: &TransferTarget,
) -> Result<Decision, Rejection> {
    let d = ctx.term.resolve_effective_date(req.effective_date, ctx.today)?;
    ctx.require_student(student_id)?;
    ctx.require_company(from_company_id)?;
    let (to_company_id, to_company_created) = resolve_transfer_target(ctx, to)?;

    let label = ctx.student_label(student_id);
    let candidate = EventPayload::StudentTransferred {
        from_company_id,
        to_company_id,
        to_company_created,
        labels: labels(&[("student", &label)]),
    };
    validate_placement_change(ctx, student_id, d, Some(from_company_id), &candidate)?;

    let primary = PlannedEvent { stream: Stream::Placement, subject_id: student_id, effective_date: d, payload: candidate, caused_by: None, revokes: None };
    let mut impact = ImpactSummary::empty(d, ctx.term.is_planning(ctx.today));
    impact.primary.push(impact_line(&label, &primary, Some(ctx.company_label(from_company_id)), Some(ctx.company_label(to_company_id))));
    let mut events = vec![primary];

    append_transfer_cascade(ctx, from_company_id, to_company_id, to_company_created, d, &mut events, &mut impact);

    finish(ctx, req, d, events, impact, Vec::new())
}

/// `to`'nun hedef işletme kimliğini çözer: var olan bir işletme (aktif
/// olmalı) ya da yerinde oluşturulmuş yeni bir işletme. `None` sessizce
/// `0` id'sine düşmek yerine reddedilir (R2b brief madde 3).
fn resolve_transfer_target(ctx: &DecisionContext, to: &TransferTarget) -> Result<(i64, bool), Rejection> {
    match to {
        TransferTarget::Existing { company_id } => {
            ctx.require_active_company(*company_id)?;
            Ok((*company_id, false))
        }
        // Yeni işletme `decide`'dan ÖNCE `companies::create_in` ile açılmıştır.
        TransferTarget::New { .. } => Ok((ctx.require_materialized_company()?, true)),
    }
}

/// Eski VE yeni işletmenin zincir etkisi, artı yerinde oluşturulan işletme
/// için kurulum bildirimi.
fn append_transfer_cascade(
    ctx: &DecisionContext,
    from_company_id: i64,
    to_company_id: i64,
    to_company_created: bool,
    d: NaiveDate,
    events: &mut Vec<PlannedEvent>,
    impact: &mut ImpactSummary,
) {
    let augmented = with_pending(ctx, events.as_slice());
    for company_id in [from_company_id, to_company_id] {
        let (cascade, warnings, notices) = build_policy_events(&augmented, company_id, d, 0);
        for e in &cascade {
            impact.automatic.push(impact_line(&ctx.company_label(company_id), e, None, None));
        }
        events.extend(cascade);
        impact.warnings.extend(warnings);
        impact.notices.extend(notices);
    }

    if to_company_created {
        impact.notices.push(ImpactNotice {
            code: NoticeCode::NewCompanyNeedsSetup,
            message: format!("{}: yeni işletme oluşturuldu; saat ve atama elle girilmeli", ctx.company_label(to_company_id)),
            subject_label: ctx.company_label(to_company_id),
            date: d,
        });
    }
}

pub(super) fn student_leaves(ctx: &DecisionContext, req: &ChangeRequest, student_id: i64, from_company_id: i64) -> Result<Decision, Rejection> {
    let d = ctx.term.resolve_effective_date(req.effective_date, ctx.today)?;
    ctx.require_student(student_id)?;
    ctx.require_company(from_company_id)?;
    let label = ctx.student_label(student_id);
    let candidate = EventPayload::StudentLeft { from_company_id, labels: labels(&[("student", &label)]) };
    validate_placement_change(ctx, student_id, d, Some(from_company_id), &candidate)?;

    let primary = PlannedEvent { stream: Stream::Placement, subject_id: student_id, effective_date: d, payload: candidate, caused_by: None, revokes: None };
    let mut impact = ImpactSummary::empty(d, ctx.term.is_planning(ctx.today));
    impact.primary.push(impact_line(&label, &primary, Some(ctx.company_label(from_company_id)), None));
    let mut events = vec![primary];

    let augmented = with_pending(ctx, &events);
    let (cascade, warnings, notices) = build_policy_events(&augmented, from_company_id, d, 0);
    for e in &cascade {
        impact.automatic.push(impact_line(&ctx.company_label(from_company_id), e, None, None));
    }
    events.extend(cascade);
    impact.warnings.extend(warnings);
    impact.notices.extend(notices);

    finish(ctx, req, d, events, impact, Vec::new())
}

pub(super) fn delete_student(ctx: &DecisionContext, req: &ChangeRequest, student_id: i64) -> Result<Decision, Rejection> {
    ctx.require_student(student_id)?;
    let placement_events = ctx.events_for(Stream::Placement, student_id);
    let live = |e: &&StoredEvent| !matches!(e.payload, EventPayload::Revoked);
    let has_history = placement_events.iter().any(|e| !e.is_opening && live(&e));
    if has_history {
        return Err(Rejection::new(
            RejectionCode::HasHistory,
            format!("{}: açılış dışında geçmişi var; silinemez, pasif yapılabilir.", ctx.student_label(student_id)),
        ));
    }

    let opening_events: Vec<&StoredEvent> = placement_events.iter().filter(|e| e.is_opening && live(e)).collect();
    let d = ctx.term.start;
    let label = ctx.student_label(student_id);
    let events: Vec<PlannedEvent> = opening_events
        .iter()
        .map(|e| PlannedEvent { stream: Stream::Placement, subject_id: student_id, effective_date: d, payload: EventPayload::Revoked, caused_by: None, revokes: Some(e.id) })
        .collect();

    let mut impact = ImpactSummary::empty(d, ctx.term.is_planning(ctx.today));
    impact.primary.push(crate::domain::history::impact::ImpactLine {
        kind: "student_deleted".to_string(),
        stream: Stream::Placement.as_str().to_string(),
        subject_id: student_id,
        subject_label: label,
        effective_date: d,
        before: Some("kayıtlı".to_string()),
        after: None,
    });

    finish(ctx, req, d, events, impact, vec![RowAction::DeleteStudent(student_id)])
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

/// `payload`'ın beklediği önceki işletme — yalnız `StudentTransferred` ve
/// `StudentLeft` taşır (spec §4.2, "beklenen önceki değer").
pub(super) fn placement_expected_from(payload: &EventPayload) -> Option<i64> {
    match payload {
        EventPayload::StudentTransferred { from_company_id, .. } => Some(*from_company_id),
        EventPayload::StudentLeft { from_company_id, .. } => Some(*from_company_id),
        _ => None,
    }
}

/// Bir yerleştirme değişikliğini iki adımda doğrular (spec §5.2):
/// 1. `d⁻` anındaki durum beklenenle eşleşmeli (`FactNotTrueAtDate`).
/// 2. Aday, SONRAKİ bir kaydın beklediği önceki işletmeyi bozmamalı
///    (`ConflictsWithLaterChange`).
fn validate_placement_change(
    ctx: &DecisionContext,
    student_id: i64,
    d: NaiveDate,
    expected_prior: Option<i64>,
    candidate: &EventPayload,
) -> Result<(), Rejection> {
    let prior = ctx.state_before::<i64>(Stream::Placement, student_id, d, apply_placement);
    if prior != expected_prior {
        return Err(Rejection::new(
            RejectionCode::FactNotTrueAtDate,
            format!("{}: {d} tarihinde beklenen işletmede değil.", ctx.student_label(student_id)),
        ));
    }
    check_no_later_placement_conflict(ctx, student_id, d, candidate)
}

/// Aday olayı zaman çizelgesine geçici olarak ekler ve SONRAKİ, `from_*`
/// bekleyen kayıtları yeniden doğrular. İlk çelişkide çelişen kümeyi adıyla
/// `ConflictsWithLaterChange` döner; otomatik yeniden sıralama YAPILMAZ
/// (spec §5.2).
pub(super) fn check_no_later_placement_conflict(
    ctx: &DecisionContext,
    student_id: i64,
    candidate_date: NaiveDate,
    candidate_payload: &EventPayload,
) -> Result<(), Rejection> {
    const CANDIDATE_ID: i64 = i64::MAX;
    let existing = ctx.events_for(Stream::Placement, student_id);
    let mut merged = existing.clone();
    merged.push(StoredEvent {
        id: CANDIDATE_ID,
        change_set_id: -1,
        stream: Stream::Placement,
        subject_id: student_id,
        term: ctx.term.term.clone(),
        effective_date: candidate_date,
        payload: candidate_payload.clone(),
        caused_by: None,
        revokes: None,
        is_opening: false,
    });

    let ordered = order_events(&merged, ctx.term.start);
    let mut state: Option<i64> = None;
    for te in &ordered {
        if te.id != CANDIDATE_ID {
            if let Some(expected_from) = placement_expected_from(&te.payload) {
                if te.effective_date > candidate_date && state != Some(expected_from) {
                    let conflicting = existing.iter().find(|e| e.id == te.id).map(|e| e.change_set_id).unwrap_or(0);
                    return Err(Rejection::new(
                        RejectionCode::ConflictsWithLaterChange,
                        format!("Bu değişiklik, #{conflicting} numaralı sonraki kayıtla çelişiyor: o kayıt farklı bir önceki işletme bekliyor."),
                    )
                    .with_conflicts(vec![conflicting]));
                }
            }
        }
        state = apply_placement(state.as_ref(), &te.payload);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use crate::domain::history::decide::{ChangeCommand, Materialized};
    use crate::domain::history::events::Labels;
    use crate::domain::models::NewCompany;

    fn opened(id: i64, company_id: i64, at: NaiveDate) -> StoredEvent {
        stored_event(id, id, Stream::Placement, 100, "2026-2027/1", at, EventPayload::StudentPlaced { to_company_id: company_id, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true)
    }

    fn request(effective_date: NaiveDate, command: ChangeCommand) -> ChangeRequest {
        ChangeRequest { term: "2026-2027/1".into(), effective_date: Some(effective_date), document_date: None, reason: "test".into(), command }
    }

    #[test]
    fn transfer_to_existing_company_emits_transfer_and_cap() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_rules(vec![rule(1, 0.0, None, 1, Some(1), 6), rule(2, 0.0, None, 2, None, 4)])
            .with_company(1, "İşletme A", Some(10.0))
            .with_company(2, "İşletme B", Some(10.0))
            .with_student(100, "Ahmet Yılmaz")
            .with_event(opened(1, 1, ymd(2026, 9, 1)))
            .with_event(stored_event(2, 1, Stream::Placement, 200, "2026-2027/1", ymd(2026, 9, 1), EventPayload::StudentPlaced { to_company_id: 2, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(3, 2, Stream::CompanyHours, 2, "2026-2027/1", ymd(2026, 9, 1), EventPayload::HoursSet { state: hours_state_for_test(6), previous_awarded: None, labels: Labels(Default::default()) }, true))
            .build();

        let req = request(ymd(2026, 11, 3), ChangeCommand::TransferStudent { student_id: 100, from_company_id: 1, to: TransferTarget::Existing { company_id: 2 } });
        let decision = transfer_student(&ctx, &req, 100, 1, &TransferTarget::Existing { company_id: 2 }).unwrap();

        assert!(matches!(decision.events[0].payload, EventPayload::StudentTransferred { .. }));
        let capped = decision.events.iter().find(|e| matches!(e.payload, EventPayload::HoursCapped { .. }));
        assert!(capped.is_some(), "2 öğrenciye düşen tavan (4), B'nin 6 saatini aşmalı ve otomatik düşürmeli");
    }

    #[test]
    fn transfer_to_new_company_uses_materialized_id_and_notices_setup() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_company(1, "İşletme A", Some(10.0))
            .with_company(9, "Yeni İşletme", None)
            .with_student(100, "Ahmet Yılmaz")
            .with_event(opened(1, 1, ymd(2026, 9, 1)))
            .materialized(Materialized { company_id: Some(9), student_id: None, teacher_id: None })
            .build();

        let new_company = NewCompany {
            name: "Yeni İşletme".into(), contact_first_name: String::new(), contact_last_name: String::new(),
            phone: String::new(), email: String::new(), address_text: String::new(), latitude: None,
            longitude: None, one_way_distance_km: None, district: String::new(), notes: String::new(),
        };
        let to = TransferTarget::New { company: new_company };
        let req = request(ymd(2026, 11, 3), ChangeCommand::TransferStudent { student_id: 100, from_company_id: 1, to: to.clone() });
        let decision = transfer_student(&ctx, &req, 100, 1, &to).unwrap();

        match &decision.events[0].payload {
            EventPayload::StudentTransferred { to_company_id, to_company_created, .. } => {
                assert_eq!(*to_company_id, 9);
                assert!(*to_company_created);
            }
            other => panic!("beklenmedik olay: {other:?}"),
        }
        assert!(decision.impact.notices.iter().any(|n| n.code == NoticeCode::NewCompanyNeedsSetup));
    }

    #[test]
    fn transfer_rejected_when_student_not_at_from_company_on_date() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_company(1, "İşletme A", Some(10.0))
            .with_company(2, "İşletme B", Some(10.0))
            .with_student(100, "Ahmet Yılmaz")
            .with_event(opened(1, 2, ymd(2026, 9, 1))) // öğrenci aslında B'de
            .build();

        let req = request(ymd(2026, 11, 3), ChangeCommand::TransferStudent { student_id: 100, from_company_id: 1, to: TransferTarget::Existing { company_id: 2 } });
        let result = transfer_student(&ctx, &req, 100, 1, &TransferTarget::Existing { company_id: 2 });
        assert_eq!(result.unwrap_err().code, RejectionCode::FactNotTrueAtDate);
    }

    #[test]
    fn transfer_rejected_when_later_leave_expects_old_company() {
        let today = ymd(2026, 11, 10);
        let later_leave_id = 50;
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_company(1, "İşletme A", Some(10.0))
            .with_company(2, "İşletme B", Some(10.0))
            .with_student(100, "Ahmet Yılmaz")
            .with_event(opened(1, 1, ymd(2026, 9, 1)))
            .with_event(stored_event(later_leave_id, later_leave_id, Stream::Placement, 100, "2026-2027/1", ymd(2026, 12, 1), EventPayload::StudentLeft { from_company_id: 1, labels: Labels(Default::default()) }, false))
            .build();

        let req = request(ymd(2026, 11, 3), ChangeCommand::TransferStudent { student_id: 100, from_company_id: 1, to: TransferTarget::Existing { company_id: 2 } });
        let result = transfer_student(&ctx, &req, 100, 1, &TransferTarget::Existing { company_id: 2 });
        let rejection = result.unwrap_err();
        assert_eq!(rejection.code, RejectionCode::ConflictsWithLaterChange);
        assert_eq!(rejection.conflicting_change_set_ids, vec![later_leave_id]);
    }

    #[test]
    fn previous_month_date_rejected_with_suggestion() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_company(1, "İşletme A", Some(10.0))
            .with_student(100, "Ahmet Yılmaz")
            .build();

        let to = TransferTarget::Existing { company_id: 1 };
        let req = request(ymd(2026, 10, 28), ChangeCommand::PlaceStudent { student_id: 100, to: to.clone() });
        let result = place_student(&ctx, &req, 100, &to);
        let rejection = result.unwrap_err();
        assert_eq!(rejection.code, RejectionCode::PreviousMonthClosed);
        assert_eq!(rejection.suggested_date, Some(ymd(2026, 11, 1)));
    }

    /// `PlaceStudent{ to: Existing }` eski davranışı korur: var olan işletmeye
    /// ilk yerleştirme.
    #[test]
    fn place_student_to_existing_company_places_the_student() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_company(1, "İşletme A", Some(10.0))
            .with_student(100, "Ahmet Yılmaz")
            .build();

        let to = TransferTarget::Existing { company_id: 1 };
        let req = request(ymd(2026, 11, 3), ChangeCommand::PlaceStudent { student_id: 100, to: to.clone() });
        let decision = place_student(&ctx, &req, 100, &to).unwrap();

        match &decision.events[0].payload {
            EventPayload::StudentPlaced { to_company_id, from_company_id, .. } => {
                assert_eq!(*to_company_id, 1);
                assert_eq!(*from_company_id, None);
            }
            other => panic!("beklenmedik olay: {other:?}"),
        }
    }

    /// `PlaceStudent{ to: New }` yerinde oluşturulmuş yeni bir işletmeye
    /// yerleştirir ve kurulum bildirimi üretir — `transfer_to_new_company_...`
    /// testiyle AYNI mekanizma (`resolve_transfer_target`).
    #[test]
    fn place_student_to_new_company_uses_materialized_id_and_notices_setup() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_company(9, "Yeni İşletme", None)
            .with_student(100, "Ahmet Yılmaz")
            .materialized(Materialized { company_id: Some(9), student_id: None, teacher_id: None })
            .build();

        let new_company = NewCompany {
            name: "Yeni İşletme".into(), contact_first_name: String::new(), contact_last_name: String::new(),
            phone: String::new(), email: String::new(), address_text: String::new(), latitude: None,
            longitude: None, one_way_distance_km: None, district: String::new(), notes: String::new(),
        };
        let to = TransferTarget::New { company: new_company };
        let req = request(ymd(2026, 11, 3), ChangeCommand::PlaceStudent { student_id: 100, to: to.clone() });
        let decision = place_student(&ctx, &req, 100, &to).unwrap();

        match &decision.events[0].payload {
            EventPayload::StudentPlaced { to_company_id, .. } => assert_eq!(*to_company_id, 9),
            other => panic!("beklenmedik olay: {other:?}"),
        }
        assert!(decision.impact.notices.iter().any(|n| n.code == NoticeCode::NewCompanyNeedsSetup));
    }

    /// Regresyon koruması: yerleştirmesi ZATEN açık olan bir öğrenciye
    /// `PlaceStudent` reddedilir (`validate_placement_change`in `d⁻` anındaki
    /// durum denetimi — spec §5.2). Bu davranış `PlaceStudent`in `to:
    /// TransferTarget` şekline geçmesiyle değişmemeli.
    #[test]
    fn place_student_already_placed_is_rejected() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_company(1, "İşletme A", Some(10.0))
            .with_company(2, "İşletme B", Some(10.0))
            .with_student(100, "Ahmet Yılmaz")
            .with_event(opened(1, 1, ymd(2026, 9, 1)))
            .build();

        let to = TransferTarget::Existing { company_id: 2 };
        let req = request(ymd(2026, 11, 3), ChangeCommand::PlaceStudent { student_id: 100, to: to.clone() });
        let result = place_student(&ctx, &req, 100, &to);
        assert_eq!(result.unwrap_err().code, RejectionCode::FactNotTrueAtDate, "zaten yerleşik bir öğrenci yeniden 'ilk' yerleştirilemez");
    }

    #[test]
    fn delete_student_with_history_rejected() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_company(1, "İşletme A", Some(10.0))
            .with_company(2, "İşletme B", Some(10.0))
            .with_student(100, "Ahmet Yılmaz")
            .with_event(opened(1, 1, ymd(2026, 9, 1)))
            .with_event(stored_event(2, 2, Stream::Placement, 100, "2026-2027/1", ymd(2026, 10, 1), EventPayload::StudentTransferred { from_company_id: 1, to_company_id: 2, to_company_created: false, labels: Labels(Default::default()) }, false))
            .build();

        let req = ChangeRequest { term: "2026-2027/1".into(), effective_date: None, document_date: None, reason: "sil".into(), command: ChangeCommand::DeleteStudent { student_id: 100 } };
        let result = delete_student(&ctx, &req, 100);
        assert_eq!(result.unwrap_err().code, RejectionCode::HasHistory);
    }

    fn hours_state_for_test(awarded: i64) -> crate::domain::history::events::HoursState {
        crate::domain::history::events::HoursState { awarded_hours: awarded, max_hours_snapshot: awarded, is_honorary: false, is_locked: false, notes: String::new() }
    }

    /// `materialized.student_id` `None` gelirse (yerinde oluşturma başarısız
    /// oldu ya da hiç çalışmadı), önceden `unwrap_or_default()` ile `0`
    /// id'sine düşüp sahte bir öğrenciyle devam ediyordu (R2b brief madde 3).
    #[test]
    fn create_student_without_materialized_id_is_invalid_request() {
        let today = ymd(2026, 8, 1);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_company(1, "İşletme A", Some(10.0))
            .build();

        let student = NewStudentInput { first_name: "Ahmet".into(), last_name: "Yılmaz".into(), student_no: None, grade: "12".into(), branch: "A".into(), submitted_at: None };
        let req = ChangeRequest { term: "2026-2027/1".into(), effective_date: None, document_date: None, reason: "yeni öğrenci".into(), command: ChangeCommand::CreateStudent { student: student.clone(), company_id: Some(1) } };
        let result = create_student(&ctx, &req, &student, Some(1));
        assert_eq!(result.unwrap_err().code, RejectionCode::InvalidRequest);
    }

    #[test]
    fn transfer_to_unknown_company_is_invalid_request() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_company(1, "İşletme A", Some(10.0))
            .with_student(100, "Ahmet Yılmaz")
            .with_event(opened(1, 1, ymd(2026, 9, 1)))
            .build();

        let req = request(ymd(2026, 11, 3), ChangeCommand::TransferStudent { student_id: 100, from_company_id: 1, to: TransferTarget::Existing { company_id: 999 } });
        let result = transfer_student(&ctx, &req, 100, 1, &TransferTarget::Existing { company_id: 999 });
        assert_eq!(result.unwrap_err().code, RejectionCode::InvalidRequest, "bağlamda olmayan bir işletme id'si ile sessizce devam edilmemeli");
    }

    #[test]
    fn transfer_to_inactive_company_is_invalid_request() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_company(1, "İşletme A", Some(10.0))
            .with_inactive_company(2, "Kapanmış İşletme", Some(10.0))
            .with_student(100, "Ahmet Yılmaz")
            .with_event(opened(1, 1, ymd(2026, 9, 1)))
            .build();

        let req = request(ymd(2026, 11, 3), ChangeCommand::TransferStudent { student_id: 100, from_company_id: 1, to: TransferTarget::Existing { company_id: 2 } });
        let result = transfer_student(&ctx, &req, 100, 1, &TransferTarget::Existing { company_id: 2 });
        assert_eq!(result.unwrap_err().code, RejectionCode::InvalidRequest, "pasif işletme nakil hedefi olamaz");
    }
}
