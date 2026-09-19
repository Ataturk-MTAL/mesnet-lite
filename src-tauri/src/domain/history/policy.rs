//! Zincir etki politikası — spec §5.3, "otomatik uygula ve bildir".
//!
//! Saf fonksiyonlardır: DB'ye dokunmazlar, `Timeline` girdisi alır ve
//! `decide/company.rs`'in olaya çevireceği bir plan döner. `hours_capped`
//! bir sınırlamadır (bkz. `apply.rs`); bu modül yalnız KARARI üretir, katlama
//! sırasında yeniden türetilmez (spec §4.2).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use chrono::NaiveDate;

use super::events::{CoordinationState, HoursState, TeacherLoad, WeeklySchedule};
use super::impact::{ImpactNotice, ImpactWarning, NoticeCode, WarningCode};
use super::timeline::Timeline;
use crate::domain::hour_rules::{select_narrowest, HourRule};
use crate::domain::scheduling::Block;
use crate::domain::validation::{check_teacher_totals, ViolationCode};
use crate::domain::workload::teacher_capacity;

/// Bir işletmenin, verilen öğrenci sayısı ve gidiş-dönüş mesafesiyle uyan EN
/// DAR saat tavanı. 0 öğrenci için tavan her zaman 0'dır (kural tablosuna
/// bakılmaz); mesafe bilinmiyorsa ya da hiçbir kural uymuyorsa `None` döner —
/// "tavan bilinmiyor" ile "tavan sıfır" birbirine karıştırılmaz.
pub fn cap_for(rules: &[HourRule], round_trip_km: Option<f64>, student_count: i64) -> Option<i64> {
    if student_count == 0 {
        return Some(0);
    }
    let km = round_trip_km?;
    select_narrowest(rules, km, student_count).map(|rule| rule.max_hours)
}

/// Bir işletme için zincir etki yürüyüşünün çıktısı. `decide/company.rs`
/// bunu `caused_by` ile birincil olaya bağlı `PlannedEvent`'lere çevirir.
#[derive(Debug, Clone, Default)]
pub struct CompanyPolicyPlan {
    /// `(tarih, yeni tavan, o tarihteki öğrenci sayısı)` — her biri bir
    /// `HoursCapped` olayına dönüşür.
    pub reductions: Vec<(NaiveDate, i64, i64)>,
    /// Öğrenci kalmayınca koordinatörün bittiği tarih (varsa).
    pub coordinator_end: Option<NaiveDate>,
    pub warnings: Vec<ImpactWarning>,
    pub notices: Vec<ImpactNotice>,
}

/// `from` tarihinden itibaren, öğrenci sayısının, saatin ve koordinasyonun
/// değiştiği HER tarihte tavanı yeniden değerlendirir (spec §5.3).
///
/// `capped_history`: bu işletmede daha önce (bu karardan önce) gerçekleşmiş
/// `hours_capped` olaylarının tarihleri — `reducedBelowCap` bildirimi için.
pub fn plan_company_policies(
    label: &str,
    round_trip_km: Option<f64>,
    counts: &Timeline<i64>,
    hours: &Timeline<HoursState>,
    coordination: &Timeline<CoordinationState>,
    capped_history: &[NaiveDate],
    rules: &[HourRule],
    from: NaiveDate,
) -> CompanyPolicyPlan {
    let mut plan = CompanyPolicyPlan::default();
    let walk_dates = collect_walk_dates(from, counts, hours, coordination);
    let mut last_known_cap: Option<i64> = None;
    let was_capped_before = !capped_history.is_empty();

    for (idx, &b) in walk_dates.iter().enumerate() {
        let student_count = counts.state_at(b).copied().unwrap_or(0);
        let cap = if student_count == 0 { Some(0) } else { cap_for(rules, round_trip_km, student_count) };
        let to_date = walk_dates.get(idx + 1).copied();

        if let Some(state) = hours.state_at(b) {
            apply_hours_policy(label, state, cap, student_count, b, to_date, was_capped_before, &mut plan);
        }

        if student_count == 0 && coordination.state_at(b).is_some() {
            plan.coordinator_end.get_or_insert(b);
        }

        note_cap_increase(label, &mut last_known_cap, cap, b, &mut plan);
    }

    plan
}

