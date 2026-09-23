//! İşletme tarafı komutları: saat, koordinasyon (spec §5.3–§5.4).

use std::collections::BTreeMap;

use chrono::NaiveDate;

use crate::domain::history::apply::{apply_coordination, apply_hours};
use crate::domain::history::events::{CoordinationState, EventPayload, HoursState, Stream};
use crate::domain::history::impact::{ImpactSummary, ImpactWarning, ImpactNotice};
use crate::domain::history::policy::{cap_for, plan_company_policies};
use crate::domain::history::rejection::{Rejection, RejectionCode};
use crate::domain::history::timeline::order_events;
use crate::domain::hour_distribution::pool_overrun_reason;
use crate::domain::scheduling::Block;

use super::flags::{apply_shadow_and_future_notices, teacher_capacity_warnings, teacher_schedule_warnings};
use super::{
    command_kind, debug_opt, impact_line, labels, touched_from, with_pending, ChangeRequest, CompanyHoursRow,
    CoordinatorRow, Decision, DecisionContext, NewChangeSet, PlannedEvent, RowAction,
};

/// `d` tarihinden itibaren bir işletmenin zincir etkisini planlar ve olaya
/// çevirir. `caused_by`, aynı karardaki BİRİNCİL olayın `events` içindeki
/// indeksidir. `student.rs` (nakil/ayrılış/silme) ve `revoke.rs` bunu
/// paylaşır — kural burada TEK yerde yaşar (DRY).
pub(super) fn build_policy_events(
    ctx: &DecisionContext,
    company_id: i64,
    from: NaiveDate,
    caused_by: usize,
) -> (Vec<PlannedEvent>, Vec<ImpactWarning>, Vec<ImpactNotice>) {
    let round_trip_km = ctx.companies.get(&company_id).and_then(|c| c.round_trip_km);
    let counts = ctx.company_student_count_timeline(company_id);
    let hours = ctx.timeline::<HoursState>(Stream::CompanyHours, company_id, apply_hours);
    let coordination = ctx.timeline::<CoordinationState>(Stream::Coordination, company_id, apply_coordination);
    let capped_history = capped_history_for(ctx, company_id);
    let label = ctx.company_label(company_id);

    let plan = plan_company_policies(&label, round_trip_km, &counts, &hours, &coordination, &capped_history, &ctx.rules, from);

    let mut events = Vec::new();
    for (date, cap, student_count) in &plan.reductions {
        events.push(PlannedEvent {
            stream: Stream::CompanyHours,
            subject_id: company_id,
            effective_date: *date,
            payload: EventPayload::HoursCapped { cap: *cap, student_count: *student_count, labels: labels(&[("company", &label)]) },
            caused_by: Some(caused_by),
            revokes: None,
        });
    }
    if let Some(end_date) = plan.coordinator_end {
        if let Some(state) = coordination.state_at(end_date) {
            events.push(PlannedEvent {
                stream: Stream::Coordination,
                subject_id: company_id,
                effective_date: end_date,
                payload: EventPayload::CoordinatorEndedByPolicy {
                    from_teacher_id: state.teacher_id,
                    student_count: 0,
                    labels: labels(&[("company", &label)]),
                },
                caused_by: Some(caused_by),
                revokes: None,
            });
        }
    }

    (events, plan.warnings, plan.notices)
}

/// Bu işletmede daha önce (geri alınmamış) gerçekleşmiş `hours_capped`
/// olaylarının tarihleri — `reducedBelowCap` bildirimi için (spec §5.3).
fn capped_history_for(ctx: &DecisionContext, company_id: i64) -> Vec<NaiveDate> {
    let ordered = order_events(&ctx.events_for(Stream::CompanyHours, company_id), ctx.term.start);
    ordered.iter().filter(|te| matches!(te.payload, EventPayload::HoursCapped { .. })).map(|te| te.effective_date).collect()
}

pub(super) fn set_company_hours(ctx: &DecisionContext, req: &ChangeRequest, rows: &[CompanyHoursRow]) -> Result<Decision, Rejection> {
    let d = ctx.term.resolve_effective_date(req.effective_date, ctx.today)?;
    let mut events = Vec::new();
    let mut impact = ImpactSummary::empty(d, ctx.term.is_planning(ctx.today));
    // Saati değişen HER işletmenin, O TARİHTEKİ koordinatörü (spec §5.3, R2b
    // brief madde 2). Koordinasyon bu komuttan ETKİLENMEZ.
    let mut coordinators: Vec<i64> = Vec::new();

    for row in rows {
        let coordinator = build_hours_event(ctx, d, row, &mut events, &mut impact)?;
        if let Some(teacher_id) = coordinator {
            if !coordinators.contains(&teacher_id) {
                coordinators.push(teacher_id);
            }
        }
    }

    reject_if_pool_overrun(ctx, d, rows, &events)?;

    let augmented = with_pending(ctx, &events);
    for teacher_id in coordinators {
        impact.warnings.extend(teacher_capacity_warnings(&augmented, teacher_id, d));
    }

    finish(ctx, req, d, events, impact, Vec::new())
}

