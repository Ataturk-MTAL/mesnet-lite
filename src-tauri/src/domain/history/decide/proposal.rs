//! `ApplyProposal`: dağıtım motorunun önerisini TEK değişiklik kümesi olarak
//! uygular (Issue #43).
//!
//! Üç parça şu SIRAYLA kararlaştırılır: saat değişiklikleri, yeni atamalar,
//! atama sonlandırmaları. Sıra rastgele değil: çakışma denetimi
//! (`company::find_overlapping_company`) bir işletmenin YÜRÜRLÜKTEKİ saatine
//! bakar. Saati küçülen bir işletmenin yanına aynı öğretmende paketlenen blok
//! ancak küçülme önce görülürse çakışmaz. Her parça, önceki parçaların
//! henüz yazılmamış olaylarını `with_pending` ile görür (`company.rs`
//! deseni); karar fonksiyonlarının kendisi YENİDEN KULLANILIR, kural burada
//! kopyalanmaz — havuz aşımı (`pool_overrun_reason`), küme içi çakışma ve
//! tekrar eden işletme denetimleri o fonksiyonlarda yaşar.
//!
//! Hepsi tek `Decision` olduğu için ya hep ya hiç yazılır ve tek `Revoke`
//! (saatler dahil) kümenin tamamını geri alır.

use crate::domain::history::events::AssignmentSource;
use crate::domain::history::impact::ImpactSummary;
use crate::domain::history::rejection::{Rejection, RejectionCode};

use super::company::{assign_coordinators, end_coordination, finish, reject_duplicate_companies, set_company_hours_excluding};
use super::{with_pending, ChangeRequest, CompanyHoursRow, CoordinatorRow, Decision, DecisionContext, PlannedEvent};

pub(super) fn apply_proposal(
    ctx: &DecisionContext,
    req: &ChangeRequest,
    hours: &[CompanyHoursRow],
    assign: &[CoordinatorRow],
    release: &[i64],
) -> Result<Decision, Rejection> {
    reject_invalid_shape(assign, release)?;

    // Öneri kaynaklı atamayı satırın kendi `source` alanı belirleyemez:
    // arayüz ne gönderirse göndersin bu komutla gelen atama `Proposal`dır.
    let proposal_rows: Vec<CoordinatorRow> =
        assign.iter().map(|row| CoordinatorRow { source: AssignmentSource::Proposal, ..row.clone() }).collect();

    let mut parts: Vec<Decision> = Vec::new();
    if !hours.is_empty() {
        // Taşınan/bırakılan işletmelerin ESKİ blokları yeni saatle denetlenmez.
        let vacating: Vec<i64> = assign.iter().map(|row| row.company_id).chain(release.iter().copied()).collect();
        parts.push(set_company_hours_excluding(ctx, req, hours, &vacating)?);
    }
    if !proposal_rows.is_empty() {
        let seen = with_pending(ctx, &pending_events(&parts));
        parts.push(assign_coordinators(&seen, req, &proposal_rows)?);
    }
    for &company_id in release {
        let seen = with_pending(ctx, &pending_events(&parts));
        parts.push(end_coordination(&seen, req, company_id)?);
    }
    merge(ctx, req, parts)
}

/// Komutun şekline ilişkin reddler: aynı işletmenin iki kez bırakılması ve
/// hem atanıp hem bırakılması. (Aynı işletmenin iki kez ATANMASI
/// `assign_coordinators` içinde, boş küme `merge` içinde yakalanır.)
fn reject_invalid_shape(assign: &[CoordinatorRow], release: &[i64]) -> Result<(), Rejection> {
    reject_duplicate_companies(release.iter().copied())?;
    if let Some(company_id) = release.iter().find(|id| assign.iter().any(|row| row.company_id == **id)) {
        return Err(Rejection::new(
            RejectionCode::InvalidRequest,
            format!("İşletme (kimlik {company_id}) aynı önerede hem atanıyor hem atamadan çıkarılıyor; yalnız birini gönderin."),
        ));
    }
    Ok(())
}

fn pending_events(parts: &[Decision]) -> Vec<PlannedEvent> {
    parts.iter().flat_map(|part| part.events.iter().cloned()).collect()
}

/// Parçaların olaylarını ve etki özetlerini tek karara birleştirir.
/// `caused_by` indeksleri parça içi olduğundan birleşik listedeki yeni
/// konuma kaydırılır (şu an hiçbir parça üretmiyor; ileride üretirse sessizce
/// yanlış olayı göstermesin).
fn merge(ctx: &DecisionContext, req: &ChangeRequest, parts: Vec<Decision>) -> Result<Decision, Rejection> {
    let mut events: Vec<PlannedEvent> = Vec::new();
    let mut impact: Option<ImpactSummary> = None;
    for part in parts {
        let offset = events.len();
        events.extend(part.events.into_iter().map(|mut event| {
            event.caused_by = event.caused_by.map(|index| index + offset);
            event
        }));
        impact = Some(match impact {
            None => part.impact,
            Some(acc) => merge_impact(acc, part.impact),
        });
    }
    let impact = impact.ok_or_else(|| {
        Rejection::new(RejectionCode::InvalidRequest, "Öneri boş: uygulanacak saat değişikliği, atama ya da atama sonlandırma yok.".to_string())
    })?;
    let effective_date = impact.effective_date;
    finish(ctx, req, effective_date, events, impact, Vec::new())
}

