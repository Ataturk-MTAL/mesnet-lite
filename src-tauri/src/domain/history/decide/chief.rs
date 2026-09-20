//! Okulda aynı anda en fazla bir alan şefi (`ChiefType::Department`) kuralı.
//!
//! Okul tek alanla çalışır; alan şefliği (MADDE 6/4, haftada 10 saat) ek ders
//! tavanının içinden düşüldüğü için (`domain/workload.rs`) iki öğretmene
//! birden verilirse aynı görev iki kez sayılır. `workshop_lab` için böyle bir
//! sınır YOKTUR.
//!
//! Kural komut türüne değil, kararın NET sonucuna uygulanır: karardaki olaylar
//! bağlama işlenir (`with_pending`; `Revoked` işaretleri hedeflerini katlamadan
//! düşürür) ve bir öğretmenin daha önce `Department` OLMAYAN günlerinden hangi-
//! lerinin yeni `Department` olduğuna bakılır. Böylece `createTeacher`,
//! `setTeacherLoad`, `revoke` ve `correct` tek yerde denetlenir: geri alma,
//! şeflikten ayrılmayı iptal edip ikinci bir `Department` doğurabilir; düzeltme
//! ise ayrılışı geri alıp yerine şefliği sürdüren bir yük koyabilir.
//!
//! Yalnız YENİ günler denetlenir: zaten `Department` olan öğretmenin saatini
//! düzenlemek yeni gün doğurmaz; göçten gelen mevcut ihlal bu yüzden
//! düzenlemeleri kilitlemez.

use std::collections::BTreeSet;

use chrono::NaiveDate;

use crate::domain::history::apply::apply_load;
use crate::domain::history::events::Stream;
use crate::domain::history::rejection::{Rejection, RejectionCode};
use crate::domain::models::ChiefType;

use super::{with_pending, Decision, DecisionContext};

/// Bir öğretmenin kesintisiz `Department` günleri: `[from, to)` yarı açık.
/// Yarı açıklık, şeflikten ayrılanın ayrıldığı günde halefin başlayabilmesini
/// sağlar. Açık uçlu şeflik `NaiveDate::MAX` ile temsil edilir.
#[derive(Debug, Clone, Copy)]
struct Span {
    from: NaiveDate,
    to: NaiveDate,
    /// Bu şefliği KURAN değişiklik kümesi (ilk aralığı açan olay).
    change_set_id: i64,
}

impl Span {
    fn overlaps(&self, other: &Span) -> bool {
        self.from < other.to && other.from < self.to
    }
}

/// Kararın net sonucunda YENİ bir `Department` günü başka bir öğretmenin
/// `Department` günüyle çakışıyorsa `ChiefAlreadyAssigned` ile reddeder.
pub(super) fn enforce_single_department(ctx: &DecisionContext, decision: &Decision) -> Result<(), Rejection> {
    let affected: BTreeSet<i64> = decision.events.iter().filter(|e| e.stream == Stream::TeacherLoad).map(|e| e.subject_id).collect();
    if affected.is_empty() {
        return Ok(());
    }

    let after = with_pending(ctx, &decision.events);
    let everyone = load_subjects(&after);
    for &teacher_id in &affected {
        let added = subtract(department_spans(&after, teacher_id), &department_spans(ctx, teacher_id));
        let holder = everyone.iter().filter(|&&other| other != teacher_id).find_map(|&other| {
            let held = department_spans(&after, other).into_iter().find(|span| added.iter().any(|new| new.overlaps(span)))?;
            Some((other, held))
        });
        if let Some((holder_id, held)) = holder {
            return Err(already_assigned(ctx, holder_id, &held));
        }
    }
    Ok(())
}

fn already_assigned(ctx: &DecisionContext, holder_id: i64, held: &Span) -> Rejection {
    let message = format!("Alan şefi zaten {}; önce onun şefliği kaldırılmalı.", ctx.teacher_label(holder_id));
    // Şefliği kuran küme yalnız GERÇEK bir kümeyse taşınır; bu kararın henüz
    // kaydedilmemiş olayları için sahte kimlik (`PENDING_CHANGE_SET_ID`) sızmaz.
    let conflicts = if held.change_set_id > 0 { vec![held.change_set_id] } else { Vec::new() };
    Rejection::new(RejectionCode::ChiefAlreadyAssigned, message).with_conflicts(conflicts)
}

