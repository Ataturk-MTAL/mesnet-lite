//! Öğretmen kapasite ve program bayraklarının `DecisionContext`'ten türetilmesi
//! (spec §5.3 "Öğretmen tarafı"; R2b brief madde 2). `policy::capacity_flags`
//! ve `policy::schedule_flags` saf fonksiyonlardır ve `Timeline` girdisi
//! ister — bu dosya o girdileri olay günlüğünden bir kez üretir, böylece
//! `capacity_flags`/`schedule_flags` her komutta AYRI AYRI yeniden yazılmaz
//! (spec §5.3, "Kapasite hesabının tek yeri").
//!
//! Ayrıca geçmiş/gelecek tarihli girişlerin `shadowed`/`futureDated`
//! bildirimlerini üretir (spec §5.2; R2b brief madde 4).

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, NaiveDate};

use super::{DecisionContext, ImpactSummary, PlannedEvent};
use crate::domain::history::apply::{apply_coordination, apply_hours, apply_load, apply_schedule};
use crate::domain::history::events::{CoordinationState, EventPayload, HoursState, Stream, StoredEvent, TeacherLoad, WeeklySchedule};
use crate::domain::history::impact::{ImpactLine, ImpactNotice, ImpactWarning, NoticeCode};
use crate::domain::history::policy::{capacity_flags, schedule_flags};
use crate::domain::history::timeline::{Interval, Timeline};
use crate::domain::scheduling::Block;

impl DecisionContext {
    /// Bir öğretmenin belirli bir tarihte koordinatörü olduğu işletmelerin
    /// saat toplamı, ziyaret gününe göre gruplanmış — `validation::check_teacher_totals`
    /// girdisi. Kural yeniden yazılmaz; yalnız `coordination` + `company_hours`
    /// olaylarından türetilir.
    pub(super) fn teacher_hours_by_day_at(&self, teacher_id: i64, at: NaiveDate) -> BTreeMap<i64, i64> {
        let mut by_day: BTreeMap<i64, i64> = BTreeMap::new();
        for &company_id in self.companies.keys() {
            let coordination = self.timeline::<CoordinationState>(Stream::Coordination, company_id, apply_coordination);
            let Some(state) = coordination.state_at(at) else { continue };
            if state.teacher_id != teacher_id {
                continue;
            }
            let hours = self
                .timeline::<HoursState>(Stream::CompanyHours, company_id, apply_hours)
                .state_at(at)
                .map(|h| h.awarded_hours)
                .unwrap_or(0);
            *by_day.entry(state.visit_day).or_insert(0) += hours;
        }
        by_day
    }

    /// Aynı öğretmenin TOPLAM atanmış saatinin zaman çizelgesi —
    /// `capacity_flags` girdisi (`teacher_hours_by_day_at`'in her değişim
    /// tarihindeki toplamı).
    pub(super) fn teacher_assigned_hours_timeline(&self, teacher_id: i64) -> Timeline<i64> {
        let mut intervals: Vec<Interval<i64>> = Vec::new();
        for date in self.coordination_and_hours_change_dates() {
            let total: i64 = self.teacher_hours_by_day_at(teacher_id, date).values().sum();
            let opens_new = intervals.last().is_none_or(|last| last.state != total);
            if opens_new {
                if let Some(last) = intervals.last_mut() {
                    last.valid_to = Some(date);
                }
                intervals.push(Interval { valid_from: date, valid_to: None, state: total, source_event_id: 0 });
            }
        }
        Timeline { intervals }
    }

    /// TÜM işletmelerin koordinasyon ve saat zaman çizelgelerindeki değişim
    /// tarihlerinin birleşimi — öğretmen bazlı yürüyüşlerin ortak adımı.
    fn coordination_and_hours_change_dates(&self) -> BTreeSet<NaiveDate> {
        let mut dates: BTreeSet<NaiveDate> = BTreeSet::new();
        for &company_id in self.companies.keys() {
            let coordination = self.timeline::<CoordinationState>(Stream::Coordination, company_id, apply_coordination);
            insert_interval_dates(&mut dates, &coordination);
            let hours = self.timeline::<HoursState>(Stream::CompanyHours, company_id, apply_hours);
            insert_interval_dates(&mut dates, &hours);
        }
        dates
    }

    /// Bir öğretmenin TÜM koordinatörlük bloklarının (işletme adı, zaman
    /// çizelgesi) listesi — yalnız ZORLANMAMIŞ atamalar (spec §5.3,
    /// "schedule_flags ... yalnız ZORLANMAMIŞ atamaları içermelidir").
    pub(super) fn teacher_blocks(&self, teacher_id: i64) -> Vec<(String, Timeline<Block>)> {
        self.companies
            .iter()
            .filter_map(|(&company_id, facts)| {
                self.teacher_block_for_company(teacher_id, company_id).map(|tl| (facts.name.clone(), tl))
            })
            .collect()
    }

