//! Akış başına saf `apply` (esrs deseni — spec §3).
//!
//! Her fonksiyon girdisini değiştirmez, yeni bir değer döner. Sıralamaya
//! dayanıklı olmaları gerekir: `Some` durum taşıyan bir olay (`StudentPlaced`,
//! `HoursSet`, ...) önceki durumdan tamamen bağımsızdır — hangi sırayla
//! uygulanırsa uygulansın aynı sonucu verir. Tek istisna `HoursCapped`: o bir
//! SINIRLAMADIR, anlık görüntü değil (spec §4.2).

use super::events::{CoordinationState, EventPayload, HoursState, TeacherLoad, WeeklySchedule};

/// `placement` akışı: sonuç, öğrencinin o an bulunduğu işletme kimliğidir.
pub fn apply_placement(prev: Option<&i64>, e: &EventPayload) -> Option<i64> {
    match e {
        EventPayload::StudentPlaced { to_company_id, .. } => Some(*to_company_id),
        EventPayload::StudentTransferred { to_company_id, .. } => Some(*to_company_id),
        EventPayload::StudentLeft { .. } => None,
        _ => prev.copied(),
    }
}

/// `company_hours` akışı.
///
/// `HoursCapped` bir sınırlamadır: önceki durum yoksa uygulanacak bir şey
/// yoktur (`None`). Önceki durum kilitliyse (`is_locked`) katlama anında
/// sınırlama uygulanmaz; durum AYNEN korunur — böylece sonradan geçmiş
/// tarihle girilen bir kilit doğru sonuç verir (spec §4.2). Aksi hâlde
/// `awarded_hours` ve `max_hours_snapshot` tavana indirilir, diğer alanlar
/// (fahrilik, kilit, not) önceki durumdan aynen taşınır.
pub fn apply_hours(prev: Option<&HoursState>, e: &EventPayload) -> Option<HoursState> {
    match e {
        EventPayload::HoursSet { state, .. } => Some(state.clone()),
        EventPayload::HoursCleared { .. } => None,
        EventPayload::HoursCapped { cap, .. } => {
            let prev = prev?;
            if prev.is_locked {
                return Some(prev.clone());
            }
            Some(HoursState {
                awarded_hours: prev.awarded_hours.min(*cap),
                max_hours_snapshot: prev.max_hours_snapshot.min(*cap),
                is_honorary: prev.is_honorary,
                is_locked: prev.is_locked,
                notes: prev.notes.clone(),
            })
        }
        _ => prev.cloned(),
    }
}

/// `coordination` akışı.
pub fn apply_coordination(prev: Option<&CoordinationState>, e: &EventPayload) -> Option<CoordinationState> {
    match e {
        EventPayload::CoordinatorAssigned { state, .. } => Some(state.clone()),
        EventPayload::CoordinatorEnded { .. } | EventPayload::CoordinatorEndedByPolicy { .. } => None,
        _ => prev.cloned(),
    }
}

/// `teacher_load` akışı.
pub fn apply_load(prev: Option<&TeacherLoad>, e: &EventPayload) -> Option<TeacherLoad> {
    match e {
        EventPayload::LoadSet { load, .. } => Some(load.clone()),
        _ => prev.cloned(),
    }
}

