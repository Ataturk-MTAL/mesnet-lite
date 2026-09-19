//! Denetim listesinin "önce → sonra" betimi (spec §6). "Önce" değeri,
//! katlamanın `(effective_date, id)` ÖNCESİNDEKİ durumudur — `decide`'ın ve
//! `policy`'nin kullandığı AYNI (date, id) sırasını izler, ama GERİ ALINAN
//! olayları da (üstü çizilecek şekilde `is_revoked=true` ile) listede
//! TUTAR; `order_events`'in aksine onları katlamadan ATMAZ.
//!
//! Basit metin gösterimi: `before`/`after` `Debug` biçimindedir. Güzel
//! Türkçe biçimlendirme R4/R5'teki arayüz katmanına bırakılmıştır.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;

use super::apply::{apply_coordination, apply_hours, apply_load, apply_placement, apply_schedule};
use super::events::{CoordinationState, EventPayload, HoursState, Labels, Stream, StoredEvent, TeacherLoad, WeeklySchedule};

/// Denetim listesindeki tek bir satır.
#[derive(Debug, Clone, PartialEq)]
pub struct AuditEvent {
    pub event_id: i64,
    pub stream: String,
    pub subject_id: i64,
    pub subject_label: String,
    pub kind: String,
    pub effective_date: NaiveDate,
    pub before: Option<String>,
    pub after: Option<String>,
    pub caused_by_event_id: Option<i64>,
    pub is_revoked: bool,
}

/// `change_set_ids` içindeki kümelere ait olayları, önce/sonra betimiyle
/// döner. `events` genellikle dönemin TÜMÜ (filtresiz) olmalıdır — "önce"
/// değerinin doğru hesaplanması, hedef dışındaki olayları da bilmeyi
/// gerektirir.
pub fn describe(events: &[StoredEvent], term_start: NaiveDate, change_set_ids: &[i64]) -> Vec<AuditEvent> {
    let target_ids: BTreeSet<i64> = change_set_ids.iter().copied().collect();
    let revoked_ids: BTreeSet<i64> = events.iter().filter_map(|e| e.revokes).collect();

    let mut by_subject: BTreeMap<(Stream, i64), Vec<StoredEvent>> = BTreeMap::new();
    for e in events {
        by_subject.entry((e.stream, e.subject_id)).or_default().push(e.clone());
    }

    let mut out = Vec::new();
    for ((stream, subject_id), subject_events) in by_subject {
        out.extend(describe_subject(stream, subject_id, &subject_events, term_start, &target_ids, &revoked_ids));
    }
    out.sort_by_key(|a| a.event_id);
    out
}

fn describe_subject(
    stream: Stream,
    subject_id: i64,
    subject_events: &[StoredEvent],
    term_start: NaiveDate,
    target_ids: &BTreeSet<i64>,
    revoked_ids: &BTreeSet<i64>,
) -> Vec<AuditEvent> {
    match stream {
        Stream::Placement => fold_and_describe::<i64>(subject_id, subject_events, term_start, target_ids, revoked_ids, apply_placement),
        Stream::CompanyHours => fold_and_describe::<HoursState>(subject_id, subject_events, term_start, target_ids, revoked_ids, apply_hours),
        Stream::Coordination => fold_and_describe::<CoordinationState>(subject_id, subject_events, term_start, target_ids, revoked_ids, apply_coordination),
        Stream::TeacherLoad => fold_and_describe::<TeacherLoad>(subject_id, subject_events, term_start, target_ids, revoked_ids, apply_load),
        Stream::TeacherSchedule => fold_and_describe::<WeeklySchedule>(subject_id, subject_events, term_start, target_ids, revoked_ids, apply_schedule),
    }
}

/// Açılış olayları dönem başına, diğerleri kendi tarihine yerleşir — aynı
/// `timeline::order_events` kuralı (spec §5.1), ama burada REVOKE edilenler
/// de dizide kalır.
fn effective_sort_date(e: &StoredEvent, term_start: NaiveDate) -> NaiveDate {
    if e.is_opening {
        term_start
    } else {
        e.effective_date
    }
}