/// Bağlamda `teacher_load` olayı olan tüm öğretmenler.
fn load_subjects(ctx: &DecisionContext) -> BTreeSet<i64> {
    ctx.events.iter().filter(|e| e.stream == Stream::TeacherLoad).map(|e| e.subject_id).collect()
}

/// Öğretmenin `Department` günleri; bitişik zaman çizelgesi aralıkları
/// (ör. şef kalıp saatini değiştirdiği kayıtlar) tek şeflik olarak birleşir,
/// böylece `change_set_id` şefliğin BAŞLADIĞI kümeyi gösterir.
fn department_spans(ctx: &DecisionContext, teacher_id: i64) -> Vec<Span> {
    let timeline = ctx.timeline(Stream::TeacherLoad, teacher_id, apply_load);
    let mut spans: Vec<Span> = Vec::new();
    for interval in timeline.intervals.iter().filter(|iv| iv.state.chief_type == ChiefType::Department) {
        let to = interval.valid_to.unwrap_or(NaiveDate::MAX);
        match spans.last_mut() {
            Some(last) if last.to == interval.valid_from => last.to = to,
            _ => spans.push(Span { from: interval.valid_from, to, change_set_id: change_set_of(ctx, interval.source_event_id) }),
        }
    }
    spans
}

fn change_set_of(ctx: &DecisionContext, event_id: i64) -> i64 {
    ctx.events.iter().find(|e| e.id == event_id).map_or(0, |e| e.change_set_id)
}

/// `spans` günlerinden `cuts` günlerini çıkarır.
fn subtract(spans: Vec<Span>, cuts: &[Span]) -> Vec<Span> {
    cuts.iter().fold(spans, |pieces, cut| pieces.into_iter().flat_map(|piece| cut_piece(piece, cut)).collect())
}