fn merge_impact(mut acc: ImpactSummary, next: ImpactSummary) -> ImpactSummary {
    acc.shadowed_until = acc.shadowed_until.or(next.shadowed_until);
    acc.primary.extend(next.primary);
    acc.automatic.extend(next.automatic);
    // Aynı öğretmen iki parçada da uyarılabilir; kullanıcı aynı uyarıyı iki kez görmesin.
    for warning in next.warnings {
        if !acc.warnings.contains(&warning) {
            acc.warnings.push(warning);
        }
    }
    for notice in next.notices {
        if !acc.notices.contains(&notice) {
            acc.notices.push(notice);
        }
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use crate::domain::history::decide::ChangeCommand;
    use crate::domain::history::events::{AssignmentSource, CoordinationState, EventPayload, HoursState, Labels, Stream};
    use crate::domain::history::rejection::RejectionCode;

    fn hours_event(id: i64, company_id: i64, awarded: i64) -> crate::domain::history::events::StoredEvent {
        let state = HoursState { awarded_hours: awarded, max_hours_snapshot: awarded, is_honorary: false, is_locked: false, notes: String::new() };
        stored_event(id, 1, Stream::CompanyHours, company_id, "2026-2027/1", ymd(2026, 9, 1), EventPayload::HoursSet { state, previous_awarded: None, labels: Labels(Default::default()) }, true)
    }

    fn placed(id: i64, student_id: i64, company_id: i64) -> crate::domain::history::events::StoredEvent {
        stored_event(id, 1, Stream::Placement, student_id, "2026-2027/1", ymd(2026, 9, 1), EventPayload::StudentPlaced { to_company_id: company_id, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true)
    }

    fn assigned(id: i64, company_id: i64, teacher_id: i64, day: i64, hour: i64) -> crate::domain::history::events::StoredEvent {
        let state = CoordinationState { teacher_id, visit_day: day, visit_hour: hour, is_forced: false, force_reason: None, source: AssignmentSource::Manual };
        stored_event(id, 1, Stream::Coordination, company_id, "2026-2027/1", ymd(2026, 9, 1), EventPayload::CoordinatorAssigned { state, from_teacher_id: None, labels: Labels(Default::default()) }, true)
    }

    fn row(company_id: i64, awarded: i64) -> CompanyHoursRow {
        CompanyHoursRow { company_id, awarded_hours: awarded, is_honorary: false, is_locked: false, notes: String::new() }
    }

    fn request_for(command: ChangeCommand) -> ChangeRequest {
        ChangeRequest { term: "2026-2027/1".into(), effective_date: Some(ymd(2026, 11, 3)), document_date: None, reason: "öneri".into(), command }
    }

    fn decide_proposal(ctx: &DecisionContext, hours: Vec<CompanyHoursRow>, assign: Vec<CoordinatorRow>, release: Vec<i64>) -> Result<Decision, Rejection> {
        let req = request_for(ChangeCommand::ApplyProposal { hours: hours.clone(), assign: assign.clone(), release: release.clone() });
        apply_proposal(ctx, &req, &hours, &assign, &release)
    }

    fn base() -> crate::domain::history::decide::test_support::ContextBuilder {
        ContextBuilder::new(ymd(2026, 11, 10), term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_pool_hours(100)
            .with_company(1, "İşletme A", Some(10.0))
            .with_company(2, "İşletme B", Some(10.0))
            .with_teacher(5, "Ali Öğretmen")
            .with_event(hours_event(1, 1, 90))
            .with_event(placed(2, 200, 2))
    }

    /// Havuz 100, A zaten 90 almış; B'ye +20 toplamı 110'a çıkarır. Saatler
    /// kümenin ilk parçası olduğu için, geçerli olabilecek atama satırı da
    /// yazılmaz: sonuç Err, hiçbir olay üretilmez (hep ya da hiç).
    #[test]
    fn pool_overrun_in_the_hours_part_rejects_the_whole_proposal() {
        let ctx = base().build();
        let assign = vec![CoordinatorRow { company_id: 2, teacher_id: 5, visit_day: 2, visit_hour: 1, is_forced: false, force_reason: None, source: Default::default() }];

        let err = decide_proposal(&ctx, vec![row(2, 20)], assign, vec![]).unwrap_err();

        assert_eq!(err.code, RejectionCode::PoolExceeded);
    }

    /// Havuz tam dolu (90 + 10 = 100) iken küme kabul edilir ve üç parçanın
    /// olayları tek kararda, SIRAYLA gelir: saat, atama, sonlandırma.
    #[test]
    fn events_come_in_order_hours_then_assign_then_release() {
        let ctx = base().with_event(assigned(3, 1, 5, 4, 1)).build();
        let assign = vec![CoordinatorRow { company_id: 2, teacher_id: 5, visit_day: 2, visit_hour: 1, is_forced: false, force_reason: None, source: Default::default() }];

        let decision = decide_proposal(&ctx, vec![row(2, 10)], assign, vec![1]).unwrap();

        let kinds: Vec<&str> = decision.events.iter().map(|e| e.payload.kind()).collect();
        assert_eq!(kinds, ["hours_set", "coordinator_assigned", "coordinator_ended"]);
        assert_eq!(decision.change_set.kind, "apply_proposal");
        assert_eq!(decision.touched.len(), 3, "A'nın koordinasyonu, B'nin saati ve B'nin koordinasyonu yeniden kurulur");
    }

    /// `assign` satırı `Manual` gelse de yazılan olayın kaynağı `Proposal`dır.
    #[test]
    fn assigned_events_always_carry_the_proposal_source() {
        let ctx = base().build();
        let assign = vec![CoordinatorRow { company_id: 2, teacher_id: 5, visit_day: 2, visit_hour: 1, is_forced: false, force_reason: None, source: AssignmentSource::Manual }];

        let decision = decide_proposal(&ctx, vec![], assign, vec![]).unwrap();

        let EventPayload::CoordinatorAssigned { state, .. } = &decision.events[0].payload else { panic!("atama olayı beklenirdi") };
        assert_eq!(state.source, AssignmentSource::Proposal);
    }
}