#[allow(clippy::too_many_arguments)]
fn fold_and_describe<S: Clone + PartialEq + std::fmt::Debug>(
    subject_id: i64,
    subject_events: &[StoredEvent],
    term_start: NaiveDate,
    target_ids: &BTreeSet<i64>,
    revoked_ids: &BTreeSet<i64>,
    apply: impl Fn(Option<&S>, &EventPayload) -> Option<S>,
) -> Vec<AuditEvent> {
    let mut sorted: Vec<&StoredEvent> = subject_events.iter().collect();
    sorted.sort_by_key(|e| (effective_sort_date(e, term_start), e.id));

    let mut out = Vec::new();
    let mut state: Option<S> = None;
    for e in sorted {
        let before = state.as_ref().map(|s| format!("{s:?}"));
        let next = apply(state.as_ref(), &e.payload);
        if target_ids.contains(&e.change_set_id) {
            out.push(AuditEvent {
                event_id: e.id,
                stream: e.stream.as_str().to_string(),
                subject_id,
                subject_label: subject_label_of(&e.payload),
                kind: e.payload.kind().to_string(),
                effective_date: e.effective_date,
                before,
                after: next.as_ref().map(|s| format!("{s:?}")),
                caused_by_event_id: e.caused_by,
                is_revoked: revoked_ids.contains(&e.id),
            });
        }
        state = next;
    }
    out
}

/// Olayın kendi `labels` alanındaki İLK ad — kayıt anındaki anlık görüntü
/// (spec §4.2, "labels"). `Revoked`'in yükü yoktur; boş döner.
fn subject_label_of(payload: &EventPayload) -> String {
    fn first(labels: &Labels) -> String {
        labels.0.values().next().cloned().unwrap_or_default()
    }
    match payload {
        EventPayload::StudentPlaced { labels, .. }
        | EventPayload::StudentTransferred { labels, .. }
        | EventPayload::StudentLeft { labels, .. }
        | EventPayload::HoursSet { labels, .. }
        | EventPayload::HoursCapped { labels, .. }
        | EventPayload::HoursCleared { labels, .. }
        | EventPayload::CoordinatorAssigned { labels, .. }
        | EventPayload::CoordinatorEnded { labels, .. }
        | EventPayload::CoordinatorEndedByPolicy { labels, .. }
        | EventPayload::LoadSet { labels, .. }
        | EventPayload::ScheduleSet { labels, .. } => first(labels),
        EventPayload::Revoked => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn labels(pairs: &[(&str, &str)]) -> Labels {
        Labels(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }

    fn stored(id: i64, change_set_id: i64, effective_date: NaiveDate, payload: EventPayload, is_opening: bool) -> StoredEvent {
        StoredEvent { id, change_set_id, stream: Stream::Placement, subject_id: 100, term: "2026-2027/1".into(), effective_date, payload, caused_by: None, revokes: None, is_opening }
    }

    #[test]
    fn describe_shows_before_and_after_from_fold() {
        let opening = stored(1, 1, ymd(2026, 9, 1), EventPayload::StudentPlaced { to_company_id: 10, from_company_id: None, source: "opening".into(), labels: labels(&[("student", "Ahmet Yılmaz")]) }, true);
        let transfer = stored(2, 2, ymd(2026, 11, 3), EventPayload::StudentTransferred { from_company_id: 10, to_company_id: 20, to_company_created: false, labels: labels(&[("student", "Ahmet Yılmaz")]) }, false);

        let events = vec![opening, transfer];
        let entries = describe(&events, ymd(2026, 9, 1), &[1, 2]);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].event_id, 1);
        assert_eq!(entries[0].before, None, "ilk olayın öncesi boş olmalı");
        assert_eq!(entries[0].after, Some("10".to_string()));
        assert_eq!(entries[0].subject_label, "Ahmet Yılmaz");

        assert_eq!(entries[1].event_id, 2);
        assert_eq!(entries[1].before, Some("10".to_string()), "ikinci olayın öncesi, birincinin fold sonucu olmalı");
        assert_eq!(entries[1].after, Some("20".to_string()));
        assert!(!entries[1].is_revoked);
    }

    /// Geri alınan bir olay listede KALIR (üstü çizilecek şekilde işaretlenir);
    /// `order_events`'in aksine atılmaz.
    #[test]
    fn describe_marks_revoked_events_without_dropping_them() {
        let opening = stored(1, 1, ymd(2026, 9, 1), EventPayload::StudentPlaced { to_company_id: 10, from_company_id: None, source: "opening".into(), labels: labels(&[("student", "Ahmet Yılmaz")]) }, true);
        let mut transfer = stored(2, 2, ymd(2026, 11, 3), EventPayload::StudentTransferred { from_company_id: 10, to_company_id: 20, to_company_created: false, labels: labels(&[("student", "Ahmet Yılmaz")]) }, false);
        transfer.revokes = None;
        let mut revoke_marker = stored(3, 3, ymd(2026, 11, 5), EventPayload::Revoked, false);
        revoke_marker.revokes = Some(2);

        let events = vec![opening, transfer, revoke_marker];
        let entries = describe(&events, ymd(2026, 9, 1), &[1, 2, 3]);

        assert_eq!(entries.len(), 3);
        let transfer_entry = entries.iter().find(|e| e.event_id == 2).unwrap();
        assert!(transfer_entry.is_revoked, "geri alınan olay is_revoked=true ile kalmalı");
    }
}