    fn teacher_block_for_company(&self, teacher_id: i64, company_id: i64) -> Option<Timeline<Block>> {
        let coordination = self.timeline::<CoordinationState>(Stream::Coordination, company_id, apply_coordination);
        let hours = self.timeline::<HoursState>(Stream::CompanyHours, company_id, apply_hours);
        let mut dates: BTreeSet<NaiveDate> = BTreeSet::new();
        insert_interval_dates(&mut dates, &coordination);
        insert_interval_dates(&mut dates, &hours);

        let mut intervals: Vec<Interval<Block>> = Vec::new();
        for date in dates {
            let block = coordination.state_at(date).filter(|s| s.teacher_id == teacher_id && !s.is_forced).map(|s| {
                let awarded = hours.state_at(date).map(|h| h.awarded_hours).unwrap_or(0);
                Block::from_start(s.visit_day, s.visit_hour, awarded)
            });
            push_block_interval(&mut intervals, date, block);
        }
        if intervals.is_empty() {
            None
        } else {
            Some(Timeline { intervals })
        }
    }
}

fn insert_interval_dates<S>(dates: &mut BTreeSet<NaiveDate>, timeline: &Timeline<S>) {
    for iv in &timeline.intervals {
        dates.insert(iv.valid_from);
        if let Some(valid_to) = iv.valid_to {
            dates.insert(valid_to);
        }
    }
}

fn push_block_interval(intervals: &mut Vec<Interval<Block>>, date: NaiveDate, block: Option<Block>) {
    match (intervals.last_mut(), block) {
        (Some(last), Some(b)) if last.state == b => {}
        (Some(last), None) if last.valid_to.is_none() => last.valid_to = Some(date),
        (_, Some(b)) => {
            if let Some(last) = intervals.last_mut() {
                last.valid_to = Some(date);
            }
            intervals.push(Interval { valid_from: date, valid_to: None, state: b, source_event_id: 0 });
        }
        _ => {}
    }
}

/// `policy::capacity_flags` çağrısını `DecisionContext`'ten türetilen zaman
/// çizelgeleriyle sarar (spec §5.3, "Kapasite hesabının tek yeri" — R2b
/// brief madde 2). `ctx`, aday olayları da görmelidir; çağıran taraf
/// gerekirse `with_pending` ile sarılmış bir bağlam geçirmelidir.
pub(super) fn teacher_capacity_warnings(ctx: &DecisionContext, teacher_id: i64, from: NaiveDate) -> Vec<ImpactWarning> {
    let label = ctx.teacher_label(teacher_id);
    let load = ctx.timeline::<TeacherLoad>(Stream::TeacherLoad, teacher_id, apply_load);
    let assigned = ctx.teacher_assigned_hours_timeline(teacher_id);
    capacity_flags(&label, teacher_id, &load, &assigned, &|at| ctx.teacher_hours_by_day_at(teacher_id, at), ctx.statutory_cap, from)
}

/// `policy::schedule_flags` çağrısını `DecisionContext`'ten türetilen zaman
/// çizelgeleriyle sarar (spec §5.3, R2b brief madde 2).
pub(super) fn teacher_schedule_warnings(ctx: &DecisionContext, teacher_id: i64, from: NaiveDate) -> Vec<ImpactWarning> {
    let label = ctx.teacher_label(teacher_id);
    let schedule = ctx.timeline::<WeeklySchedule>(Stream::TeacherSchedule, teacher_id, apply_schedule);
    let blocks = ctx.teacher_blocks(teacher_id);
    schedule_flags(&label, &schedule, &blocks, from)
}