/// Kullanıcı kuralı "havuz aşılamaz" (bkz. `domain::hour_distribution::pool_overrun_reason`,
/// TEK doğruluk yeri). Dönemin TÜM işletmeleri için `d` tarihindeki takdiri
/// toplar; bu komutla değişenler için ZATEN üretilmiş `events`'teki YENİ
/// değeri kullanır — `state_before` sorgusunu ikinci kez çalıştırmaz (DRY).
fn reject_if_pool_overrun(ctx: &DecisionContext, d: NaiveDate, rows: &[CompanyHoursRow], events: &[PlannedEvent]) -> Result<(), Rejection> {
    let overrides: BTreeMap<i64, i64> = events
        .iter()
        .filter_map(|e| match &e.payload {
            EventPayload::HoursSet { state, .. } => Some((e.subject_id, state.awarded_hours)),
            _ => None,
        })
        .collect();

    let old_total = total_awarded_at(ctx, d, &BTreeMap::new());
    let new_total = total_awarded_at(ctx, d, &overrides);

    let Some(reason) = pool_overrun_reason(old_total, new_total, ctx.pool_hours) else { return Ok(()) };
    let company_names = rows.iter().map(|r| ctx.company_label(r.company_id)).collect::<Vec<_>>().join(", ");
    Err(Rejection::new(RejectionCode::PoolExceeded, format!("{company_names}: {reason}")))
}

/// Dönemdeki TÜM işletmelerin `d` tarihindeki takdir toplamı. `overrides`'ta
/// olan işletmeler için o değer, olmayanlar için ZATEN KAYITLI (geçmiş)
/// durum kullanılır — böylece bu partide değişmeyen işletmelerin mevcut
/// takdiri de toplama girer.
fn total_awarded_at(ctx: &DecisionContext, d: NaiveDate, overrides: &BTreeMap<i64, i64>) -> i64 {
    ctx.companies
        .keys()
        .map(|&company_id| match overrides.get(&company_id) {
            Some(&awarded) => awarded,
            None => ctx
                .timeline::<HoursState>(Stream::CompanyHours, company_id, apply_hours)
                .state_at(d)
                .map(|h| h.awarded_hours)
                .unwrap_or(0),
        })
        .sum()
}

/// Tek bir `CompanyHoursRow` için olayı `events`e ekler ve etki satırını
/// üretir; döndürdüğü öğretmen kimliği (varsa) O TARİHTEKİ koordinatördür —
/// kapasite bayrağı çağıran tarafta TOPLU hesaplanır (spec §5.3, R2b brief
/// madde 2).
fn build_hours_event(
    ctx: &DecisionContext,
    d: NaiveDate,
    row: &CompanyHoursRow,
    events: &mut Vec<PlannedEvent>,
    impact: &mut ImpactSummary,
) -> Result<Option<i64>, Rejection> {
    ctx.require_company(row.company_id)?;
    let label = ctx.company_label(row.company_id);
    let prior = ctx.state_before::<HoursState>(Stream::CompanyHours, row.company_id, d, apply_hours);
    let student_count = ctx.company_student_count_timeline(row.company_id).state_at(d).copied().unwrap_or(0);
    let round_trip_km = ctx.companies.get(&row.company_id).and_then(|c| c.round_trip_km);
    let cap = cap_for(&ctx.rules, round_trip_km, student_count);

    let awarded = if row.is_honorary { 0 } else { row.awarded_hours };
    validate_manual_hours(&label, awarded, cap, prior.as_ref())?;

    let max_hours_snapshot = cap.unwrap_or_else(|| prior.as_ref().map(|p| p.max_hours_snapshot).unwrap_or(awarded));
    let state = HoursState { awarded_hours: awarded, max_hours_snapshot, is_honorary: row.is_honorary, is_locked: row.is_locked, notes: row.notes.clone() };
    let event = PlannedEvent {
        stream: Stream::CompanyHours,
        subject_id: row.company_id,
        effective_date: d,
        payload: EventPayload::HoursSet {
            state: state.clone(),
            previous_awarded: prior.as_ref().map(|p| p.awarded_hours),
            labels: labels(&[("company", &label)]),
        },
        caused_by: None,
        revokes: None,
    };
    impact.primary.push(impact_line(&label, &event, debug_opt(prior.as_ref()), Some(format!("{state:?}"))));
    events.push(event);
    apply_shadow_and_future_notices(ctx, Stream::CompanyHours, row.company_id, &label, d, events, impact);

    let coordinator = ctx.timeline::<CoordinationState>(Stream::Coordination, row.company_id, apply_coordination).state_at(d).map(|c| c.teacher_id);
    Ok(coordinator)
}

