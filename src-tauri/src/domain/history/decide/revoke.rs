//! Geri alma ve düzeltme (spec §5.4). İkisi de TEK bir karardır: geri alma
//! olayları ve (varsa) yeni olaylar aynı `Decision`'da birlikte üretilir.

use std::collections::BTreeSet;

use crate::domain::history::apply::apply_coordination;
use crate::domain::history::events::{CoordinationState, EventPayload, Stream, StoredEvent};
use crate::domain::history::impact::{ImpactLine, ImpactSummary};
use crate::domain::history::rejection::{Rejection, RejectionCode};
use crate::domain::history::timeline::order_events;

use super::student::placement_expected_from;
use super::{touched_from, ChangeCommand, ChangeRequest, Decision, DecisionContext, NewChangeSet, PlannedEvent, RowAction};

pub(super) fn revoke(ctx: &DecisionContext, req: &ChangeRequest, target_id: i64) -> Result<Decision, Rejection> {
    build_revoke_decision(ctx, req, target_id)
}

pub(super) fn correct(ctx: &DecisionContext, req: &ChangeRequest, target_id: i64, replacement: &ChangeCommand) -> Result<Decision, Rejection> {
    if matches!(replacement, ChangeCommand::Revoke { .. } | ChangeCommand::Correct { .. }) {
        return Err(Rejection::new(
            RejectionCode::NotRevocable,
            "Düzeltmenin yerine geçen komut geri alma ya da başka bir düzeltme olamaz.".to_string(),
        ));
    }

    let revoke_decision = build_revoke_decision(ctx, req, target_id)?;
    let replacement_decision = decide_replacement(ctx, req, target_id, replacement)?;
    Ok(merge_into_correction(req, target_id, revoke_decision, replacement_decision))
}

/// Düzeltilen komut, hedef küme HİÇ OLMAMIŞ gibi bir bağlamda değerlendirilir:
/// böylece "yanlış nakli düzelt" gibi işlemler eski duruma başvurabilir.
fn decide_replacement(ctx: &DecisionContext, req: &ChangeRequest, target_id: i64, replacement: &ChangeCommand) -> Result<Decision, Rejection> {
    let mut adjusted_ctx = ctx.clone();
    adjusted_ctx.events.retain(|e| e.change_set_id != target_id);
    let replacement_req = ChangeRequest {
        term: req.term.clone(),
        effective_date: req.effective_date,
        document_date: req.document_date,
        reason: req.reason.clone(),
        command: replacement.clone(),
    };
    super::decide(&adjusted_ctx, &replacement_req)
}

/// Geri alma olaylarını ve yeni olayları TEK bir karara birleştirir; kümenin
/// `kind`'ı `correct` olur ve `revokes_change_set_id` dolu gelir (spec §5.4).
fn merge_into_correction(req: &ChangeRequest, target_id: i64, revoke_decision: Decision, replacement_decision: Decision) -> Decision {
    let offset = revoke_decision.events.len();
    let mut events = revoke_decision.events;
    events.extend(replacement_decision.events.into_iter().map(|mut e| {
        e.caused_by = e.caused_by.map(|idx| idx + offset);
        e
    }));

    let mut impact = revoke_decision.impact;
    impact.primary.extend(replacement_decision.impact.primary);
    impact.automatic.extend(replacement_decision.impact.automatic);
    impact.warnings.extend(replacement_decision.impact.warnings);
    impact.notices.extend(replacement_decision.impact.notices);

    let mut row_actions = revoke_decision.row_actions;
    row_actions.extend(replacement_decision.row_actions);

    let mut touched = revoke_decision.touched;
    for key in replacement_decision.touched {
        if !touched.contains(&key) {
            touched.push(key);
        }
    }

    Decision {
        change_set: NewChangeSet {
            term: req.term.clone(),
            kind: "correct".to_string(),
            effective_date: revoke_decision.change_set.effective_date,
            document_date: req.document_date,
            reason: req.reason.clone(),
            revokes_change_set_id: Some(target_id),
        },
        events,
        impact,
        row_actions,
        touched,
    }
}