/// `from` ve sonrasında en az bir akışta değişiklik olan her tarih, tekil ve
/// sıralı. `from`'un kendisi her zaman dahildir: karar bu tarihte alınıyor.
fn collect_walk_dates(
    from: NaiveDate,
    counts: &Timeline<i64>,
    hours: &Timeline<HoursState>,
    coordination: &Timeline<CoordinationState>,
) -> Vec<NaiveDate> {
    let mut dates: BTreeSet<NaiveDate> = BTreeSet::from([from]);
    dates.extend(counts.change_dates_from(from));
    dates.extend(hours.change_dates_from(from));
    dates.extend(coordination.change_dates_from(from));
    dates.into_iter().collect()
}

/// Tek bir tarihteki saat durumunu tavana göre değerlendirir: fahri satır
/// dokunulmaz, kilitli+aşan satır yalnız uyarır, aksi hâlde bir düşürme
/// planlanır. Tavan bilinmiyorsa ve saat sıfırdan büyükse `capUnknown` uyarır.
#[allow(clippy::too_many_arguments)]
fn apply_hours_policy(
    label: &str,
    state: &HoursState,
    cap: Option<i64>,
    student_count: i64,
    at: NaiveDate,
    to_date: Option<NaiveDate>,
    was_capped_before: bool,
    plan: &mut CompanyPolicyPlan,
) {
    let Some(cap) = cap else {
        if state.awarded_hours > 0 {
            plan.warnings.push(ImpactWarning {
                code: WarningCode::CapUnknown,
                message: format!("{label}: gidiş-dönüş mesafesi ya da uyan bir saat kuralı olmadığı için tavan hesaplanamadı"),
                subject_label: label.to_string(),
                from_date: at,
                to_date,
            });
        }
        return;
    };

    if state.is_honorary || state.awarded_hours <= cap {
        if state.awarded_hours < cap && was_capped_before {
            plan.notices.push(ImpactNotice {
                code: NoticeCode::ReducedBelowCap,
                message: format!(
                    "{label}: saat daha önce otomatik düşürülmüştü, şimdi {cap} saatlik tavanın altında kaldı; elle geri verebilirsiniz"
                ),
                subject_label: label.to_string(),
                date: at,
            });
        }
        return;
    }

    if state.is_locked {
        plan.warnings.push(ImpactWarning {
            code: WarningCode::LockedAboveCap,
            message: format!(
                "{label}: kilitli satır {} saat, yeni tavan {cap} saati aşıyor",
                state.awarded_hours
            ),
            subject_label: label.to_string(),
            from_date: at,
            to_date,
        });
    } else {
        plan.reductions.push((at, cap, student_count));
    }
}

/// Bilinen tavan bir öncekinden yüksekse "yalnız bildirim" kuralı (spec §5.3):
/// saat kendiliğinden artmaz, kullanıcı isterse elle yükseltir.
fn note_cap_increase(
    label: &str,
    last_known_cap: &mut Option<i64>,
    cap: Option<i64>,
    at: NaiveDate,
    plan: &mut CompanyPolicyPlan,
) {
    if let (Some(last), Some(current)) = (*last_known_cap, cap) {
        if current > last {
            plan.notices.push(ImpactNotice {
                code: NoticeCode::CapIncreased,
                message: format!("{label}: tavan {last} saatten {current} saate yükseldi; saat kendiliğinden artmaz"),
                subject_label: label.to_string(),
                date: at,
            });
        }
    }
    if cap.is_some() {
        *last_known_cap = cap;
    }
}