/// Elle girilen saat, tavanı AŞACAK ŞEKİLDE artırılamaz (spec §5.2). Tek
/// istisna: satır ZATEN kilitli ve tavanın üstündeyse — o satırda not
/// değişikliği, kilit kaldırma ve saat DÜŞÜRME serbesttir; yalnız daha da
/// YÜKSELTİLEMEZ.
fn validate_manual_hours(label: &str, awarded: i64, cap: Option<i64>, prior: Option<&HoursState>) -> Result<(), Rejection> {
    let Some(cap) = cap else { return Ok(()) };
    if awarded <= cap {
        return Ok(());
    }
    let grandfathered = prior.is_some_and(|p| p.is_locked && p.awarded_hours > cap && awarded <= p.awarded_hours);
    if grandfathered {
        return Ok(());
    }
    Err(Rejection::new(RejectionCode::AboveCap, format!("{label}: {awarded} saat, {cap} saatlik tavanı aşıyor")))
}

pub(super) fn assign_coordinators(ctx: &DecisionContext, req: &ChangeRequest, rows: &[CoordinatorRow]) -> Result<Decision, Rejection> {
    let d = ctx.term.resolve_effective_date(req.effective_date, ctx.today)?;
    let mut events = Vec::new();
    let mut impact = ImpactSummary::empty(d, ctx.term.is_planning(ctx.today));
    let mut teachers: Vec<i64> = Vec::new();

    for row in rows {
        let event = build_coordinator_event(ctx, d, row, &mut impact)?;
        if !teachers.contains(&row.teacher_id) {
            teachers.push(row.teacher_id);
        }
        events.push(event);
    }

    // Yeni atanan öğretmenin haftalık kapasitesi ve boş saatleri artık farklı
    // bir yük taşıyor olabilir (spec §5.3, R2b brief madde 2).
    let augmented = with_pending(ctx, &events);
    for teacher_id in teachers {
        impact.warnings.extend(teacher_capacity_warnings(&augmented, teacher_id, d));
        impact.warnings.extend(teacher_schedule_warnings(&augmented, teacher_id, d));
    }

    finish(ctx, req, d, events, impact, Vec::new())
}

/// Tek bir `CoordinatorRow` için olayı ve etki satırını üretir. Çakışma
/// denetimi (`find_overlapping_company`) burada kalır; kapasite/program
/// bayrakları çağıran tarafta TOPLU hesaplanır.
fn build_coordinator_event(ctx: &DecisionContext, d: NaiveDate, row: &CoordinatorRow, impact: &mut ImpactSummary) -> Result<PlannedEvent, Rejection> {
    // Pasif işletme artık kullanılmıyor; ona ziyaret planlanmaz.
    ctx.require_active_company(row.company_id)?;
    ctx.require_teacher(row.teacher_id)?;
    let label = ctx.company_label(row.company_id);
    let hours = ctx.timeline::<HoursState>(Stream::CompanyHours, row.company_id, apply_hours);
    let awarded = hours.state_at(d).map(|h| h.awarded_hours).unwrap_or(0);
    let candidate_block = Block::from_start(row.visit_day, row.visit_hour, awarded);

    if !row.is_forced {
        if let Some(other) = find_overlapping_company(ctx, row.teacher_id, row.company_id, candidate_block, d) {
            return Err(Rejection::new(
                RejectionCode::BlockOverlap,
                format!("{}: aynı öğretmenin {other} işletmesindeki bloğuyla çakışıyor", ctx.teacher_label(row.teacher_id)),
            ));
        }
    }

    let prior = ctx.state_before::<CoordinationState>(Stream::Coordination, row.company_id, d, apply_coordination);
    let state = CoordinationState { teacher_id: row.teacher_id, visit_day: row.visit_day, visit_hour: row.visit_hour, is_forced: row.is_forced, force_reason: row.force_reason.clone() };
    let event = PlannedEvent {
        stream: Stream::Coordination,
        subject_id: row.company_id,
        effective_date: d,
        payload: EventPayload::CoordinatorAssigned {
            state: state.clone(),
            from_teacher_id: prior.as_ref().map(|p| p.teacher_id),
            labels: labels(&[("company", &label)]),
        },
        caused_by: None,
        revokes: None,
    };
    impact.primary.push(impact_line(&label, &event, debug_opt(prior.as_ref()), Some(format!("{state:?}"))));
    Ok(event)
}

/// Aynı öğretmenin, tarih aralıkları çakışan başka bir işletme bloğu var mı?
/// Yalnız GERÇEKTEN örtüşen dönemler bakılır (spec §5.3, "coordinator_block_overlap
/// yalnız tarihler çakışınca reddedilir").
fn find_overlapping_company(
    ctx: &DecisionContext,
    teacher_id: i64,
    exclude_company_id: i64,
    candidate_block: Block,
    from: NaiveDate,
) -> Option<String> {
    for (&company_id, facts) in &ctx.companies {
        if company_id == exclude_company_id {
            continue;
        }
        let hours = ctx.timeline::<HoursState>(Stream::CompanyHours, company_id, apply_hours);
        let coordination = ctx.timeline::<CoordinationState>(Stream::Coordination, company_id, apply_coordination);
        for iv in &coordination.intervals {
            if iv.state.teacher_id != teacher_id || !ranges_overlap(from, None, iv.valid_from, iv.valid_to) {
                continue;
            }
            let awarded = hours.state_at(iv.valid_from).map(|h| h.awarded_hours).unwrap_or(0);
            let other_block = Block::from_start(iv.state.visit_day, iv.state.visit_hour, awarded);
            if other_block.overlaps(&candidate_block) {
                return Some(facts.name.clone());
            }
        }
    }
    None
}