fn build_revoke_decision(ctx: &DecisionContext, req: &ChangeRequest, target_id: i64) -> Result<Decision, Rejection> {
    let facts = validate_revocable(ctx, target_id)?;
    let family: Vec<StoredEvent> = ctx.events.iter().filter(|e| e.change_set_id == target_id && !matches!(e.payload, EventPayload::Revoked)).cloned().collect();
    let (mut events, mut impact) = revoke_family_events(ctx, &facts, &family);
    replay_policy_for_affected_companies(ctx, target_id, &facts, &family, &mut events, &mut impact);
    let row_actions: Vec<RowAction> = deactivation_row_action(ctx, &family).into_iter().collect();
    let touched = touched_from(&events, &req.term);

    Ok(Decision {
        change_set: NewChangeSet {
            term: req.term.clone(),
            kind: "revoke".to_string(),
            effective_date: facts.effective_date,
            document_date: req.document_date,
            reason: req.reason.clone(),
            revokes_change_set_id: Some(target_id),
        },
        events,
        impact,
        row_actions,
        touched,
    })
}

/// Hedef kümenin geri alınabilir olup olmadığını denetler (spec §5.4,
/// "Geri alınamayanlar" + `HasDependents`).
fn validate_revocable(ctx: &DecisionContext, target_id: i64) -> Result<super::ChangeSetFacts, Rejection> {
    let facts = ctx
        .change_sets
        .get(&target_id)
        .cloned()
        .ok_or_else(|| Rejection::new(RejectionCode::NotRevocable, format!("#{target_id} numaralı kayıt bulunamadı; geri alınamaz.")))?;

    if facts.kind == "opening" {
        return Err(Rejection::new(RejectionCode::NotRevocable, "Açılış kümesi geri alınamaz.".to_string()));
    }
    if facts.revokes_change_set_id.is_some() {
        return Err(Rejection::new(RejectionCode::NotRevocable, "Bir geri alma kümesi tekrar geri alınamaz.".to_string()));
    }
    if facts.effective_date < ctx.term.earliest_allowed(ctx.today) {
        return Err(Rejection::new(
            RejectionCode::NotRevocable,
            "Önceki ayın ek ders puantajı ilçeye gönderildi; bu kayıt artık geri alınamaz.".to_string(),
        ));
    }
    if let Some(dependent) = find_dependent(ctx, target_id) {
        return Err(Rejection::new(RejectionCode::HasDependents, format!("Önce #{dependent} numaralı bağlı kaydı geri alın.")).with_conflicts(vec![dependent]));
    }
    Ok(facts)
}

/// Hedefin her canlı olayı için bir `Revoked` işareti ve etki satırı üretir.
fn revoke_family_events(ctx: &DecisionContext, facts: &super::ChangeSetFacts, family: &[StoredEvent]) -> (Vec<PlannedEvent>, ImpactSummary) {
    let mut events = Vec::new();
    let mut impact = ImpactSummary::empty(facts.effective_date, ctx.term.is_planning(ctx.today));
    for e in family {
        events.push(PlannedEvent { stream: e.stream, subject_id: e.subject_id, effective_date: facts.effective_date, payload: EventPayload::Revoked, caused_by: None, revokes: Some(e.id) });
        impact.primary.push(ImpactLine {
            kind: "revoked".to_string(),
            stream: e.stream.as_str().to_string(),
            subject_id: e.subject_id,
            subject_label: subject_label_for(ctx, e.stream, e.subject_id),
            effective_date: facts.effective_date,
            before: Some(e.payload.kind().to_string()),
            after: None,
        });
    }
    (events, impact)
}