/// Öğretmenin haftalık ve günlük ek ders kapasitesi bayrakları (spec §5.3,
/// "Öğretmen tarafı"). Kural yeniden yazılmaz: `workload::teacher_capacity`
/// ve `validation::check_teacher_totals` yeniden kullanılır.
pub fn capacity_flags(
    teacher_label: &str,
    teacher_id: i64,
    load: &Timeline<TeacherLoad>,
    assigned_hours: &Timeline<i64>,
    hours_by_day: &dyn Fn(NaiveDate) -> BTreeMap<i64, i64>,
    statutory_cap: i64,
    from: NaiveDate,
) -> Vec<ImpactWarning> {
    let mut dates: BTreeSet<NaiveDate> = BTreeSet::from([from]);
    dates.extend(load.change_dates_from(from));
    dates.extend(assigned_hours.change_dates_from(from));
    let dates: Vec<NaiveDate> = dates.into_iter().collect();

    let mut warnings = Vec::new();
    for (idx, &at) in dates.iter().enumerate() {
        let Some(teacher_load) = load.state_at(at) else { continue };
        let capacity = teacher_capacity(teacher_load, statutory_cap);
        let awarded = assigned_hours.state_at(at).copied().unwrap_or(0);
        let day_hours = hours_by_day(at);
        let to_date = dates.get(idx + 1).copied();
        let violations = check_teacher_totals(teacher_id, teacher_label, capacity, awarded, &day_hours, false);
        warnings.extend(violations.into_iter().filter_map(|v| {
            let code = match v.code {
                ViolationCode::WeeklyCapExceeded => WarningCode::CapacityExceeded,
                ViolationCode::DailyCapExceeded => WarningCode::DailyCapExceeded,
                _ => return None,
            };
            Some(ImpactWarning { code, message: v.message, subject_label: teacher_label.to_string(), from_date: at, to_date })
        }));
    }
    warnings
}