fn cut_piece(piece: Span, cut: &Span) -> Vec<Span> {
    if !piece.overlaps(cut) {
        return vec![piece];
    }
    let mut remaining = Vec::new();
    if piece.from < cut.from {
        remaining.push(Span { to: cut.from, ..piece });
    }
    if cut.to < piece.to {
        remaining.push(Span { from: cut.to, ..piece });
    }
    remaining
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::{decide, ChangeCommand, ChangeRequest, ChangeSetFacts, DecisionContext, Materialized, NewTeacherProfile};
    use crate::domain::history::events::{EventPayload, Labels, Stream, StoredEvent, TeacherLoad};
    use crate::domain::history::rejection::{Rejection, RejectionCode};
    use crate::domain::models::{ChiefType, EmploymentType};

    const TERM_ID: &str = "2026-2027/1";
    const CHIEF: i64 = 5;
    const OTHER: i64 = 6;
    const NEW_TEACHER: i64 = 7;
    const OPENING_SET: i64 = 1;

    fn load(chief_type: ChiefType, other_extra_hours: i64) -> TeacherLoad {
        TeacherLoad { base_hours: 15, max_extra_hours: 24, other_extra_hours, chief_type, employment_type: EmploymentType::Tenured }
    }

    fn load_event(id: i64, change_set_id: i64, teacher: i64, date: chrono::NaiveDate, chief_type: ChiefType, is_opening: bool) -> StoredEvent {
        let payload = EventPayload::LoadSet { load: load(chief_type, 0), previous: None, source: "test".into(), labels: Labels(Default::default()) };
        stored_event(id, change_set_id, Stream::TeacherLoad, teacher, TERM_ID, date, payload, is_opening)
    }

    fn opening_facts() -> ChangeSetFacts {
        ChangeSetFacts { id: OPENING_SET, kind: "opening".into(), effective_date: ymd(2026, 9, 1), revokes_change_set_id: None, revoked_by: None }
    }

    /// Ayşe (5) açılıştan beri alan şefi, Ali (6) şef değil; bugün 12 Ekim.
    fn world(chief_type_of_first: ChiefType) -> DecisionContext {
        ContextBuilder::new(ymd(2026, 10, 12), term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_teacher(CHIEF, "Ayşe Yılmaz")
            .with_teacher(OTHER, "Ali Veli")
            .with_change_set(opening_facts())
            .with_event(load_event(1, OPENING_SET, CHIEF, ymd(2026, 9, 1), chief_type_of_first, true))
            .with_event(load_event(2, OPENING_SET, OTHER, ymd(2026, 9, 1), ChiefType::None, true))
            .build()
    }

    fn set_load(teacher_id: i64, date: chrono::NaiveDate, load: TeacherLoad) -> ChangeRequest {
        ChangeRequest { term: TERM_ID.into(), effective_date: Some(date), document_date: None, reason: "test".into(), command: ChangeCommand::SetTeacherLoad { teacher_id, load } }
    }

    fn set_chief(teacher_id: i64, date: chrono::NaiveDate, chief_type: ChiefType) -> ChangeRequest {
        set_load(teacher_id, date, load(chief_type, 0))
    }

    /// Kararı `ctx`'e işler ve küme kimliğini döner.
    fn commit_chief(ctx: &mut DecisionContext, sim: &mut SimCommit, teacher_id: i64, date: chrono::NaiveDate, chief_type: ChiefType) -> i64 {
        let decision = decide(ctx, &set_chief(teacher_id, date, chief_type)).unwrap();
        sim.apply(ctx, decision)
    }

    fn expect_chief_rejection(result: Result<super::super::Decision, Rejection>) -> Rejection {
        let rejection = result.unwrap_err();
        assert_eq!(rejection.code, RejectionCode::ChiefAlreadyAssigned, "beklenen kod: {rejection:?}");
        rejection
    }

    /// Ayşe 10 Ekim'de şeflikten ayrılır (küme 2). Sonraki komut kimliği 3.
    fn world_where_chief_leaves_on_oct_10() -> (DecisionContext, SimCommit, i64) {
        let mut ctx = world(ChiefType::Department);
        let mut sim = SimCommit::starting_at(3, 2);
        let leave_set = commit_chief(&mut ctx, &mut sim, CHIEF, ymd(2026, 10, 10), ChiefType::None);
        (ctx, sim, leave_set)
    }

    fn new_teacher_request(chief_type: ChiefType) -> (ChangeRequest, TeacherLoad) {
        let profile = NewTeacherProfile { first_name: "Can".into(), last_name: "Demir".into(), registry_no: "9".into(), field: "Elektrik".into(), branches: vec![], is_active: true };
        let load = load(chief_type, 0);
        let req = ChangeRequest {
            term: TERM_ID.into(),
            effective_date: Some(ymd(2026, 10, 15)),
            document_date: None,
            reason: "yeni öğretmen".into(),
            command: ChangeCommand::CreateTeacher { teacher: profile, load: load.clone() },
        };
        (req, load)
    }

    fn with_new_teacher_materialized(mut ctx: DecisionContext) -> DecisionContext {
        ctx.materialized = Materialized { teacher_id: Some(NEW_TEACHER), ..Materialized::default() };
        ctx
    }

    #[test]
    fn create_teacher_as_second_department_is_rejected() {
        let ctx = with_new_teacher_materialized(world(ChiefType::Department));
        let (req, _) = new_teacher_request(ChiefType::Department);

        let rejection = expect_chief_rejection(decide(&ctx, &req));

        assert!(rejection.message.contains("Ayşe Yılmaz"), "mesaj mevcut şefi adıyla söylemeli: {}", rejection.message);
        assert_eq!(rejection.conflicting_change_set_ids, vec![OPENING_SET], "şefliği kuran küme (açılış) taşınmalı");
    }

    #[test]
    fn create_teacher_with_other_chief_types_is_allowed_next_to_a_department() {
        let ctx = with_new_teacher_materialized(world(ChiefType::Department));
        for chief_type in [ChiefType::None, ChiefType::WorkshopLab] {
            let (req, _) = new_teacher_request(chief_type);
            assert!(decide(&ctx, &req).is_ok(), "{chief_type:?} alan şefliğiyle çakışmamalı");
        }
    }

    #[test]
    fn create_teacher_as_department_is_allowed_when_nobody_else_is() {
        let ctx = with_new_teacher_materialized(world(ChiefType::None));
        let (req, _) = new_teacher_request(ChiefType::Department);
        assert!(decide(&ctx, &req).is_ok());
    }

    #[test]
    fn set_load_from_none_to_department_is_rejected() {
        let ctx = world(ChiefType::Department);
        let rejection = expect_chief_rejection(decide(&ctx, &set_chief(OTHER, ymd(2026, 10, 15), ChiefType::Department)));
        assert!(rejection.message.contains("Ayşe Yılmaz"), "mesaj: {}", rejection.message);
        assert_eq!(rejection.conflicting_change_set_ids, vec![OPENING_SET]);
    }

    #[test]
    fn set_load_from_workshop_lab_to_department_is_rejected() {
        let mut ctx = world(ChiefType::Department);
        let mut sim = SimCommit::starting_at(3, 2);
        commit_chief(&mut ctx, &mut sim, OTHER, ymd(2026, 10, 5), ChiefType::WorkshopLab);

        expect_chief_rejection(decide(&ctx, &set_chief(OTHER, ymd(2026, 10, 15), ChiefType::Department)));
    }

    #[test]
    fn second_department_is_refused_before_the_first_leaves_but_allowed_on_that_day() {
        let (ctx, _, _) = world_where_chief_leaves_on_oct_10();

        let early = expect_chief_rejection(decide(&ctx, &set_chief(OTHER, ymd(2026, 10, 5), ChiefType::Department)));
        assert_eq!(early.conflicting_change_set_ids, vec![OPENING_SET], "Ayşe'nin şefliğini kuran küme açılıştır");

        assert!(decide(&ctx, &set_chief(OTHER, ymd(2026, 10, 10), ChiefType::Department)).is_ok(), "Ayşe'nin ayrıldığı günün kendisinde devir serbest");
        assert!(decide(&ctx, &set_chief(OTHER, ymd(2026, 10, 20), ChiefType::Department)).is_ok());
    }

    #[test]
    fn a_department_window_that_ends_before_the_next_chief_begins_is_allowed() {
        let mut ctx = world(ChiefType::None);
        let mut sim = SimCommit::starting_at(3, 2);
        commit_chief(&mut ctx, &mut sim, CHIEF, ymd(2026, 11, 20), ChiefType::Department);

        // Ali'nin 1 Kasım'da şeflikten çıkacağı kayıtlıdır; 15 Ekim'de başlayan
        // şefliği o güne kadar sürer ve Ayşe'nin 20 Kasım şefliğine değmez.
        // (Bırakma kaydı olmasaydı çakışırdı: bkz. sonraki test.)
        commit_chief(&mut ctx, &mut sim, OTHER, ymd(2026, 11, 1), ChiefType::None);
        assert!(decide(&ctx, &set_chief(OTHER, ymd(2026, 10, 15), ChiefType::Department)).is_ok());
    }

    #[test]
    fn open_ended_department_is_refused_when_another_starts_later() {
        let mut ctx = world(ChiefType::None);
        let mut sim = SimCommit::starting_at(3, 2);
        let later_set = commit_chief(&mut ctx, &mut sim, CHIEF, ymd(2026, 11, 20), ChiefType::Department);

        let rejection = expect_chief_rejection(decide(&ctx, &set_chief(OTHER, ymd(2026, 10, 15), ChiefType::Department)));
        assert_eq!(rejection.conflicting_change_set_ids, vec![later_set], "çakışan şeflik 20 Kasım kaydıyla kurulmuş");
    }

    #[test]
    fn editing_an_existing_department_is_never_refused_even_with_legacy_violation() {
        // Göçten gelen ihlal: iki öğretmen de açılıştan beri Department.
        let ctx = ContextBuilder::new(ymd(2026, 10, 12), term(ymd(2026, 9, 1), ymd(2027, 1, 31)))
            .with_teacher(CHIEF, "Ayşe Yılmaz")
            .with_teacher(OTHER, "Ali Veli")
            .with_change_set(opening_facts())
            .with_event(load_event(1, OPENING_SET, CHIEF, ymd(2026, 9, 1), ChiefType::Department, true))
            .with_event(load_event(2, OPENING_SET, OTHER, ymd(2026, 9, 1), ChiefType::Department, true))
            .build();

        let edited = set_load(CHIEF, ymd(2026, 10, 15), load(ChiefType::Department, 2));
        assert!(decide(&ctx, &edited).is_ok(), "zaten Department olan öğretmenin saat düzenlemesi geçişi değildir");
    }

    #[test]
    fn workshop_lab_has_no_limit() {
        let mut ctx = world(ChiefType::WorkshopLab);
        assert!(decide(&ctx, &set_chief(OTHER, ymd(2026, 10, 15), ChiefType::WorkshopLab)).is_ok());

        let mut sim = SimCommit::starting_at(3, 2);
        commit_chief(&mut ctx, &mut sim, OTHER, ymd(2026, 10, 15), ChiefType::WorkshopLab);
        // Atölye/lab şefi varken bir başkası alan şefi olabilir.
        let third = with_new_teacher_materialized(ctx);
        let (req, _) = new_teacher_request(ChiefType::Department);
        assert!(decide(&third, &req).is_ok());
    }

    #[test]
    fn revoking_the_leave_would_recreate_a_second_department_and_is_rejected() {
        let (mut ctx, mut sim, leave_set) = world_where_chief_leaves_on_oct_10();
        let successor_set = commit_chief(&mut ctx, &mut sim, OTHER, ymd(2026, 10, 10), ChiefType::Department);

        let revoke = ChangeRequest { term: TERM_ID.into(), effective_date: None, document_date: None, reason: "geri al".into(), command: ChangeCommand::Revoke { change_set_id: leave_set } };
        let rejection = expect_chief_rejection(decide(&ctx, &revoke));

        assert!(rejection.message.contains("Ali Veli"), "Ayşe'nin geri dönüşü Ali'nin şefliğiyle çakışıyor: {}", rejection.message);
        assert_eq!(rejection.conflicting_change_set_ids, vec![successor_set]);
    }

    #[test]
    fn revoking_the_successors_appointment_is_allowed() {
        let (mut ctx, mut sim, _) = world_where_chief_leaves_on_oct_10();
        let successor_set = commit_chief(&mut ctx, &mut sim, OTHER, ymd(2026, 10, 10), ChiefType::Department);

        let revoke = ChangeRequest { term: TERM_ID.into(), effective_date: None, document_date: None, reason: "geri al".into(), command: ChangeCommand::Revoke { change_set_id: successor_set } };
        assert!(decide(&ctx, &revoke).is_ok(), "şeflik kaldırıldığında yeni gün eklenmez");
    }

    #[test]
    fn correct_cannot_keep_the_departing_chief_in_office() {
        let (mut ctx, mut sim, leave_set) = world_where_chief_leaves_on_oct_10();
        commit_chief(&mut ctx, &mut sim, OTHER, ymd(2026, 10, 10), ChiefType::Department);

        // Yerine geçen komut, yalnız kendi başına bakıldığında (hedef yokmuş
        // gibi) Department'ın saat düzenlemesidir; net sonuç ise Ayşe'nin
        // şefliğini Ali'nin şefliğinin üstüne uzatır.
        let replacement = ChangeCommand::SetTeacherLoad { teacher_id: CHIEF, load: load(ChiefType::Department, 2) };
        let correct = ChangeRequest {
            term: TERM_ID.into(),
            effective_date: Some(ymd(2026, 10, 10)),
            document_date: None,
            reason: "düzelt".into(),
            command: ChangeCommand::Correct { change_set_id: leave_set, replacement: Box::new(replacement) },
        };
        expect_chief_rejection(decide(&ctx, &correct));
    }

    #[test]
    fn correct_that_moves_the_leave_earlier_is_allowed() {
        let (mut ctx, mut sim, leave_set) = world_where_chief_leaves_on_oct_10();
        commit_chief(&mut ctx, &mut sim, OTHER, ymd(2026, 10, 10), ChiefType::Department);

        let replacement = ChangeCommand::SetTeacherLoad { teacher_id: CHIEF, load: load(ChiefType::None, 0) };
        let correct = ChangeRequest {
            term: TERM_ID.into(),
            effective_date: Some(ymd(2026, 10, 8)),
            document_date: None,
            reason: "düzelt".into(),
            command: ChangeCommand::Correct { change_set_id: leave_set, replacement: Box::new(replacement) },
        };
        assert!(decide(&ctx, &correct).is_ok(), "ayrılış öne alınırsa Ali'nin 10 Ekim şefliğiyle çakışma olmaz");
    }
}