/// "Politika, en erken tarihten yeniden yürütülür" (spec §5.4): hedefi geri
/// almak öğrenci sayısını değiştirmiş olabilir, bu yüzden hedefin dokunduğu
/// her işletme için zincir etki YENİDEN yürütülür — hedefin KENDİ olayları
/// çıkarılmış bir bağlamda (`adjusted_ctx`). Bu, `reducedBelowCap` gibi
/// çok nedenli geri alma bildirimlerini kapsar (spec §5.3).
fn replay_policy_for_affected_companies(
    ctx: &DecisionContext,
    target_id: i64,
    facts: &super::ChangeSetFacts,
    family: &[StoredEvent],
    events: &mut Vec<PlannedEvent>,
    impact: &mut ImpactSummary,
) {
    let mut adjusted_ctx = ctx.clone();
    adjusted_ctx.events.retain(|e| e.change_set_id != target_id);
    let caused_by = 0; // ilk geri alma işaretine bağlanır; salt görüntüleme amaçlıdır.

    for company_id in affected_companies(family) {
        if !ctx.companies.contains_key(&company_id) {
            continue;
        }
        let (cascade, warnings, notices) = super::company::build_policy_events(&adjusted_ctx, company_id, facts.effective_date, caused_by);
        events.extend(cascade);
        impact.warnings.extend(warnings);
        impact.notices.extend(notices);
    }
}

/// Geri alınan ailedeki olayların değindiği işletmeler (spec §5.3, "Etkilenen
/// işletmeler": eski ve yeni işletme, geri alınan olaylarda adı geçenler).
fn affected_companies(family: &[StoredEvent]) -> BTreeSet<i64> {
    let mut companies = BTreeSet::new();
    for e in family {
        match &e.payload {
            EventPayload::StudentPlaced { to_company_id, from_company_id, .. } => {
                companies.insert(*to_company_id);
                if let Some(id) = from_company_id {
                    companies.insert(*id);
                }
            }
            EventPayload::StudentTransferred { from_company_id, to_company_id, .. } => {
                companies.insert(*from_company_id);
                companies.insert(*to_company_id);
            }
            EventPayload::StudentLeft { from_company_id, .. } => {
                companies.insert(*from_company_id);
            }
            _ if e.stream == Stream::CompanyHours || e.stream == Stream::Coordination => {
                companies.insert(e.subject_id);
            }
            _ => {}
        }
    }
    companies
}

fn subject_label_for(ctx: &DecisionContext, stream: Stream, subject_id: i64) -> String {
    match stream {
        Stream::Placement => ctx.student_label(subject_id),
        Stream::CompanyHours | Stream::Coordination => ctx.company_label(subject_id),
        Stream::TeacherLoad | Stream::TeacherSchedule => ctx.teacher_label(subject_id),
    }
}

