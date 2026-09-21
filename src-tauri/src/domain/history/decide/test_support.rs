//! Karar testleri için ortak fixture kurucuları. Yalnız `cfg(test)`'te
//! derlenir; üretim kodunda çağıranı YOKTUR ve olmamalıdır.

use std::collections::BTreeMap;

use chrono::NaiveDate;

use super::{ChangeSetFacts, CompanyFacts, Decision, DecisionContext, Materialized};
use crate::domain::history::events::{EventPayload, Stream, StoredEvent, WeeklySchedule};
use crate::domain::hour_rules::HourRule;
use crate::domain::terms::TermDates;

pub(super) fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

pub(super) fn term(start: NaiveDate, end: NaiveDate) -> TermDates {
    TermDates { term: "2026-2027/1".into(), start, end, dates_confirmed: true }
}

pub(super) fn rule(id: i64, min_km: f64, max_km: Option<f64>, min_students: i64, max_students: Option<i64>, max_hours: i64) -> HourRule {
    HourRule { id, min_distance_km: min_km, max_distance_km: max_km, min_students, max_students, max_hours }
}

pub(super) fn stored_event(
    id: i64,
    change_set_id: i64,
    stream: Stream,
    subject_id: i64,
    term: &str,
    effective_date: NaiveDate,
    payload: EventPayload,
    is_opening: bool,
) -> StoredEvent {
    StoredEvent {
        id,
        change_set_id,
        stream,
        subject_id,
        term: term.to_string(),
        effective_date,
        payload,
        caused_by: None,
        revokes: None,
        is_opening,
    }
}

/// Testlerde `DecisionContext` kurmak için küçük bir builder. Üretimde
/// bu işi `db::history_context::load` (R3) yapar.
#[derive(Clone)]
pub(super) struct ContextBuilder {
    ctx: DecisionContext,
}

impl ContextBuilder {
    pub(super) fn new(today: NaiveDate, term: TermDates) -> Self {
        Self {
            ctx: DecisionContext {
                today,
                term,
                rules: Vec::new(),
                statutory_cap: 20,
                day_end_hour: 17,
                pool_hours: 0,
                companies: BTreeMap::new(),
                student_names: BTreeMap::new(),
                teacher_names: BTreeMap::new(),
                events: Vec::new(),
                change_sets: BTreeMap::new(),
                high_water: 0,
                materialized: Materialized::default(),
                source_schedules: BTreeMap::new(),
            },
        }
    }

    pub(super) fn with_rules(mut self, rules: Vec<HourRule>) -> Self {
        self.ctx.rules = rules;
        self
    }

    /// Havuz aşım denetimi testleri için (`domain::hour_distribution::pool_overrun_reason`).
    pub(super) fn with_pool_hours(mut self, pool_hours: i64) -> Self {
        self.ctx.pool_hours = pool_hours;
        self
    }

    pub(super) fn with_company(mut self, id: i64, name: &str, round_trip_km: Option<f64>) -> Self {
        self.ctx.companies.insert(id, CompanyFacts { name: name.to_string(), round_trip_km, is_active: true });
        self
    }

    pub(super) fn with_student(mut self, id: i64, name: &str) -> Self {
        self.ctx.student_names.insert(id, name.to_string());
        self
    }

    pub(super) fn with_teacher(mut self, id: i64, name: &str) -> Self {
        self.ctx.teacher_names.insert(id, name.to_string());
        self
    }

    pub(super) fn with_event(mut self, event: StoredEvent) -> Self {
        self.ctx.events.push(event);
        self
    }

    pub(super) fn with_change_set(mut self, facts: ChangeSetFacts) -> Self {
        self.ctx.change_sets.insert(facts.id, facts);
        self
    }

    pub(super) fn materialized(mut self, m: Materialized) -> Self {
        self.ctx.materialized = m;
        self
    }

    /// `copySchedulesFromTerm` testleri için: kaynak dönemde bir öğretmenin
    /// son geçerli programı (R2b brief madde 1).
    pub(super) fn with_source_schedule(mut self, teacher_id: i64, schedule: WeeklySchedule) -> Self {
        self.ctx.source_schedules.insert(teacher_id, schedule);
        self
    }

    /// Pasif işletme testleri için (`InvalidRequest`, R2b brief madde 3).
    pub(super) fn with_inactive_company(mut self, id: i64, name: &str, round_trip_km: Option<f64>) -> Self {
        self.ctx.companies.insert(id, CompanyFacts { name: name.to_string(), round_trip_km, is_active: false });
        self
    }

    pub(super) fn build(self) -> DecisionContext {
        self.ctx
    }
}

/// Birden çok `decide()` çağrısını art arda "kaydeden" küçük bir simülatör:
/// gerçek kalıcılığın (R3) yaptığı id atama ve `caused_by` çözümlemesini
/// TESTTE taklit eder, böylece çok adımlı senaryolar (Kasım senaryosu gibi)
/// tek bir `DecisionContext` üzerinde ilerletilebilir.
pub(super) struct SimCommit {
    next_event_id: i64,
    next_change_set_id: i64,
}

impl SimCommit {
    pub(super) fn new() -> Self {
        Self { next_event_id: 1, next_change_set_id: 1 }
    }

    /// Test fixture'ı `ContextBuilder` ile elle seçilmiş kimliklerle
    /// tohumlanmışsa, çakışmayı önlemek için sayaçları oradan devam ettirir.
    pub(super) fn starting_at(next_event_id: i64, next_change_set_id: i64) -> Self {
        Self { next_event_id, next_change_set_id }
    }

    /// Kararı `ctx`'e "yazar" ve oluşan `change_set_id`'yi döner.
    pub(super) fn apply(&mut self, ctx: &mut DecisionContext, decision: Decision) -> i64 {
        let change_set_id = self.next_change_set_id;
        self.next_change_set_id += 1;

        if let Some(revoked) = decision.change_set.revokes_change_set_id {
            if let Some(facts) = ctx.change_sets.get_mut(&revoked) {
                facts.revoked_by = Some(change_set_id);
            }
        }
        ctx.change_sets.insert(
            change_set_id,
            ChangeSetFacts {
                id: change_set_id,
                kind: decision.change_set.kind.clone(),
                effective_date: decision.change_set.effective_date,
                revokes_change_set_id: decision.change_set.revokes_change_set_id,
                revoked_by: None,
            },
        );

        let ids: Vec<i64> = decision
            .events
            .iter()
            .map(|_| {
                let id = self.next_event_id;
                self.next_event_id += 1;
                id
            })
            .collect();

        for (planned, &id) in decision.events.iter().zip(ids.iter()) {
            ctx.events.push(StoredEvent {
                id,
                change_set_id,
                stream: planned.stream,
                subject_id: planned.subject_id,
                term: ctx.term.term.clone(),
                effective_date: planned.effective_date,
                payload: planned.payload.clone(),
                caused_by: planned.caused_by.map(|idx| ids[idx]),
                revokes: planned.revokes,
                is_opening: decision.change_set.kind == "opening",
            });
        }
        ctx.high_water = self.next_event_id - 1;
        change_set_id
    }
}