/// `teacher_schedule` akışı.
pub fn apply_schedule(prev: Option<&WeeklySchedule>, e: &EventPayload) -> Option<WeeklySchedule> {
    match e {
        EventPayload::ScheduleSet { schedule, .. } => Some(schedule.clone()),
        _ => prev.cloned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::history::events::Labels;
    use crate::domain::models::{ChiefType, EmploymentType};
    use crate::domain::scheduling::Slot;
    use std::collections::BTreeSet;

    fn labels() -> Labels {
        Labels(Default::default())
    }

    fn hours(awarded: i64, max_snapshot: i64, locked: bool) -> HoursState {
        HoursState {
            awarded_hours: awarded,
            max_hours_snapshot: max_snapshot,
            is_honorary: false,
            is_locked: locked,
            notes: "önceki not".into(),
        }
    }

    #[test]
    fn hours_capped_clamps_down_only() {
        let prev = hours(6, 6, false);
        let capped = apply_hours(Some(&prev), &EventPayload::HoursCapped { cap: 4, student_count: 1, labels: labels() }).unwrap();
        assert_eq!(capped.awarded_hours, 4);
        assert_eq!(capped.max_hours_snapshot, 4);
    }

    /// Tavan önceki takdirin ÜSTÜNDEYSE hiçbir şey değişmemeli (yalnız düşürür).
    #[test]
    fn hours_capped_never_raises_awarded_above_previous() {
        let prev = hours(4, 4, false);
        let capped = apply_hours(Some(&prev), &EventPayload::HoursCapped { cap: 8, student_count: 3, labels: labels() }).unwrap();
        assert_eq!(capped.awarded_hours, 4, "tavan yükseldiğinde saat kendiliğinden artmaz");
    }

    #[test]
    fn hours_capped_does_not_touch_a_locked_row() {
        let prev = hours(6, 6, true);
        let result = apply_hours(Some(&prev), &EventPayload::HoursCapped { cap: 2, student_count: 1, labels: labels() }).unwrap();
        assert_eq!(result, prev, "kilitli satır katlama anında AYNEN kalmalı");
    }

    #[test]
    fn hours_capped_without_prior_state_is_none() {
        let result = apply_hours(None, &EventPayload::HoursCapped { cap: 4, student_count: 1, labels: labels() });
        assert_eq!(result, None);
    }

    #[test]
    fn hours_capped_carries_notes_and_flags() {
        let mut prev = hours(6, 6, false);
        prev.is_honorary = true;
        prev.notes = "özel not".into();
        let result = apply_hours(Some(&prev), &EventPayload::HoursCapped { cap: 3, student_count: 1, labels: labels() }).unwrap();
        assert!(result.is_honorary);
        assert_eq!(result.notes, "özel not");
    }

    /// `StudentPlaced`, önceki işletmeden bağımsız olarak yeni işletmeyi verir.
    #[test]
    fn placement_snapshot_events_ignore_prior_state() {
        let e = EventPayload::StudentPlaced { to_company_id: 9, from_company_id: None, source: "manual".into(), labels: labels() };
        assert_eq!(apply_placement(Some(&1), &e), apply_placement(None, &e));
        assert_eq!(apply_placement(Some(&1), &e), Some(9));
    }

    /// `HoursSet`, önceki takdirden bağımsız yeni durumu verir.
    #[test]
    fn hours_set_ignores_prior_state() {
        let new_state = hours(5, 5, false);
        let e = EventPayload::HoursSet { state: new_state.clone(), previous_awarded: Some(2), labels: labels() };
        let stale_prev = hours(99, 99, true);
        assert_eq!(apply_hours(Some(&stale_prev), &e), apply_hours(None, &e));
        assert_eq!(apply_hours(None, &e), Some(new_state));
    }

    /// `CoordinatorAssigned`, önceki koordinatörden bağımsız yeni durumu verir.
    #[test]
    fn coordination_assigned_ignores_prior_state() {
        let new_state = CoordinationState { teacher_id: 7, visit_day: 2, visit_hour: 4, is_forced: false, force_reason: None };
        let e = EventPayload::CoordinatorAssigned { state: new_state.clone(), from_teacher_id: Some(3), labels: labels() };
        let stale_prev = CoordinationState { teacher_id: 1, visit_day: 1, visit_hour: 1, is_forced: true, force_reason: None };
        assert_eq!(apply_coordination(Some(&stale_prev), &e), apply_coordination(None, &e));
        assert_eq!(apply_coordination(None, &e), Some(new_state));
    }

    /// `LoadSet`, önceki yükten bağımsız yeni durumu verir.
    #[test]
    fn load_set_ignores_prior_state() {
        let new_load = TeacherLoad { base_hours: 15, max_extra_hours: 24, other_extra_hours: 2, chief_type: ChiefType::Department, employment_type: EmploymentType::Tenured };
        let e = EventPayload::LoadSet { load: new_load.clone(), previous: None, source: "manual".into(), labels: labels() };
        let stale_prev = TeacherLoad { base_hours: 0, max_extra_hours: 0, other_extra_hours: 0, chief_type: ChiefType::None, employment_type: EmploymentType::Contracted };
        assert_eq!(apply_load(Some(&stale_prev), &e), apply_load(None, &e));
        assert_eq!(apply_load(None, &e), Some(new_load));
    }

    /// `ScheduleSet`, önceki programdan bağımsız yeni durumu verir.
    #[test]
    fn schedule_set_ignores_prior_state() {
        let new_schedule = WeeklySchedule(BTreeSet::from([Slot::new(1, 3)]));
        let e = EventPayload::ScheduleSet { schedule: new_schedule.clone(), previous_slot_count: None, source: "manual".into(), labels: labels() };
        let stale_prev = WeeklySchedule(BTreeSet::from([Slot::new(5, 8)]));
        assert_eq!(apply_schedule(Some(&stale_prev), &e), apply_schedule(None, &e));
        assert_eq!(apply_schedule(None, &e), Some(new_schedule));
    }
}