/// Hedefi geri alınca, SONRAKİ bir kümenin `from_*` beklentisi bozuluyor mu?
/// Yalnız `placement` (nakil/ayrılış) ve `coordination` (koordinatör devri)
/// akışlarında "beklenen önceki değer" vardır; anlık görüntü akışlarında
/// (saat, yük, program) bu denetim uygulanmaz (spec §4.2, §5.4).
fn find_dependent(ctx: &DecisionContext, target_id: i64) -> Option<i64> {
    let touched: BTreeSet<(Stream, i64)> = ctx.events.iter().filter(|e| e.change_set_id == target_id).map(|e| (e.stream, e.subject_id)).collect();

    for (stream, subject_id) in touched {
        let found = match stream {
            Stream::Placement => placement_dependent(ctx, subject_id, target_id),
            Stream::Coordination => coordination_dependent(ctx, subject_id, target_id),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

fn placement_dependent(ctx: &DecisionContext, student_id: i64, target_id: i64) -> Option<i64> {
    let all = ctx.events_for(Stream::Placement, student_id);
    let without: Vec<StoredEvent> = all.iter().filter(|e| e.change_set_id != target_id).cloned().collect();
    let ordered = order_events(&without, ctx.term.start);

    let mut state: Option<i64> = None;
    for te in &ordered {
        if let Some(expected) = placement_expected_from(&te.payload) {
            if state != Some(expected) {
                if let Some(stored) = all.iter().find(|e| e.id == te.id) {
                    if stored.change_set_id != target_id {
                        return Some(stored.change_set_id);
                    }
                }
            }
        }
        state = crate::domain::history::apply::apply_placement(state.as_ref(), &te.payload);
    }
    None
}

fn coordination_dependent(ctx: &DecisionContext, company_id: i64, target_id: i64) -> Option<i64> {
    let all = ctx.events_for(Stream::Coordination, company_id);
    let without: Vec<StoredEvent> = all.iter().filter(|e| e.change_set_id != target_id).cloned().collect();
    let ordered = order_events(&without, ctx.term.start);

    let mut state: Option<CoordinationState> = None;
    for te in &ordered {
        if let EventPayload::CoordinatorAssigned { from_teacher_id: Some(expected), .. } = &te.payload {
            let matches_expected = state.as_ref().map(|s| s.teacher_id) == Some(*expected);
            if !matches_expected {
                if let Some(stored) = all.iter().find(|e| e.id == te.id) {
                    if stored.change_set_id != target_id {
                        return Some(stored.change_set_id);
                    }
                }
            }
        }
        state = apply_coordination(state.as_ref(), &te.payload);
    }
    None
}

/// Nakil yerinde oluşturulmuş bir işletmeye yapılmışsa VE o işletmeye,
/// geri alınan kümenin DIŞINDA başka referans yoksa pasifleştirilir
/// (spec §5.4).
fn deactivation_row_action(ctx: &DecisionContext, family: &[StoredEvent]) -> Option<RowAction> {
    for e in family {
        if let EventPayload::StudentTransferred { to_company_id, to_company_created: true, .. } = &e.payload {
            if !company_still_referenced(ctx, *to_company_id, e.change_set_id) {
                return Some(RowAction::DeactivateCompany(*to_company_id));
            }
        }
    }
    None
}

fn company_still_referenced(ctx: &DecisionContext, company_id: i64, excluding_change_set: i64) -> bool {
    ctx.events
        .iter()
        .any(|e| e.change_set_id != excluding_change_set && !matches!(e.payload, EventPayload::Revoked) && event_references_company(e, company_id))
}

fn event_references_company(e: &StoredEvent, company_id: i64) -> bool {
    match &e.payload {
        EventPayload::StudentPlaced { to_company_id, from_company_id, .. } => *to_company_id == company_id || *from_company_id == Some(company_id),
        EventPayload::StudentTransferred { from_company_id, to_company_id, .. } => *from_company_id == company_id || *to_company_id == company_id,
        EventPayload::StudentLeft { from_company_id, .. } => *from_company_id == company_id,
        _ => (e.stream == Stream::CompanyHours || e.stream == Stream::Coordination) && e.subject_id == company_id,
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use crate::domain::history::decide::ChangeSetFacts;
    use crate::domain::history::events::Labels;

    fn request(command: ChangeCommand) -> ChangeRequest {
        ChangeRequest { term: "2026-2027/1".into(), effective_date: None, document_date: None, reason: "geri al".into(), command }
    }

    #[test]
    fn revoke_rejected_when_later_set_depends_on_it() {
        let today = ymd(2026, 11, 20);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_company(1, "İşletme A", Some(10.0))
            .with_company(2, "İşletme B", Some(10.0))
            .with_student(100, "Ahmet Yılmaz")
            .with_event(stored_event(1, 1, Stream::Placement, 100, "2026-2027/1", ymd(2026, 9, 1), EventPayload::StudentPlaced { to_company_id: 1, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(2, 2, Stream::Placement, 100, "2026-2027/1", ymd(2026, 11, 5), EventPayload::StudentTransferred { from_company_id: 1, to_company_id: 2, to_company_created: false, labels: Labels(Default::default()) }, false))
            .with_change_set(ChangeSetFacts { id: 2, kind: "transfer_student".into(), effective_date: ymd(2026, 11, 5), revokes_change_set_id: None, revoked_by: None })
            .build();

        let req = request(ChangeCommand::Revoke { change_set_id: 2 });
        let result = revoke(&ctx, &req, 2);
        // NOT: bu senaryoda #2'yi geri almak sorunsuzdur (sonraki bir kayıt yok);
        // asıl bağımlılık testi aşağıda #1 (açılış olmayan bir önceki nakil) ile kurulur.
        assert!(result.is_ok());

        let ctx2 = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_company(1, "İşletme A", Some(10.0))
            .with_company(2, "İşletme B", Some(10.0))
            .with_company(3, "İşletme C", Some(10.0))
            .with_student(100, "Ahmet Yılmaz")
            .with_event(stored_event(1, 1, Stream::Placement, 100, "2026-2027/1", ymd(2026, 9, 1), EventPayload::StudentPlaced { to_company_id: 1, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(2, 2, Stream::Placement, 100, "2026-2027/1", ymd(2026, 11, 3), EventPayload::StudentTransferred { from_company_id: 1, to_company_id: 2, to_company_created: false, labels: Labels(Default::default()) }, false))
            .with_event(stored_event(3, 3, Stream::Placement, 100, "2026-2027/1", ymd(2026, 11, 10), EventPayload::StudentTransferred { from_company_id: 2, to_company_id: 3, to_company_created: false, labels: Labels(Default::default()) }, false))
            .with_change_set(ChangeSetFacts { id: 2, kind: "transfer_student".into(), effective_date: ymd(2026, 11, 3), revokes_change_set_id: None, revoked_by: None })
            .with_change_set(ChangeSetFacts { id: 3, kind: "transfer_student".into(), effective_date: ymd(2026, 11, 10), revokes_change_set_id: None, revoked_by: None })
            .build();

        let req2 = request(ChangeCommand::Revoke { change_set_id: 2 });
        let result2 = revoke(&ctx2, &req2, 2);
        let rejection = result2.unwrap_err();
        assert_eq!(rejection.code, RejectionCode::HasDependents);
        assert_eq!(rejection.conflicting_change_set_ids, vec![3]);
    }

    #[test]
    fn opening_revoke_and_previous_month_sets_are_not_revocable() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_student(100, "Ahmet Yılmaz")
            .with_company(1, "İşletme A", Some(10.0))
            .with_event(stored_event(1, 1, Stream::Placement, 100, "2026-2027/1", ymd(2026, 9, 1), EventPayload::StudentPlaced { to_company_id: 1, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(2, 2, Stream::Placement, 100, "2026-2027/1", ymd(2026, 10, 5), EventPayload::StudentLeft { from_company_id: 1, labels: Labels(Default::default()) }, false))
            .with_change_set(ChangeSetFacts { id: 1, kind: "opening".into(), effective_date: ymd(2026, 9, 1), revokes_change_set_id: None, revoked_by: None })
            .with_change_set(ChangeSetFacts { id: 2, kind: "student_leaves".into(), effective_date: ymd(2026, 10, 5), revokes_change_set_id: None, revoked_by: None })
            .build();

        assert_eq!(revoke(&ctx, &request(ChangeCommand::Revoke { change_set_id: 1 }), 1).unwrap_err().code, RejectionCode::NotRevocable, "açılış geri alınamaz");
        assert_eq!(revoke(&ctx, &request(ChangeCommand::Revoke { change_set_id: 2 }), 2).unwrap_err().code, RejectionCode::NotRevocable, "önceki aya düşen küme geri alınamaz");
    }

    #[test]
    fn revoke_takes_own_cascade_with_it() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_student(100, "Ahmet Yılmaz")
            .with_company(1, "İşletme A", Some(10.0))
            .with_event(stored_event(1, 1, Stream::Placement, 100, "2026-2027/1", ymd(2026, 9, 1), EventPayload::StudentPlaced { to_company_id: 1, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(2, 2, Stream::Placement, 100, "2026-2027/1", ymd(2026, 11, 3), EventPayload::StudentLeft { from_company_id: 1, labels: Labels(Default::default()) }, false))
            .with_event(stored_event(3, 2, Stream::CompanyHours, 1, "2026-2027/1", ymd(2026, 11, 3), EventPayload::HoursCapped { cap: 0, student_count: 0, labels: Labels(Default::default()) }, false))
            .with_change_set(ChangeSetFacts { id: 2, kind: "student_leaves".into(), effective_date: ymd(2026, 11, 3), revokes_change_set_id: None, revoked_by: None })
            .build();

        let decision = revoke(&ctx, &request(ChangeCommand::Revoke { change_set_id: 2 }), 2).unwrap();
        assert_eq!(decision.events.len(), 2, "hem StudentLeft hem onun HoursCapped kaskadı geri alınmalı");
        let revoked_ids: BTreeSet<i64> = decision.events.iter().filter_map(|e| e.revokes).collect();
        assert_eq!(revoked_ids, BTreeSet::from([2, 3]));
    }

    #[test]
    fn correct_is_one_change_set_with_revokes_and_new_events() {
        let today = ymd(2026, 11, 20);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_student(100, "Ahmet Yılmaz")
            .with_company(1, "İşletme A", Some(10.0))
            .with_company(2, "İşletme B", Some(10.0))
            .with_company(3, "İşletme C", Some(10.0))
            .with_event(stored_event(1, 1, Stream::Placement, 100, "2026-2027/1", ymd(2026, 9, 1), EventPayload::StudentPlaced { to_company_id: 1, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(2, 2, Stream::Placement, 100, "2026-2027/1", ymd(2026, 11, 3), EventPayload::StudentTransferred { from_company_id: 1, to_company_id: 3, to_company_created: false, labels: Labels(Default::default()) }, false))
            .with_change_set(ChangeSetFacts { id: 2, kind: "transfer_student".into(), effective_date: ymd(2026, 11, 3), revokes_change_set_id: None, revoked_by: None })
            .build();

        let replacement = ChangeCommand::TransferStudent {
            student_id: 100,
            from_company_id: 1,
            to: crate::domain::history::decide::TransferTarget::Existing { company_id: 2 },
        };
        let req = ChangeRequest { term: "2026-2027/1".into(), effective_date: Some(ymd(2026, 11, 3)), document_date: None, reason: "düzeltme".into(), command: replacement.clone() };
        let decision = correct(&ctx, &req, 2, &replacement).unwrap();

        assert_eq!(decision.change_set.kind, "correct");
        assert_eq!(decision.change_set.revokes_change_set_id, Some(2));
        assert!(decision.events.iter().any(|e| e.revokes == Some(2)));
        assert!(decision.events.iter().any(|e| matches!(&e.payload, EventPayload::StudentTransferred { to_company_id, .. } if *to_company_id == 2)));
    }

    #[test]
    fn revoking_transfer_to_created_company_deactivates_it() {
        let today = ymd(2026, 11, 20);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_student(100, "Ahmet Yılmaz")
            .with_company(1, "İşletme A", Some(10.0))
            .with_company(9, "Yeni İşletme", None)
            .with_event(stored_event(1, 1, Stream::Placement, 100, "2026-2027/1", ymd(2026, 9, 1), EventPayload::StudentPlaced { to_company_id: 1, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(2, 2, Stream::Placement, 100, "2026-2027/1", ymd(2026, 11, 3), EventPayload::StudentTransferred { from_company_id: 1, to_company_id: 9, to_company_created: true, labels: Labels(Default::default()) }, false))
            .with_change_set(ChangeSetFacts { id: 2, kind: "transfer_student".into(), effective_date: ymd(2026, 11, 3), revokes_change_set_id: None, revoked_by: None })
            .build();

        let decision = revoke(&ctx, &request(ChangeCommand::Revoke { change_set_id: 2 }), 2).unwrap();
        assert_eq!(decision.row_actions, vec![RowAction::DeactivateCompany(9)]);
    }

    /// Kritik kural 5 (brief): "Bir tarihte awarded < cap ise ve o saat daha
    /// önce bir hours_capped ile düşürülmüşse bu bildirim verilir. Bu, ÇOK
    /// NEDENLİ geri alma durumunu kapsar." İki bağımsız ayrılış A'yı iki kez
    /// kısıtlar; yalnız BİRİNİ geri almak, kalan (bağımsız) kısıtlamanın
    /// ARTIK GEÇERLİ tavanın altında kaldığını `reducedBelowCap` ile bildirir.
    #[test]
    fn revoking_one_of_two_causes_notices_reduced_below_cap() {
        use crate::domain::history::events::HoursState;
        use crate::domain::history::impact::NoticeCode;

        let today = ymd(2026, 11, 20);
        let hours = |awarded: i64| HoursState { awarded_hours: awarded, max_hours_snapshot: awarded, is_honorary: false, is_locked: false, notes: String::new() };

        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_rules(vec![rule(1, 0.0, None, 1, Some(1), 2), rule(2, 0.0, None, 2, Some(2), 4), rule(3, 0.0, None, 3, None, 6)])
            .with_company(1, "İşletme A", Some(10.0))
            .with_student(100, "S Öğrenci")
            .with_student(101, "X1 Öğrenci")
            .with_student(102, "X2 Öğrenci")
            .with_event(stored_event(1, 1, Stream::Placement, 100, "2026-2027/1", ymd(2026, 9, 1), EventPayload::StudentPlaced { to_company_id: 1, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(2, 1, Stream::Placement, 101, "2026-2027/1", ymd(2026, 9, 1), EventPayload::StudentPlaced { to_company_id: 1, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(3, 1, Stream::Placement, 102, "2026-2027/1", ymd(2026, 9, 1), EventPayload::StudentPlaced { to_company_id: 1, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(4, 1, Stream::CompanyHours, 1, "2026-2027/1", ymd(2026, 9, 1), EventPayload::HoursSet { state: hours(6), previous_awarded: None, labels: Labels(Default::default()) }, true))
            // Değişiklik kümesi 2: X1 ayrılır (Kasım 3) — A 3->2 öğrenciye düşer, tavan 4'e iner.
            .with_event(stored_event(5, 2, Stream::Placement, 101, "2026-2027/1", ymd(2026, 11, 3), EventPayload::StudentLeft { from_company_id: 1, labels: Labels(Default::default()) }, false))
            .with_event(stored_event(6, 2, Stream::CompanyHours, 1, "2026-2027/1", ymd(2026, 11, 3), EventPayload::HoursCapped { cap: 4, student_count: 2, labels: Labels(Default::default()) }, false))
            // Değişiklik kümesi 3: X2 (BAĞIMSIZ) ayrılır (Kasım 5) — o anki gerçekliğe göre tavan 2'ye iner.
            .with_event(stored_event(7, 3, Stream::Placement, 102, "2026-2027/1", ymd(2026, 11, 5), EventPayload::StudentLeft { from_company_id: 1, labels: Labels(Default::default()) }, false))
            .with_event(stored_event(8, 3, Stream::CompanyHours, 1, "2026-2027/1", ymd(2026, 11, 5), EventPayload::HoursCapped { cap: 2, student_count: 1, labels: Labels(Default::default()) }, false))
            .with_change_set(ChangeSetFacts { id: 1, kind: "opening".into(), effective_date: ymd(2026, 9, 1), revokes_change_set_id: None, revoked_by: None })
            .with_change_set(ChangeSetFacts { id: 2, kind: "student_leaves".into(), effective_date: ymd(2026, 11, 3), revokes_change_set_id: None, revoked_by: None })
            .with_change_set(ChangeSetFacts { id: 3, kind: "student_leaves".into(), effective_date: ymd(2026, 11, 5), revokes_change_set_id: None, revoked_by: None })
            .build();

        // Yalnız X1'in ayrılışını (küme 2) geri al; X2'nin BAĞIMSIZ ayrılışı kalır.
        let decision = revoke(&ctx, &request(ChangeCommand::Revoke { change_set_id: 2 }), 2).unwrap();

        assert!(
            decision.impact.notices.iter().any(|n| n.code == NoticeCode::ReducedBelowCap),
            "X1 geri alınınca A'nın gerçek tavanı yükselir, ama X2'nin kaydı hâlâ eski (2 saatlik) kısıtlamayı taşıyor; bu artık gereğinden fazla düşürülmüş sayılmalı"
        );
    }
}