fn ranges_overlap(a_from: NaiveDate, a_to: Option<NaiveDate>, b_from: NaiveDate, b_to: Option<NaiveDate>) -> bool {
    let a_end = a_to.unwrap_or(NaiveDate::MAX);
    let b_end = b_to.unwrap_or(NaiveDate::MAX);
    a_from < b_end && b_from < a_end
}

pub(super) fn end_coordination(ctx: &DecisionContext, req: &ChangeRequest, company_id: i64) -> Result<Decision, Rejection> {
    let d = ctx.term.resolve_effective_date(req.effective_date, ctx.today)?;
    ctx.require_company(company_id)?;
    let label = ctx.company_label(company_id);
    let prior = ctx.state_before::<CoordinationState>(Stream::Coordination, company_id, d, apply_coordination);
    let Some(prior) = prior else {
        return Err(Rejection::new(RejectionCode::FactNotTrueAtDate, format!("{label}: bu tarihte koordinatörü yok.")));
    };

    let event = PlannedEvent {
        stream: Stream::Coordination,
        subject_id: company_id,
        effective_date: d,
        payload: EventPayload::CoordinatorEnded { from_teacher_id: prior.teacher_id, labels: labels(&[("company", &label)]) },
        caused_by: None,
        revokes: None,
    };
    let mut impact = ImpactSummary::empty(d, ctx.term.is_planning(ctx.today));
    impact.primary.push(impact_line(&label, &event, debug_opt(Some(&prior)), None));

    finish(ctx, req, d, vec![event], impact, Vec::new())
}