/// Program değişince, zorlanmamış blokların hâlâ boş saatlerin içinde olup
/// olmadığını bayraklar (spec §5.3). `blocks` yalnız ZORLANMAMIŞ atamaları
/// içermelidir — zorlama kullanıcının bilinçli kararıdır, burada tekrar
/// bayraklanmaz.
pub fn schedule_flags(
    teacher_label: &str,
    schedule: &Timeline<WeeklySchedule>,
    blocks: &[(String, Timeline<Block>)],
    from: NaiveDate,
) -> Vec<ImpactWarning> {
    let mut dates: BTreeSet<NaiveDate> = BTreeSet::from([from]);
    dates.extend(schedule.change_dates_from(from));
    for (_, block_timeline) in blocks {
        dates.extend(block_timeline.change_dates_from(from));
    }
    let dates: Vec<NaiveDate> = dates.into_iter().collect();

    let mut warnings = Vec::new();
    for (idx, &at) in dates.iter().enumerate() {
        let Some(free) = schedule.state_at(at) else { continue };
        let to_date = dates.get(idx + 1).copied();
        for (company_label, block_timeline) in blocks {
            let Some(block) = block_timeline.state_at(at) else { continue };
            let outside = block.cells().iter().any(|cell| !free.0.contains(cell));
            if outside {
                warnings.push(ImpactWarning {
                    code: WarningCode::BlockOutsideFreeSlots,
                    message: format!("{teacher_label}: {company_label} ziyaret bloğu artık boş saatlerin dışında"),
                    subject_label: teacher_label.to_string(),
                    from_date: at,
                    to_date,
                });
            }
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::history::events::Labels;
    use crate::domain::models::{ChiefType, EmploymentType};
    use crate::domain::scheduling::Slot;
    use std::collections::BTreeSet as Set;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn rule(id: i64, min_km: f64, max_km: Option<f64>, min_students: i64, max_students: Option<i64>, max_hours: i64) -> HourRule {
        HourRule { id, min_distance_km: min_km, max_distance_km: max_km, min_students, max_students, max_hours }
    }

    fn hours(awarded: i64, locked: bool, honorary: bool) -> HoursState {
        HoursState { awarded_hours: awarded, max_hours_snapshot: awarded, is_honorary: honorary, is_locked: locked, notes: String::new() }
    }

    fn counts_timeline(pairs: &[(NaiveDate, Option<i64>)]) -> Timeline<i64> {
        timeline_from_pairs(pairs)
    }

    fn hours_timeline(pairs: &[(NaiveDate, Option<HoursState>)]) -> Timeline<HoursState> {
        timeline_from_pairs(pairs)
    }

    fn coordination_timeline(pairs: &[(NaiveDate, Option<CoordinationState>)]) -> Timeline<CoordinationState> {
        timeline_from_pairs(pairs)
    }

    /// Testlerde doğrudan `Interval` listesi kurmak için küçük bir yardımcı:
    /// `fold_intervals`'ın gerçek olay akışı olmadan, önceden bilinen
    /// (tarih, durum) çiftlerinden yarı açık aralıklar üretir.
    fn timeline_from_pairs<S: Clone + PartialEq>(pairs: &[(NaiveDate, Option<S>)]) -> Timeline<S> {
        use crate::domain::history::timeline::Interval;
        let mut intervals: Vec<Interval<S>> = Vec::new();
        for (date, state) in pairs {
            if let Some(last) = intervals.last_mut() {
                last.valid_to = Some(*date);
            }
            if let Some(s) = state {
                intervals.push(Interval { valid_from: *date, valid_to: None, state: s.clone(), source_event_id: 0 });
            }
        }
        Timeline { intervals }
    }

    fn empty_coordination() -> Timeline<CoordinationState> {
        coordination_timeline(&[])
    }

    #[test]
    fn cap_for_zero_students_is_always_zero() {
        let rules = vec![rule(1, 0.0, None, 1, None, 8)];
        assert_eq!(cap_for(&rules, Some(10.0), 0), Some(0));
    }

    #[test]
    fn cap_unknown_warns_when_distance_missing() {
        let counts = counts_timeline(&[(ymd(2026, 11, 3), Some(3))]);
        let hours = hours_timeline(&[(ymd(2026, 11, 3), Some(hours(4, false, false)))]);
        let plan = plan_company_policies("İşletme A", None, &counts, &hours, &empty_coordination(), &[], &[], ymd(2026, 11, 3));
        assert!(plan.reductions.is_empty());
        assert_eq!(plan.warnings.len(), 1);
        assert_eq!(plan.warnings[0].code, WarningCode::CapUnknown);
    }

    /// Öğrenci ayrılınca tavan aynı tarihte düşer; kilitsiz satır tavana iner.
    #[test]
    fn leave_lowers_unlocked_hours_to_new_cap_on_same_date() {
        let rules = vec![rule(1, 0.0, None, 1, None, 4)];
        let counts = counts_timeline(&[(ymd(2026, 11, 3), Some(1))]);
        let hours = hours_timeline(&[(ymd(2026, 11, 3), Some(hours(6, false, false)))]);
        let plan = plan_company_policies("İşletme A", Some(10.0), &counts, &hours, &empty_coordination(), &[], &rules, ymd(2026, 11, 3));
        assert_eq!(plan.reductions, vec![(ymd(2026, 11, 3), 4, 1)]);
    }

    #[test]
    fn locked_row_only_warns() {
        let rules = vec![rule(1, 0.0, None, 1, None, 4)];
        let counts = counts_timeline(&[(ymd(2026, 11, 3), Some(1))]);
        let hours = hours_timeline(&[(ymd(2026, 11, 3), Some(hours(6, true, false)))]);
        let plan = plan_company_policies("İşletme A", Some(10.0), &counts, &hours, &empty_coordination(), &[], &rules, ymd(2026, 11, 3));
        assert!(plan.reductions.is_empty(), "kilitli satır düşürülmez");
        assert_eq!(plan.warnings[0].code, WarningCode::LockedAboveCap);
    }

    #[test]
    fn zero_students_caps_to_zero_and_ends_coordinator() {
        let rules = vec![rule(1, 0.0, None, 1, None, 6)];
        let counts = counts_timeline(&[(ymd(2026, 11, 3), Some(0))]);
        let hours = hours_timeline(&[(ymd(2026, 9, 1), Some(hours(6, false, false)))]);
        let coordination = coordination_timeline(&[(
            ymd(2026, 9, 1),
            Some(CoordinationState { teacher_id: 5, visit_day: 1, visit_hour: 3, is_forced: false, force_reason: None }),
        )]);
        let plan = plan_company_policies("İşletme A", Some(10.0), &counts, &hours, &coordination, &[], &rules, ymd(2026, 11, 3));
        assert_eq!(plan.reductions, vec![(ymd(2026, 11, 3), 0, 0)]);
        assert_eq!(plan.coordinator_end, Some(ymd(2026, 11, 3)));
    }

    #[test]
    fn cap_increase_is_notice_only() {
        let rules = vec![
            rule(1, 0.0, None, 1, Some(1), 4),
            rule(2, 0.0, None, 2, None, 8),
        ];
        let counts = counts_timeline(&[(ymd(2026, 9, 1), Some(1)), (ymd(2026, 11, 3), Some(2))]);
        let hours = hours_timeline(&[(ymd(2026, 9, 1), Some(hours(4, false, false)))]);
        let plan = plan_company_policies("İşletme A", Some(10.0), &counts, &hours, &empty_coordination(), &[], &rules, ymd(2026, 9, 1));
        assert!(plan.reductions.is_empty(), "tavan yükselince saat kendiliğinden artmaz");
        assert!(plan.notices.iter().any(|n| n.code == NoticeCode::CapIncreased));
    }

    /// İki ayrılıştan biri geri alınınca tavan tekrar yükselir ama saat
    /// otomatik düşürüldüğü için hâlâ tavanın altındadır — elle geri verilebilir.
    #[test]
    fn reduced_below_cap_notice_after_other_cause_revoked() {
        let rules = vec![rule(1, 0.0, None, 1, None, 8)];
        let counts = counts_timeline(&[(ymd(2026, 11, 3), Some(3))]);
        let hours = hours_timeline(&[(ymd(2026, 11, 3), Some(hours(4, false, false)))]);
        let capped_history = vec![ymd(2026, 10, 1)];
        let plan = plan_company_policies("İşletme A", Some(10.0), &counts, &hours, &empty_coordination(), &capped_history, &rules, ymd(2026, 11, 3));
        assert!(plan.reductions.is_empty());
        assert!(plan.notices.iter().any(|n| n.code == NoticeCode::ReducedBelowCap));
    }

    /// Yürüyüş yalnız `from` tarihinde durmaz: sonraki bir manuel yükseltme
    /// KENDİ tarihinde de tavana göre denetlenir.
    #[test]
    fn walk_caps_later_manual_raise_at_its_own_date() {
        let rules = vec![rule(1, 0.0, None, 1, None, 4)];
        let counts = counts_timeline(&[(ymd(2026, 11, 3), Some(1))]);
        let hours = hours_timeline(&[
            (ymd(2026, 11, 3), Some(hours(4, false, false))),
            (ymd(2026, 12, 1), Some(hours(6, false, false))), // sonradan elle 6'ya çıkarılmış
        ]);
        let plan = plan_company_policies("İşletme A", Some(10.0), &counts, &hours, &empty_coordination(), &[], &rules, ymd(2026, 11, 3));
        assert_eq!(plan.reductions, vec![(ymd(2026, 12, 1), 4, 1)], "12/1'deki satır da kendi tarihinde tavana göre denetlenmeli");
    }

    /// 24/10/4 → kapasite 10 (bkz. `workload::teacher_capacity_matches_coordinator_capacity`);
    /// önceki kapasite 14'tü ve atanan saat 14 ile UYUMLUYDU — düşürülmez, yalnız bayraklanır.
    #[test]
    fn other_extra_hours_rise_flags_capacity_without_drop() {
        let before = TeacherLoad { base_hours: 15, max_extra_hours: 24, other_extra_hours: 0, chief_type: ChiefType::Department, employment_type: EmploymentType::Tenured };
        let after = TeacherLoad { base_hours: 15, max_extra_hours: 24, other_extra_hours: 4, chief_type: ChiefType::Department, employment_type: EmploymentType::Tenured };
        let load = timeline_from_pairs(&[(ymd(2026, 9, 1), Some(before)), (ymd(2026, 11, 5), Some(after))]);
        let assigned = timeline_from_pairs(&[(ymd(2026, 9, 1), Some(14i64))]);
        let warnings = capacity_flags("Ali Öğretmen", 7, &load, &assigned, &|_| BTreeMap::new(), 20, ymd(2026, 9, 1));
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code, WarningCode::CapacityExceeded);
        assert_eq!(warnings[0].from_date, ymd(2026, 11, 5));
    }

    #[test]
    fn schedule_change_flags_blocks_outside_free_slots() {
        let free_before = WeeklySchedule(Set::from([Slot::new(1, 9), Slot::new(1, 10)]));
        let free_after = WeeklySchedule(Set::from([Slot::new(2, 9)])); // pazartesi artık boş değil
        let schedule = timeline_from_pairs(&[(ymd(2026, 9, 1), Some(free_before)), (ymd(2026, 11, 5), Some(free_after))]);
        let block = Block::from_start(1, 9, 2); // pazartesi 9-10, tek blok her zaman geçerli
        let blocks = vec![("İşletme A".to_string(), timeline_from_pairs(&[(ymd(2026, 9, 1), Some(block))]))];
        let warnings = schedule_flags("Ali Öğretmen", &schedule, &blocks, ymd(2026, 9, 1));
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code, WarningCode::BlockOutsideFreeSlots);
        assert_eq!(warnings[0].from_date, ymd(2026, 11, 5));
    }
}