/// Anlık görüntü akışlarında (saat, yük, program) bir değişikliğin öznenin
/// SONRAKİ kaydına kadar mı geçerli olduğunu ve yürürlük tarihinin gelecekte
/// olup olmadığını bildirir (spec §5.2 "shadowed"; R2b brief madde 4).
///
/// Kanıtlanmış teşhis (kullanıcı Hakan GÜLEN): 14 Eylül'den geçerli bir
/// değişiklik girildi, "Kaydedildi" görüldü, ama ekranda hiçbir şey
/// değişmedi — çünkü AYNI AYIN 20'sinde girilmiş DAHA ESKİ bir kayıt hâlâ
/// duruyordu ve eski davranış bunu yalnız `Shadowed` ile BİLDİRİYORDU. Ek
/// ders puantajı AY SONU durumuna göre yapılır (önceki ay zaten
/// `PreviousMonthClosed` ile kapalıdır — bkz. `terms::TermDates::earliest_allowed`);
/// dolayısıyla `d` ile AYNI TAKVİM AYINDA (yıl+ay) yürürlüğe giren, henüz
/// geri alınmamış her kayıt bu ay için ARTIK ANLAMSIZDIR ve OTOMATİK geri
/// alınır — kullanıcının ayrıca bir şey işaretlemesi GEREKMEZ. Yalnız
/// SONRAKİ AYLARDAKİ kayıtlar hâlâ `Shadowed` ile bildirilir (bir sonraki
/// puantaj dönemi henüz kapanmadığı için o kayıt anlamlı kalabilir); birden
/// çok özne için çağrılırsa `impact.shadowed_until` EN ERKEN kalan
/// gölgeleme tarihine indirilir.
pub(super) fn apply_shadow_and_future_notices(
    ctx: &DecisionContext,
    stream: Stream,
    subject_id: i64,
    label: &str,
    d: NaiveDate,
    events: &mut Vec<PlannedEvent>,
    impact: &mut ImpactSummary,
) {
    if d > ctx.today {
        impact.notices.push(ImpactNotice {
            code: NoticeCode::FutureDated,
            message: format!("{label}: yürürlük tarihi ({d}) gelecekte; değişiklik henüz etkili değil"),
            subject_label: label.to_string(),
            date: d,
        });
    }

    let (same_month, other_months) = split_later_live_records_by_month(ctx, stream, subject_id, d);
    revoke_records(stream, subject_id, label, d, &same_month, events, impact);

    let Some(next_date) = other_months.iter().map(|e| e.effective_date).min() else { return };
    impact.notices.push(ImpactNotice {
        code: NoticeCode::Shadowed,
        message: format!("{label}: bu değişiklik {next_date} tarihli kayda kadar geçerli"),
        subject_label: label.to_string(),
        date: d,
    });
    impact.shadowed_until = Some(match impact.shadowed_until {
        Some(existing) if existing <= next_date => existing,
        _ => next_date,
    });
}

/// Her geri alınan kayıt için `Revoked` işareti (`decide::revoke::revoke_family_events`
/// ile AYNI desen: aynı `change_set` içinde) ve `impact.primary`de
/// `kind: revoked` satırı üretir — satırın tarihi geri alınan kaydın KENDİ
/// tarihidir, `d` değil (kullanıcı hangi kaydın kalktığını görebilsin diye).
fn revoke_records(stream: Stream, subject_id: i64, label: &str, d: NaiveDate, records: &[StoredEvent], events: &mut Vec<PlannedEvent>, impact: &mut ImpactSummary) {
    for record in records {
        events.push(PlannedEvent {
            stream,
            subject_id,
            effective_date: d,
            payload: EventPayload::Revoked,
            caused_by: None,
            revokes: Some(record.id),
        });
        impact.primary.push(ImpactLine {
            kind: "revoked".to_string(),
            stream: stream.as_str().to_string(),
            subject_id,
            subject_label: label.to_string(),
            effective_date: record.effective_date,
            before: Some(record.payload.kind().to_string()),
            after: None,
        });
    }
}

/// `after`'DAN SONRA yürürlüğe giren, HENÜZ geri alınmamış canlı kayıtları
/// `after` ile AYNI takvim ayında olanlar / olmayanlar diye ikiye ayırır.
/// `order_events`in filtresiyle AYNI iki eleme uygulanır (DRY: markörler VE
/// başka bir markörün hedefi olan kayıtlar dışlanır) — aksi hâlde zaten geri
/// alınmış bir kayıt burada ikinci kez "canlı" sayılırdı.
fn split_later_live_records_by_month(ctx: &DecisionContext, stream: Stream, subject_id: i64, after: NaiveDate) -> (Vec<StoredEvent>, Vec<StoredEvent>) {
    let raw = ctx.events_for(stream, subject_id);
    let revoked_target_ids: BTreeSet<i64> = raw.iter().filter_map(|e| e.revokes).collect();
    raw.into_iter()
        .filter(|e| !matches!(e.payload, EventPayload::Revoked))
        .filter(|e| !revoked_target_ids.contains(&e.id))
        .filter(|e| e.effective_date > after)
        .partition(|e| same_calendar_month(e.effective_date, after))
}

/// İki tarih aynı takvim ayında mı (yıl VE ay eşit)?
fn same_calendar_month(a: NaiveDate, b: NaiveDate) -> bool {
    a.year() == b.year() && a.month() == b.month()
}