/// Yalnız PLANLAMA evresinde tüm koordinatör atamalarını temizler (spec §5.4).
pub(super) fn clear_coordination(ctx: &DecisionContext, req: &ChangeRequest) -> Result<Decision, Rejection> {
    if !ctx.term.is_planning(ctx.today) {
        return Err(Rejection::new(
            RejectionCode::PlanningOnly,
            "Koordinatör atamaları yalnız planlama evresinde (dönem başlamadan) topluca temizlenebilir.".to_string(),
        ));
    }
    let d = ctx.term.start;
    let mut events = Vec::new();
    let mut impact = ImpactSummary::empty(d, true);
    for (&company_id, facts) in &ctx.companies {
        let coordination = ctx.timeline::<CoordinationState>(Stream::Coordination, company_id, apply_coordination);
        let Some(state) = coordination.state_at(d) else { continue };
        let event = PlannedEvent {
            stream: Stream::Coordination,
            subject_id: company_id,
            effective_date: d,
            payload: EventPayload::CoordinatorEnded { from_teacher_id: state.teacher_id, labels: labels(&[("company", &facts.name)]) },
            caused_by: None,
            revokes: None,
        };
        impact.primary.push(impact_line(&facts.name, &event, debug_opt(Some(state)), None));
        events.push(event);
    }

    finish(ctx, req, d, events, impact, Vec::new())
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
    let _ = ctx; // imza tutarlılığı: gelecekte bağlam tabanlı ek kontroller buraya eklenebilir
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
    use crate::domain::history::decide::{ChangeCommand, CompanyHoursRow, CoordinatorRow};
    use crate::domain::history::events::{Labels, TeacherLoad};
    use crate::domain::history::impact::WarningCode;
    use crate::domain::models::{ChiefType, EmploymentType};

    fn hours_state(awarded: i64, locked: bool) -> HoursState {
        HoursState { awarded_hours: awarded, max_hours_snapshot: awarded, is_honorary: false, is_locked: locked, notes: "eski not".into() }
    }

    fn request(effective_date: NaiveDate, command: ChangeCommand) -> ChangeRequest {
        ChangeRequest { term: "2026-2027/1".into(), effective_date: Some(effective_date), document_date: None, reason: "test".into(), command }
    }

    // --- Havuz aşımı (kullanıcı kuralı: "havuz aşılamaz") ---

    /// Kapak/tavan denetimini devre dışı bırakmak için (bu testlerin konusu
    /// havuz, tek işletmenin mesafe/öğrenci tavanı değil): `cap_for`,
    /// öğrencisi 0 olan işletmeye `Some(0)` tavan verir (policy.rs), bu da
    /// havuz denetimine hiç ulaşmadan `AboveCap` fırlatır. Bir öğrenci
    /// yerleştirip kural listesini boş bırakmak `cap_for`'u `None`'a
    /// (tavansız) düşürür.
    fn placed(event_id: i64, student_id: i64, company_id: i64) -> crate::domain::history::events::StoredEvent {
        stored_event(event_id, 900, Stream::Placement, student_id, "2026-2027/1", ymd(2026, 9, 1), EventPayload::StudentPlaced { to_company_id: company_id, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true)
    }

    /// Havuz 100, İşletme A zaten 90 almış; B'ye +20 vermek toplamı 110'a
    /// çıkarır — reddedilir, HİÇBİR olay üretilmez.
    #[test]
    fn set_company_hours_rejects_when_pool_would_be_exceeded() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_pool_hours(100)
            .with_company(1, "İşletme A", Some(10.0))
            .with_company(2, "İşletme B", Some(10.0))
            .with_event(stored_event(1, 1, Stream::CompanyHours, 1, "2026-2027/1", ymd(2026, 9, 1), EventPayload::HoursSet { state: hours_state(90, false), previous_awarded: None, labels: Labels(Default::default()) }, true))
            .with_event(placed(2, 200, 2))
            .build();

        let rows = vec![CompanyHoursRow { company_id: 2, awarded_hours: 20, is_honorary: false, is_locked: false, notes: String::new() }];
        let req = request(ymd(2026, 11, 3), ChangeCommand::SetCompanyHours { rows: rows.clone() });
        let err = set_company_hours(&ctx, &req, &rows).unwrap_err();

        assert_eq!(err.code, RejectionCode::PoolExceeded);
        assert!(err.message.contains("110"), "gerçek toplamı söylemeli: {}", err.message);
        assert!(err.message.contains("İşletme"), "hangi işletmenin değiştiğini söylemeli: {}", err.message);
    }

    /// Tam havuza eşitlemek (90 + 10 = 100) SINIR DAHİL kabul edilir.
    #[test]
    fn set_company_hours_accepts_reaching_the_pool_exactly() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_pool_hours(100)
            .with_company(1, "İşletme A", Some(10.0))
            .with_company(2, "İşletme B", Some(10.0))
            .with_event(stored_event(1, 1, Stream::CompanyHours, 1, "2026-2027/1", ymd(2026, 9, 1), EventPayload::HoursSet { state: hours_state(90, false), previous_awarded: None, labels: Labels(Default::default()) }, true))
            .with_event(placed(2, 200, 2))
            .build();

        let rows = vec![CompanyHoursRow { company_id: 2, awarded_hours: 10, is_honorary: false, is_locked: false, notes: String::new() }];
        let req = request(ymd(2026, 11, 3), ChangeCommand::SetCompanyHours { rows: rows.clone() });
        assert!(set_company_hours(&ctx, &req, &rows).is_ok());
    }

    /// Havuz tanımlanmamışsa (`0`, ders yükü ekranı hiç doldurulmamış) aşım
    /// denetimi hiç yapılmaz — mevcut "tanımlanmamış" uyarısıyla tutarlı.
    #[test]
    fn set_company_hours_skips_the_pool_check_when_the_pool_is_undefined() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_company(1, "İşletme A", Some(10.0))
            .with_event(placed(1, 200, 1))
            .build();

        let rows = vec![CompanyHoursRow { company_id: 1, awarded_hours: 10_000, is_honorary: false, is_locked: false, notes: String::new() }];
        let req = request(ymd(2026, 11, 3), ChangeCommand::SetCompanyHours { rows: rows.clone() });
        assert!(set_company_hours(&ctx, &req, &rows).is_ok());
    }

    /// Aşımı AZALTAN bir düzenleme (100 → 80, havuz 80) kabul edilir.
    #[test]
    fn set_company_hours_accepts_an_edit_that_reduces_the_total() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_pool_hours(80)
            .with_company(1, "İşletme A", Some(10.0))
            .with_event(stored_event(1, 1, Stream::CompanyHours, 1, "2026-2027/1", ymd(2026, 9, 1), EventPayload::HoursSet { state: hours_state(100, false), previous_awarded: None, labels: Labels(Default::default()) }, true))
            .with_event(placed(2, 200, 1))
            .build();

        let rows = vec![CompanyHoursRow { company_id: 1, awarded_hours: 80, is_honorary: false, is_locked: false, notes: String::new() }];
        let req = request(ymd(2026, 11, 3), ChangeCommand::SetCompanyHours { rows: rows.clone() });
        assert!(set_company_hours(&ctx, &req, &rows).is_ok());
    }

    /// Göçten kalma bir veri zaten havuzu aşmışsa (60 > 50), aşımı ARTIRMAYAN
    /// bir düzenleme (60 → 60) kilitlenmez; aşımı BÜYÜTEN bir düzenleme
    /// (60 → 65) yine reddedilir.
    #[test]
    fn set_company_hours_does_not_lock_a_pre_existing_overrun_but_still_rejects_a_further_increase() {
        let today = ymd(2026, 11, 10);
        let base = || {
            ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
                .with_pool_hours(50)
                .with_company(1, "İşletme A", Some(10.0))
                .with_event(stored_event(1, 1, Stream::CompanyHours, 1, "2026-2027/1", ymd(2026, 9, 1), EventPayload::HoursSet { state: hours_state(60, false), previous_awarded: None, labels: Labels(Default::default()) }, true))
                .with_event(placed(2, 200, 1))
        };

        let unchanged = vec![CompanyHoursRow { company_id: 1, awarded_hours: 60, is_honorary: false, is_locked: false, notes: "not güncellendi".into() }];
        let req_unchanged = request(ymd(2026, 11, 3), ChangeCommand::SetCompanyHours { rows: unchanged.clone() });
        assert!(
            set_company_hours(&base().build(), &req_unchanged, &unchanged).is_ok(),
            "mevcut aşımı korumak kilitlenmemeli"
        );

        let increased = vec![CompanyHoursRow { company_id: 1, awarded_hours: 65, is_honorary: false, is_locked: false, notes: String::new() }];
        let req_increased = request(ymd(2026, 11, 3), ChangeCommand::SetCompanyHours { rows: increased.clone() });
        assert_eq!(
            set_company_hours(&base().build(), &req_increased, &increased).unwrap_err().code,
            RejectionCode::PoolExceeded,
            "aşımı büyüten değişiklik yine reddedilmeli"
        );
    }

    #[test]
    fn manual_raise_above_cap_rejected() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_rules(vec![rule(1, 0.0, None, 1, None, 4)])
            .with_company(1, "İşletme A", Some(10.0))
            .with_event(stored_event(1, 1, Stream::Placement, 100, "2026-2027/1", ymd(2026, 9, 1), EventPayload::StudentPlaced { to_company_id: 1, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .build();

        let rows = vec![CompanyHoursRow { company_id: 1, awarded_hours: 8, is_honorary: false, is_locked: false, notes: String::new() }];
        let req = request(ymd(2026, 11, 3), ChangeCommand::SetCompanyHours { rows });
        let result = set_company_hours(&ctx, &req, match &req.command { ChangeCommand::SetCompanyHours { rows } => rows, _ => unreachable!() });
        assert_eq!(result.unwrap_err().code, RejectionCode::AboveCap);
    }

    #[test]
    fn locked_above_cap_row_can_change_notes_and_unlock() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_rules(vec![rule(1, 0.0, None, 1, None, 4)])
            .with_company(1, "İşletme A", Some(10.0))
            .with_event(stored_event(1, 1, Stream::Placement, 100, "2026-2027/1", ymd(2026, 9, 1), EventPayload::StudentPlaced { to_company_id: 1, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(2, 2, Stream::CompanyHours, 1, "2026-2027/1", ymd(2026, 9, 1), EventPayload::HoursSet { state: hours_state(8, true), previous_awarded: None, labels: Labels(Default::default()) }, false))
            .build();

        let rows = vec![CompanyHoursRow { company_id: 1, awarded_hours: 8, is_honorary: false, is_locked: false, notes: "kilidi kaldırdım".into() }];
        let req = request(ymd(2026, 11, 3), ChangeCommand::SetCompanyHours { rows: rows.clone() });
        let decision = set_company_hours(&ctx, &req, &rows).unwrap();
        assert_eq!(decision.events.len(), 1);
        match &decision.events[0].payload {
            EventPayload::HoursSet { state, .. } => {
                assert!(!state.is_locked);
                assert_eq!(state.awarded_hours, 8);
            }
            other => panic!("beklenmedik olay: {other:?}"),
        }
    }

    #[test]
    fn honorary_forces_zero() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_rules(vec![rule(1, 0.0, None, 1, None, 4)])
            .with_company(1, "İşletme A", Some(10.0))
            .with_event(stored_event(1, 1, Stream::Placement, 100, "2026-2027/1", ymd(2026, 9, 1), EventPayload::StudentPlaced { to_company_id: 1, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .build();

        let rows = vec![CompanyHoursRow { company_id: 1, awarded_hours: 6, is_honorary: true, is_locked: false, notes: String::new() }];
        let req = request(ymd(2026, 11, 3), ChangeCommand::SetCompanyHours { rows: rows.clone() });
        let decision = set_company_hours(&ctx, &req, &rows).unwrap();
        match &decision.events[0].payload {
            EventPayload::HoursSet { state, .. } => assert_eq!(state.awarded_hours, 0, "fahri ziyarette saat 0'a zorlanır"),
            other => panic!("beklenmedik olay: {other:?}"),
        }
    }

    /// AYNI AY içinde girilmiş, DAHA ESKİ bir `company_hours` kaydı OTOMATİK
    /// geri alınır (brief: "CompanyHours ... için bir test" — `flags.rs`'teki
    /// kuralın bu akışta da çalıştığının kanıtı).
    #[test]
    fn same_month_later_hours_record_is_revoked_automatically() {
        let today = ymd(2026, 9, 25);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_company(1, "İşletme A", Some(10.0))
            .with_event(placed(1, 200, 1))
            .with_event(stored_event(2, 2, Stream::CompanyHours, 1, "2026-2027/1", ymd(2026, 9, 1), EventPayload::HoursSet { state: hours_state(4, false), previous_awarded: None, labels: Labels(Default::default()) }, true))
            .with_event(stored_event(20, 3, Stream::CompanyHours, 1, "2026-2027/1", ymd(2026, 9, 20), EventPayload::HoursSet { state: hours_state(6, false), previous_awarded: None, labels: Labels(Default::default()) }, false))
            .build();

        let rows = vec![CompanyHoursRow { company_id: 1, awarded_hours: 5, is_honorary: false, is_locked: false, notes: String::new() }];
        let req = request(ymd(2026, 9, 14), ChangeCommand::SetCompanyHours { rows: rows.clone() });
        let decision = set_company_hours(&ctx, &req, &rows).unwrap();

        assert!(decision.events.iter().any(|e| e.revokes == Some(20)), "aynı aydaki 09-20 kaydı geri alınmalı");
        assert_eq!(decision.impact.shadowed_until, None);
        let revoked_line = decision.impact.primary.iter().find(|l| l.kind == "revoked").expect("revoked satırı beklenir");
        assert_eq!(revoked_line.effective_date, ymd(2026, 9, 20), "satırın tarihi geri alınan kaydın KENDİ tarihi olmalı");
    }

    #[test]
    fn coordinator_block_overlap_rejected_only_when_dates_overlap() {
        let today = ymd(2026, 11, 10);
        let base = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_company(1, "İşletme A", Some(10.0))
            .with_company(2, "İşletme B", Some(10.0))
            .with_teacher(5, "Ali Öğretmen")
            .with_event(stored_event(1, 1, Stream::CompanyHours, 2, "2026-2027/1", ymd(2026, 9, 1), EventPayload::HoursSet { state: hours_state(2, false), previous_awarded: None, labels: Labels(Default::default()) }, true));

        // Çakışmayan durum: B'nin ataması Ekim'de bitiyor, yeni atama Kasım'da başlıyor.
        let non_overlapping = base
            .clone()
            .with_event(stored_event(2, 2, Stream::Coordination, 2, "2026-2027/1", ymd(2026, 9, 1), EventPayload::CoordinatorAssigned { state: CoordinationState { teacher_id: 5, visit_day: 1, visit_hour: 9, is_forced: false, force_reason: None }, from_teacher_id: None, labels: Labels(Default::default()) }, true))
            .with_event(stored_event(3, 3, Stream::Coordination, 2, "2026-2027/1", ymd(2026, 10, 15), EventPayload::CoordinatorEnded { from_teacher_id: 5, labels: Labels(Default::default()) }, false))
            .build();
        let rows_ok = vec![CoordinatorRow { company_id: 1, teacher_id: 5, visit_day: 1, visit_hour: 9, is_forced: false, force_reason: None }];
        let req_ok = request(ymd(2026, 11, 3), ChangeCommand::AssignCoordinators { rows: rows_ok.clone() });
        assert!(assign_coordinators(&non_overlapping, &req_ok, &rows_ok).is_ok(), "tarihler örtüşmüyorsa reddedilmemeli");

        // Örtüşen durum: B'nin ataması SÜRÜYOR (bitiş tarihi yok).
        let overlapping = base
            .with_event(stored_event(2, 2, Stream::Coordination, 2, "2026-2027/1", ymd(2026, 9, 1), EventPayload::CoordinatorAssigned { state: CoordinationState { teacher_id: 5, visit_day: 1, visit_hour: 9, is_forced: false, force_reason: None }, from_teacher_id: None, labels: Labels(Default::default()) }, true))
            .build();
        let rows_bad = vec![CoordinatorRow { company_id: 1, teacher_id: 5, visit_day: 1, visit_hour: 9, is_forced: false, force_reason: None }];
        let req_bad = request(ymd(2026, 11, 3), ChangeCommand::AssignCoordinators { rows: rows_bad.clone() });
        let result = assign_coordinators(&overlapping, &req_bad, &rows_bad);
        assert_eq!(result.unwrap_err().code, RejectionCode::BlockOverlap);
    }

    #[test]
    fn clear_coordination_only_while_planning() {
        let running = ContextBuilder::new(ymd(2026, 11, 10), term(ymd(2026, 9, 1), ymd(2027, 1, 31))).build();
        let req = request(ymd(2026, 11, 3), ChangeCommand::ClearCoordination);
        assert_eq!(clear_coordination(&running, &req).unwrap_err().code, RejectionCode::PlanningOnly);

        let planning = ContextBuilder::new(ymd(2026, 8, 1), term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_company(1, "İşletme A", Some(10.0))
            .with_event(stored_event(1, 1, Stream::Coordination, 1, "2026-2027/1", ymd(2026, 9, 1), EventPayload::CoordinatorAssigned { state: CoordinationState { teacher_id: 5, visit_day: 1, visit_hour: 9, is_forced: false, force_reason: None }, from_teacher_id: None, labels: Labels(Default::default()) }, true))
            .build();
        let req = ChangeRequest { term: "2026-2027/1".into(), effective_date: None, document_date: None, reason: "temizle".into(), command: ChangeCommand::ClearCoordination };
        let decision = clear_coordination(&planning, &req).unwrap();
        assert_eq!(decision.events.len(), 1);
        assert!(matches!(decision.events[0].payload, EventPayload::CoordinatorEnded { .. }));
    }

    fn department_load(other_extra_hours: i64, max_extra_hours: i64, chief_type: ChiefType) -> TeacherLoad {
        TeacherLoad { base_hours: 15, max_extra_hours, other_extra_hours, chief_type, employment_type: EmploymentType::Tenured }
    }

    /// B'ye yeni koordinatör atanınca, öğretmenin A'daki 8 saatiyle
    /// TOPLAM ataması kapasiteyi (24/10/4 → 10) aşar. Bloklar farklı günde
    /// olduğu için `BlockOverlap` tetiklenmez; yalnız `CapacityExceeded`
    /// bayraklanır (R2b brief madde 2).
    #[test]
    fn assign_coordinator_flags_capacity_via_decide() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_teacher(5, "Ali Öğretmen")
            .with_company(1, "İşletme A", Some(10.0))
            .with_company(2, "İşletme B", Some(10.0))
            .with_event(stored_event(1, 1, Stream::TeacherLoad, 5, "2026-2027/1", ymd(2026, 9, 1), EventPayload::LoadSet { load: department_load(4, 24, ChiefType::Department), previous: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(2, 1, Stream::Coordination, 1, "2026-2027/1", ymd(2026, 9, 1), EventPayload::CoordinatorAssigned { state: CoordinationState { teacher_id: 5, visit_day: 1, visit_hour: 3, is_forced: false, force_reason: None }, from_teacher_id: None, labels: Labels(Default::default()) }, true))
            .with_event(stored_event(3, 1, Stream::CompanyHours, 1, "2026-2027/1", ymd(2026, 9, 1), EventPayload::HoursSet { state: hours_state(8, false), previous_awarded: None, labels: Labels(Default::default()) }, true))
            .with_event(stored_event(4, 1, Stream::CompanyHours, 2, "2026-2027/1", ymd(2026, 9, 1), EventPayload::HoursSet { state: hours_state(5, false), previous_awarded: None, labels: Labels(Default::default()) }, true))
            .build();

        // Farklı gün (2 = Salı): A'nın (Pazartesi) bloğuyla ÖRTÜŞMEZ.
        let rows = vec![CoordinatorRow { company_id: 2, teacher_id: 5, visit_day: 2, visit_hour: 9, is_forced: false, force_reason: None }];
        let req = request(ymd(2026, 11, 5), ChangeCommand::AssignCoordinators { rows: rows.clone() });
        let decision = assign_coordinators(&ctx, &req, &rows).unwrap();

        let warning = decision.impact.warnings.iter().find(|w| w.code == WarningCode::CapacityExceeded).expect("kapasite uyarısı beklenir");
        assert_eq!(warning.from_date, ymd(2026, 11, 5));
    }

    /// A'nın koordinatörü Ali'nin kişisel bütçesi (10/0/0 → 10) sabit;
    /// işletmenin saati 2'den 15'e çıkınca Ali'nin TOPLAM ataması kapasiteyi
    /// aşar. Bu, kurumun MADDE 15/2 tavanından (burada 20, ayrı bir eksen)
    /// bağımsız bir bayraktır (R2b brief madde 2).
    #[test]
    fn set_company_hours_flags_the_coordinators_capacity() {
        let today = ymd(2026, 11, 10);
        let ctx = ContextBuilder::new(today, term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_rules(vec![rule(1, 0.0, None, 1, None, 20)])
            .with_teacher(5, "Ali Öğretmen")
            .with_company(1, "İşletme A", Some(10.0))
            .with_event(stored_event(1, 1, Stream::Placement, 100, "2026-2027/1", ymd(2026, 9, 1), EventPayload::StudentPlaced { to_company_id: 1, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(2, 1, Stream::TeacherLoad, 5, "2026-2027/1", ymd(2026, 9, 1), EventPayload::LoadSet { load: department_load(0, 10, ChiefType::None), previous: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(3, 1, Stream::Coordination, 1, "2026-2027/1", ymd(2026, 9, 1), EventPayload::CoordinatorAssigned { state: CoordinationState { teacher_id: 5, visit_day: 1, visit_hour: 3, is_forced: false, force_reason: None }, from_teacher_id: None, labels: Labels(Default::default()) }, true))
            .with_event(stored_event(4, 1, Stream::CompanyHours, 1, "2026-2027/1", ymd(2026, 9, 1), EventPayload::HoursSet { state: hours_state(2, false), previous_awarded: None, labels: Labels(Default::default()) }, true))
            .build();

        let rows = vec![CompanyHoursRow { company_id: 1, awarded_hours: 15, is_honorary: false, is_locked: false, notes: String::new() }];
        let req = request(ymd(2026, 11, 5), ChangeCommand::SetCompanyHours { rows: rows.clone() });
        let decision = set_company_hours(&ctx, &req, &rows).unwrap();

        let warning = decision.impact.warnings.iter().find(|w| w.code == WarningCode::CapacityExceeded).expect("koordinatörün kapasite uyarısı beklenir");
        assert_eq!(warning.from_date, ymd(2026, 11, 5));
    }
}
