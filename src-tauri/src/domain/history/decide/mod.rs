//! Karar verme — esrs deseninin `decide(ctx, cmd) -> Result<Decision, Rejection>`
//! karşılığı (spec §3). Saf fonksiyondur: DB'ye dokunmaz, `DecisionContext`
//! önceden yüklenmiş olarak gelir (bunu R3'te `db/history_context.rs` yapar).
//!
//! Her komut türü kendi dosyasında yaşar (`student.rs`, `company.rs`,
//! `teacher.rs`, `revoke.rs`); bu dosya yalnız ortak türleri, `DecisionContext`
//! yardımcılarını ve dağıtımı (`decide()`) barındırır.

mod chief;
mod company;
mod flags;
mod revoke;
mod student;
mod teacher;
#[cfg(test)]
mod test_support;

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::Deserialize;

use super::events::{EventPayload, Labels, Stream, StoredEvent, TeacherLoad, WeeklySchedule};
use super::rejection::{Rejection, RejectionCode};
use super::timeline::{fold_intervals, order_events, Interval, Timeline};
use crate::domain::hour_rules::HourRule;
use crate::domain::models::NewCompany;
use crate::domain::scheduling::Slot;
use crate::domain::terms::TermDates;

pub use super::impact::ImpactSummary;

/// `preview_change`/`commit_change` isteğinin gövdesi (spec §8).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRequest {
    pub term: String,
    pub effective_date: Option<NaiveDate>,
    pub document_date: Option<NaiveDate>,
    pub reason: String,
    pub command: ChangeCommand,
}

/// `createStudent.student` — `companyId` ve `term` bu gövdede YOKTUR; zarftaki
/// `companyId` ve istekteki `term` kullanılır (brief, spec §8 "İç gövdeler").
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewStudentInput {
    pub first_name: String,
    pub last_name: String,
    pub student_no: Option<String>,
    pub grade: String,
    pub branch: String,
    pub submitted_at: Option<String>,
}

/// `createTeacher.teacher` (spec §8 "İç gövdeler").
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTeacherProfile {
    pub first_name: String,
    pub last_name: String,
    pub registry_no: String,
    pub field: String,
    pub branches: Vec<String>,
    pub is_active: bool,
}

/// `setCompanyHours.rows[]` öğesi.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanyHoursRow {
    pub company_id: i64,
    pub awarded_hours: i64,
    pub is_honorary: bool,
    pub is_locked: bool,
    pub notes: String,
}

/// `assignCoordinators.rows[]` öğesi.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatorRow {
    pub company_id: i64,
    pub teacher_id: i64,
    pub visit_day: i64,
    pub visit_hour: i64,
    pub is_forced: bool,
    pub force_reason: Option<String>,
}

/// `transferStudent.to` (spec §8): mevcut bir işletmeye ya da yerinde
/// oluşturulan yeni bir işletmeye nakil.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum TransferTarget {
    Existing { company_id: i64 },
    New { company: NewCompany },
}

/// Arayüzün gönderdiği 16 komut türü (spec §8, birebir alan adlarıyla).
/// `correct.replacement` herhangi bir `ChangeCommand` olabilir; ama içinde
/// `revoke` ya da `correct` varsa `decide::revoke::correct` bunu
/// `NotRevocable` ile reddeder.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ChangeCommand {
    CreateStudent { student: NewStudentInput, company_id: Option<i64> },
    // `to` `transferStudent`in `TransferTarget`iyle AYNI şekli kullanır:
    // var olan bir işletme (`Existing`) ya da yerinde oluşturulmuş yeni bir
    // işletme (`New`) — teşhis: eskiden yalnız var olan işletme kabul
    // edilirdi, bu da yeni bir işletmeye ilk yerleştirmeyi kapıdan yapmayı
    // imkansız kılıyordu.
    PlaceStudent { student_id: i64, to: TransferTarget },
    TransferStudent { student_id: i64, from_company_id: i64, to: TransferTarget },
    StudentLeaves { student_id: i64, from_company_id: i64 },
    DeleteStudent { student_id: i64 },
    SetCompanyHours { rows: Vec<CompanyHoursRow> },
    AssignCoordinators { rows: Vec<CoordinatorRow> },
    EndCoordination { company_id: i64 },
    ClearCoordination,
    CreateTeacher { teacher: NewTeacherProfile, load: TeacherLoad },
    SetTeacherLoad { teacher_id: i64, load: TeacherLoad },
    SetTeacherSchedule { teacher_id: i64, slots: Vec<Slot> },
    CopySchedulesFromTerm { from_term: String },
    DeleteTeacher { teacher_id: i64 },
    Revoke { change_set_id: i64 },
    Correct { change_set_id: i64, replacement: Box<ChangeCommand> },
}

/// Bir değişiklik kümesinin denetimde/politikada ihtiyaç duyulan olguları.
#[derive(Debug, Clone)]
pub struct ChangeSetFacts {
    pub id: i64,
    pub kind: String,
    pub effective_date: NaiveDate,
    pub revokes_change_set_id: Option<i64>,
    pub revoked_by: Option<i64>,
}

/// Bir işletmenin karar sırasında gereken olguları.
#[derive(Debug, Clone)]
pub struct CompanyFacts {
    pub name: String,
    pub round_trip_km: Option<f64>,
    pub is_active: bool,
}

/// Yerinde (aynı transaction'da, `decide`'dan ÖNCE) oluşturulmuş satırların
/// id'leri. Anahtar komutun neresinde kullanıldığıdır (ör. `transferStudent`
/// içindeki `to: {type:'new', ...}`).
#[derive(Debug, Clone, Default)]
pub struct Materialized {
    pub company_id: Option<i64>,
    pub student_id: Option<i64>,
    pub teacher_id: Option<i64>,
}

/// `decide`'ın saf olarak çalışması için gereken TÜM bağlam. `db/history_context.rs`
/// (R3) bunu tek bir transaction içinde, dönemin tüm olaylarını okuyarak kurar.
#[derive(Debug, Clone)]
pub struct DecisionContext {
    pub today: NaiveDate,
    pub term: TermDates,
    pub rules: Vec<HourRule>,
    pub statutory_cap: i64,
    pub day_end_hour: i64,
    /// Alan koordinatörlüğü ders yükü havuzu (OÖKY MADDE 88/2-ç, Norm Kadro
    /// Yön. MADDE 6/4): şeflik saatleri + Σ (haftalık ders saati × grup
    /// sayısı). TEK hesap noktası `db::teaching_load::total_pool_hours_in`'dir
    /// (bkz. `db/history_context.rs::load`); burada kopyalanmaz. `0` ise
    /// havuz bu dönem için henüz tanımlanmamıştır — aşım denetimi bu durumda
    /// YAPILMAZ (bkz. `domain::hour_distribution::pool_overrun_reason`).
    pub pool_hours: i64,
    pub companies: BTreeMap<i64, CompanyFacts>,
    pub student_names: BTreeMap<i64, String>,
    pub teacher_names: BTreeMap<i64, String>,
    pub events: Vec<StoredEvent>,
    pub change_sets: BTreeMap<i64, ChangeSetFacts>,
    pub high_water: i64,
    pub materialized: Materialized,
    /// Kaynak dönemdeki her öğretmenin SON geçerli programı — yalnız
    /// `copySchedulesFromTerm` için doldurulur (R3'te `db::history_context::load`
    /// bunu okur); diğer komutlarda boş kalır (R2b brief madde 1).
    pub source_schedules: BTreeMap<i64, WeeklySchedule>,
}

/// `decide`'ın planladığı, henüz kaydedilmemiş bir olay.
#[derive(Debug, Clone)]
pub struct PlannedEvent {
    pub stream: Stream,
    pub subject_id: i64,
    pub effective_date: NaiveDate,
    pub payload: EventPayload,
    /// Aynı karardaki BİRİNCİL olayın `Decision.events` içindeki indeksi.
    /// `db::change_log::append_events` bunu gerçek `change_events.id`'ye çevirir.
    pub caused_by: Option<usize>,
    /// Bu olay bir başkasını geri alıyorsa hedefin gerçek (zaten kaydedilmiş)
    /// `change_events.id`'si.
    pub revokes: Option<i64>,
}

/// `change_sets` tablosuna yazılacak yeni satır.
#[derive(Debug, Clone)]
pub struct NewChangeSet {
    pub term: String,
    pub kind: String,
    pub effective_date: NaiveDate,
    pub document_date: Option<NaiveDate>,
    pub reason: String,
    pub revokes_change_set_id: Option<i64>,
}

/// `decide`'ın DB satırları üzerinde istediği yan etki (silme/pasifleştirme).
/// Olay günlüğü dışındaki TEK yazma kanalıdır; `change_service` (R4) bunu
/// olaylarla aynı transaction'da uygular.
#[derive(Debug, Clone, PartialEq)]
pub enum RowAction {
    DeleteStudent(i64),
    DeleteTeacher(i64),
    DeactivateCompany(i64),
}

/// Bir olayın ait olduğu projeksiyon anahtarı — rebuild bunun üzerinden özne
/// başına BİR kez çalışır (spec §5 adım 6).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct StreamKey {
    pub stream: Stream,
    pub subject_id: i64,
    pub term: String,
}

/// `decide`'ın çıktısı: kalıcılaştırılacak her şey artı önizlemede gösterilecek
/// özet.
#[derive(Debug, Clone)]
pub struct Decision {
    pub change_set: NewChangeSet,
    pub events: Vec<PlannedEvent>,
    pub impact: ImpactSummary,
    pub row_actions: Vec<RowAction>,
    pub touched: Vec<StreamKey>,
}

/// Zincir etki henüz KAYDEDİLMEMİŞ birincil olay(lar)ı da görmelidir — aksi
/// hâlde `ctx.events` yalnız GEÇMİŞİ yansıttığı için ör. bir nakil kendi
/// hedef işletmesindeki yeni öğrenci sayısını göremez. Bu, `ctx`'in geçici
/// bir KOPYASINA aday olayları (sahte, çakışmayan kimliklerle) ekler; asıl
/// `ctx` DEĞİŞMEZ.
const PENDING_EVENT_ID_BASE: i64 = 1_000_000_000_000;
const PENDING_CHANGE_SET_ID: i64 = -1;

pub(super) fn with_pending(ctx: &DecisionContext, pending: &[PlannedEvent]) -> DecisionContext {
    let mut augmented = ctx.clone();
    for (index, planned) in pending.iter().enumerate() {
        augmented.events.push(StoredEvent {
            id: PENDING_EVENT_ID_BASE + index as i64,
            change_set_id: PENDING_CHANGE_SET_ID,
            stream: planned.stream,
            subject_id: planned.subject_id,
            term: ctx.term.term.clone(),
            effective_date: planned.effective_date,
            payload: planned.payload.clone(),
            caused_by: None,
            revokes: planned.revokes,
            is_opening: false,
        });
    }
    augmented
}

/// Ortak komut dağıtımı. Her kol kendi dosyasına yönlenir. Tek istisna,
/// komut türünden bağımsız NET sonuç kuralıdır (alan şefi tekliği, `chief.rs`):
/// `revoke`/`correct` da şefliği doğurabildiği için kararın tamamına bakılır.
pub fn decide(ctx: &DecisionContext, req: &ChangeRequest) -> Result<Decision, Rejection> {
    let decision = dispatch(ctx, req)?;
    chief::enforce_single_department(ctx, &decision)?;
    Ok(decision)
}

fn dispatch(ctx: &DecisionContext, req: &ChangeRequest) -> Result<Decision, Rejection> {
    match &req.command {
        ChangeCommand::CreateStudent { student, company_id } => student::create_student(ctx, req, student, *company_id),
        ChangeCommand::PlaceStudent { student_id, to } => student::place_student(ctx, req, *student_id, to),
        ChangeCommand::TransferStudent { student_id, from_company_id, to } => {
            student::transfer_student(ctx, req, *student_id, *from_company_id, to)
        }
        ChangeCommand::StudentLeaves { student_id, from_company_id } => student::student_leaves(ctx, req, *student_id, *from_company_id),
        ChangeCommand::DeleteStudent { student_id } => student::delete_student(ctx, req, *student_id),
        ChangeCommand::SetCompanyHours { rows } => company::set_company_hours(ctx, req, rows),
        ChangeCommand::AssignCoordinators { rows } => company::assign_coordinators(ctx, req, rows),
        ChangeCommand::EndCoordination { company_id } => company::end_coordination(ctx, req, *company_id),
        ChangeCommand::ClearCoordination => company::clear_coordination(ctx, req),
        ChangeCommand::CreateTeacher { teacher, load } => teacher::create_teacher(ctx, req, teacher, load),
        ChangeCommand::SetTeacherLoad { teacher_id, load } => teacher::set_teacher_load(ctx, req, *teacher_id, load),
        ChangeCommand::SetTeacherSchedule { teacher_id, slots } => teacher::set_teacher_schedule(ctx, req, *teacher_id, slots),
        ChangeCommand::CopySchedulesFromTerm { from_term } => teacher::copy_schedules_from_term(ctx, req, from_term),
        ChangeCommand::DeleteTeacher { teacher_id } => teacher::delete_teacher(ctx, req, *teacher_id),
        ChangeCommand::Revoke { change_set_id } => revoke::revoke(ctx, req, *change_set_id),
        ChangeCommand::Correct { change_set_id, replacement } => revoke::correct(ctx, req, *change_set_id, replacement),
    }
}

/// Komutun `change_sets.kind` sütununa yazılacak adı. `"opening"`, `"revoke"`
/// ve `"correct"` ayrıca karar kurallarında (ör. `NotRevocable`) özel anlam
/// taşır; diğerleri yalnız görüntüleme/denetim amaçlıdır.
pub(super) fn command_kind(command: &ChangeCommand) -> &'static str {
    match command {
        ChangeCommand::CreateStudent { .. } => "create_student",
        ChangeCommand::PlaceStudent { .. } => "place_student",
        ChangeCommand::TransferStudent { .. } => "transfer_student",
        ChangeCommand::StudentLeaves { .. } => "student_leaves",
        ChangeCommand::DeleteStudent { .. } => "delete_student",
        ChangeCommand::SetCompanyHours { .. } => "set_company_hours",
        ChangeCommand::AssignCoordinators { .. } => "assign_coordinators",
        ChangeCommand::EndCoordination { .. } => "end_coordination",
        ChangeCommand::ClearCoordination => "clear_coordination",
        ChangeCommand::CreateTeacher { .. } => "create_teacher",
        ChangeCommand::SetTeacherLoad { .. } => "set_teacher_load",
        ChangeCommand::SetTeacherSchedule { .. } => "set_teacher_schedule",
        ChangeCommand::CopySchedulesFromTerm { .. } => "copy_schedules_from_term",
        ChangeCommand::DeleteTeacher { .. } => "delete_teacher",
        ChangeCommand::Revoke { .. } => "revoke",
        ChangeCommand::Correct { .. } => "correct",
    }
}

pub(super) fn labels(pairs: &[(&str, &str)]) -> Labels {
    Labels(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
}

/// Bir `PlannedEvent`i etki özetinin bir satırına çevirir. `before`/`after`
/// hazır metin olarak verilir; çağıran taraf uygun `Debug`/etiket biçimini
/// seçer (bkz. `debug_opt`).
pub(super) fn impact_line(
    subject_label: &str,
    event: &PlannedEvent,
    before: Option<String>,
    after: Option<String>,
) -> super::impact::ImpactLine {
    super::impact::ImpactLine {
        kind: event.payload.kind().to_string(),
        stream: event.stream.as_str().to_string(),
        subject_id: event.subject_id,
        subject_label: subject_label.to_string(),
        effective_date: event.effective_date,
        before,
        after,
    }
}

/// Etki satırlarında ve denetimde kullanılan basit metin gösterimi. Süslü bir
/// Türkçe betimleme değildir — R4/R5'te arayüz katmanı isterse zenginleştirir.
pub(super) fn debug_opt<S: std::fmt::Debug>(value: Option<&S>) -> Option<String> {
    value.map(|v| format!("{v:?}"))
}

/// Bir kararın olaylarındaki her `(stream, subject, term)` anahtarını TEKİL
/// olarak toplar (spec §5 adım 6, "touched").
pub(super) fn touched_from(events: &[PlannedEvent], term: &str) -> Vec<StreamKey> {
    let mut keys: Vec<StreamKey> = Vec::new();
    for e in events {
        let key = StreamKey { stream: e.stream, subject_id: e.subject_id, term: term.to_string() };
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

impl DecisionContext {
    /// Bir akış+özne çiftinin TÜM ham olayları, günlükteki sırayla.
    pub(super) fn events_for(&self, stream: Stream, subject_id: i64) -> Vec<StoredEvent> {
        self.events.iter().filter(|e| e.stream == stream && e.subject_id == subject_id).cloned().collect()
    }

    /// Bir akış+öznenin katlanmış zaman çizelgesi (geri alınanlar hariç).
    pub(super) fn timeline<S: Clone + PartialEq>(
        &self,
        stream: Stream,
        subject_id: i64,
        apply: impl Fn(Option<&S>, &EventPayload) -> Option<S>,
    ) -> Timeline<S> {
        let filtered = self.events_for(stream, subject_id);
        let ordered = order_events(&filtered, self.term.start);
        Timeline { intervals: fold_intervals(&ordered, apply) }
    }

    /// `target` tarihinin HEMEN ÖNCESİNDEKİ (d⁻) durum — o tarihte kaydedilmiş
    /// bir olay varsa bile onu SAYMAZ (spec §5.2, "Geçmiş tarihli giriş").
    pub(super) fn state_before<S: Clone + PartialEq>(
        &self,
        stream: Stream,
        subject_id: i64,
        target: NaiveDate,
        apply: impl Fn(Option<&S>, &EventPayload) -> Option<S>,
    ) -> Option<S> {
        let filtered = self.events_for(stream, subject_id);
        let ordered = order_events(&filtered, self.term.start);
        let mut state: Option<S> = None;
        for te in &ordered {
            if te.effective_date >= target {
                break;
            }
            state = apply(state.as_ref(), &te.payload);
        }
        state
    }

    pub(super) fn company_label(&self, id: i64) -> String {
        self.companies.get(&id).map(|c| c.name.clone()).unwrap_or_else(|| format!("İşletme #{id}"))
    }

    pub(super) fn student_label(&self, id: i64) -> String {
        self.student_names.get(&id).cloned().unwrap_or_else(|| format!("Öğrenci #{id}"))
    }

    pub(super) fn teacher_label(&self, id: i64) -> String {
        self.teacher_names.get(&id).cloned().unwrap_or_else(|| format!("Öğretmen #{id}"))
    }

    /// Sınırda doğrulama (R2b brief madde 3): komuttaki bir öğrenci id'si
    /// bağlamda yoksa `InvalidRequest`. Önceden bu denetim YOKTU; eksik id
    /// sessizce `student_label`'ın yer tutucu etiketine düşüyordu.
    pub(super) fn require_student(&self, id: i64) -> Result<(), Rejection> {
        if self.student_names.contains_key(&id) {
            Ok(())
        } else {
            Err(Rejection::new(RejectionCode::InvalidRequest, format!("#{id} numaralı öğrenci bulunamadı.")))
        }
    }

    pub(super) fn require_teacher(&self, id: i64) -> Result<(), Rejection> {
        if self.teacher_names.contains_key(&id) {
            Ok(())
        } else {
            Err(Rejection::new(RejectionCode::InvalidRequest, format!("#{id} numaralı öğretmen bulunamadı.")))
        }
    }

    /// Yalnız VARLIK denetimi yapar; aktiflik `require_active_company`'nindir.
    pub(super) fn require_company(&self, id: i64) -> Result<&CompanyFacts, Rejection> {
        self.companies
            .get(&id)
            .ok_or_else(|| Rejection::new(RejectionCode::InvalidRequest, format!("#{id} numaralı işletme bulunamadı.")))
    }

    /// `require_company` + `is_active` denetimi. Yalnız YENİ öğrenci ataması
    /// ve nakil HEDEFİNDE kullanılır (spec §5.4, R2b brief madde 3): pasif
    /// bir işletme yeni bir öğrenci kabul edemez.
    pub(super) fn require_active_company(&self, id: i64) -> Result<&CompanyFacts, Rejection> {
        let facts = self.require_company(id)?;
        if facts.is_active {
            Ok(facts)
        } else {
            Err(Rejection::new(
                RejectionCode::InvalidRequest,
                format!("{}: pasif işletme yeni atama ya da nakil hedefi olamaz.", facts.name),
            ))
        }
    }

    /// `decide`'dan ÖNCE, aynı transaction'da yerinde oluşturulmuş bir
    /// satırın id'si `None` gelirse bu, oluşturma adımının başarısız olduğu
    /// ya da hiç çalıştırılmadığı anlamına gelir — önceden `unwrap_or_default()`
    /// ile sessizce `0` id'sine düşüyordu (R2b brief madde 3).
    pub(super) fn require_materialized_student(&self) -> Result<i64, Rejection> {
        self.materialized
            .student_id
            .ok_or_else(|| Rejection::new(RejectionCode::InvalidRequest, "Öğrenci yerinde oluşturulamadı.".to_string()))
    }

    pub(super) fn require_materialized_teacher(&self) -> Result<i64, Rejection> {
        self.materialized
            .teacher_id
            .ok_or_else(|| Rejection::new(RejectionCode::InvalidRequest, "Öğretmen yerinde oluşturulamadı.".to_string()))
    }

    pub(super) fn require_materialized_company(&self) -> Result<i64, Rejection> {
        self.materialized
            .company_id
            .ok_or_else(|| Rejection::new(RejectionCode::InvalidRequest, "Yeni işletme oluşturulamadı.".to_string()))
    }

    /// `revoke`/`correct` bilinmeyen bir `changeSetId` alırsa `NotRevocable`
    /// DEĞİL `InvalidRequest` döner (R2b brief madde 3): hedef hiç yok, bu
    /// bir iş kuralı reddi değil, geçersiz bir isteğin belirtisidir.
    pub(super) fn require_change_set(&self, id: i64) -> Result<ChangeSetFacts, Rejection> {
        self.change_sets
            .get(&id)
            .cloned()
            .ok_or_else(|| Rejection::new(RejectionCode::InvalidRequest, format!("#{id} numaralı kayıt bulunamadı.")))
    }

    /// Bir işletmedeki öğrenci sayısının zaman içindeki seyri. Tek bir olay
    /// akışından değil, TÜM öğrencilerin yerleştirme zaman çizelgelerinin
    /// birleşiminden türetilir (`placement` akışı özne başınadır, işletme
    /// başına değil).
    pub(super) fn company_student_count_timeline(&self, company_id: i64) -> Timeline<i64> {
        use crate::domain::history::apply::apply_placement;
        use std::collections::BTreeSet;

        let student_ids: BTreeSet<i64> =
            self.events.iter().filter(|e| e.stream == Stream::Placement).map(|e| e.subject_id).collect();
        let placement_timelines: Vec<Timeline<i64>> =
            student_ids.iter().map(|sid| self.timeline::<i64>(Stream::Placement, *sid, apply_placement)).collect();

        let mut change_points: BTreeSet<NaiveDate> = BTreeSet::new();
        for tl in &placement_timelines {
            for iv in &tl.intervals {
                change_points.insert(iv.valid_from);
                if let Some(valid_to) = iv.valid_to {
                    change_points.insert(valid_to);
                }
            }
        }

        let mut intervals: Vec<Interval<i64>> = Vec::new();
        for date in change_points {
            let count = placement_timelines.iter().filter(|tl| tl.state_at(date) == Some(&company_id)).count() as i64;
            let opens_new = intervals.last().is_none_or(|last| last.state != count);
            if opens_new {
                if let Some(last) = intervals.last_mut() {
                    last.valid_to = Some(date);
                }
                intervals.push(Interval { valid_from: date, valid_to: None, state: count, source_event_id: 0 });
            }
        }
        Timeline { intervals }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_json(kind: &str) -> serde_json::Value {
        match kind {
            "createStudent" => serde_json::json!({
                "type": "createStudent",
                "student": {"firstName": "Ahmet", "lastName": "Yılmaz", "studentNo": "123", "grade": "12", "branch": "A", "submittedAt": null},
                "companyId": null,
            }),
            "placeStudent" => serde_json::json!({
                "type": "placeStudent", "studentId": 1,
                "to": {"type": "existing", "companyId": 2},
            }),
            "placeStudentNew" => serde_json::json!({
                "type": "placeStudent", "studentId": 1,
                "to": {"type": "new", "company": {
                    "name": "Yeni İşletme", "contactFirstName": "", "contactLastName": "", "phone": "",
                    "email": "", "addressText": "", "latitude": null, "longitude": null,
                    "oneWayDistanceKm": null, "notes": "",
                }},
            }),
            "transferStudent" => serde_json::json!({
                "type": "transferStudent", "studentId": 1, "fromCompanyId": 2,
                "to": {"type": "existing", "companyId": 3},
            }),
            "transferStudentNew" => serde_json::json!({
                "type": "transferStudent", "studentId": 1, "fromCompanyId": 2,
                "to": {"type": "new", "company": {
                    "name": "Yeni İşletme", "contactFirstName": "", "contactLastName": "", "phone": "",
                    "email": "", "addressText": "", "latitude": null, "longitude": null,
                    "oneWayDistanceKm": null, "notes": "",
                }},
            }),
            "studentLeaves" => serde_json::json!({"type": "studentLeaves", "studentId": 1, "fromCompanyId": 2}),
            "deleteStudent" => serde_json::json!({"type": "deleteStudent", "studentId": 1}),
            "setCompanyHours" => serde_json::json!({"type": "setCompanyHours", "rows": [
                {"companyId": 1, "awardedHours": 4, "isHonorary": false, "isLocked": false, "notes": ""},
            ]}),
            "assignCoordinators" => serde_json::json!({"type": "assignCoordinators", "rows": [
                {"companyId": 1, "teacherId": 2, "visitDay": 1, "visitHour": 3, "isForced": false, "forceReason": null},
            ]}),
            "endCoordination" => serde_json::json!({"type": "endCoordination", "companyId": 1}),
            "clearCoordination" => serde_json::json!({"type": "clearCoordination"}),
            "createTeacher" => serde_json::json!({
                "type": "createTeacher",
                "teacher": {"firstName": "Ali", "lastName": "Veli", "registryNo": "999", "field": "Elektrik", "branches": ["Elektronik"], "isActive": true},
                "load": {"baseHours": 15, "maxExtraHours": 24, "otherExtraHours": 0, "chiefType": "none", "employmentType": "tenured"},
            }),
            "setTeacherLoad" => serde_json::json!({
                "type": "setTeacherLoad", "teacherId": 1,
                "load": {"baseHours": 15, "maxExtraHours": 24, "otherExtraHours": 0, "chiefType": "none", "employmentType": "tenured"},
            }),
            "setTeacherSchedule" => serde_json::json!({"type": "setTeacherSchedule", "teacherId": 1, "slots": [{"dayOfWeek": 1, "hour": 3}]}),
            "copySchedulesFromTerm" => serde_json::json!({"type": "copySchedulesFromTerm", "fromTerm": "2025-2026/1"}),
            "deleteTeacher" => serde_json::json!({"type": "deleteTeacher", "teacherId": 1}),
            "revoke" => serde_json::json!({"type": "revoke", "changeSetId": 5}),
            "correct" => serde_json::json!({
                "type": "correct", "changeSetId": 5,
                "replacement": {"type": "endCoordination", "companyId": 1},
            }),
            other => panic!("bilinmeyen örnek: {other}"),
        }
    }

    /// V1'in gönderdiği JSON örneklerinin HER tür için deserialize edildiğini
    /// doğrular (brief: "Bir test, V1'in gönderdiği JSON örneklerinin her tür
    /// için deserialize edildiğini doğrulamalı").
    #[test]
    fn every_change_command_wire_shape_deserializes() {
        let kinds = [
            "createStudent", "placeStudent", "placeStudentNew", "transferStudent", "transferStudentNew", "studentLeaves",
            "deleteStudent", "setCompanyHours", "assignCoordinators", "endCoordination", "clearCoordination",
            "createTeacher", "setTeacherLoad", "setTeacherSchedule", "copySchedulesFromTerm", "deleteTeacher",
            "revoke", "correct",
        ];
        for kind in kinds {
            let json = sample_json(kind);
            let result: Result<ChangeCommand, _> = serde_json::from_value(json.clone());
            assert!(result.is_ok(), "{kind} deserialize edilemedi: {:?} — girdi: {json}", result.err());
        }
    }

    #[test]
    fn change_request_envelope_deserializes_with_camel_case_dates() {
        let json = serde_json::json!({
            "term": "2026-2027/1",
            "effectiveDate": "2026-11-03",
            "documentDate": null,
            "reason": "Nakil",
            "command": sample_json("transferStudent"),
        });
        let req: ChangeRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.effective_date, Some(NaiveDate::from_ymd_opt(2026, 11, 3).unwrap()));
    }

    /// Kasım senaryosu (plan, Görev R2, "Kasım senaryosu testi"):
    /// 1. Dönem 15 Eylül'de başlar; S öğrencisi A'da, A'da 3 öğrenci, tavan 6,
    ///    takdir 6, koordinatör T1 Pazartesi 3. saat.
    /// 2. 5 Kasım: T1'in okuldaki ek dersi (`other_extra_hours`) 0 → 6.
    /// 3. 10 Kasım (today) S, yeni işletme C'ye nakledilir; yürürlük 3 Kasım.
    /// 4. 12 Kasım (today) düzeltme: nakil aslında MEVCUT işletme B'ye.
    #[test]
    fn november_scenario_corrects_a_transfer_and_keeps_october_state() {
        use super::test_support::*;
        use crate::domain::history::apply::apply_hours;
        use crate::domain::models::{ChiefType, EmploymentType};

        let term_start = ymd(2026, 9, 15);
        let term_dates = term(term_start, ymd(2027, 1, 31));
        let term_name = term_dates.term.clone();

        // 1-2 öğrenci -> 4 saat; 3+ öğrenci -> 6 saat (mesafeden bağımsız).
        let rules = vec![rule(1, 0.0, None, 1, Some(2), 4), rule(2, 0.0, None, 3, None, 6)];

        let mut ctx = ContextBuilder::new(ymd(2026, 11, 1), term_dates)
            .with_rules(rules)
            .with_company(1, "İşletme A", Some(10.0))
            .with_company(2, "İşletme B", Some(10.0))
            .with_student(100, "S Öğrenci")
            .with_student(101, "Diğer Öğrenci 1")
            .with_student(102, "Diğer Öğrenci 2")
            .with_teacher(5, "T1 Öğretmen")
            .with_event(stored_event(1, 1, Stream::Placement, 100, &term_name, term_start, EventPayload::StudentPlaced { to_company_id: 1, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(2, 1, Stream::Placement, 101, &term_name, term_start, EventPayload::StudentPlaced { to_company_id: 1, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(3, 1, Stream::Placement, 102, &term_name, term_start, EventPayload::StudentPlaced { to_company_id: 1, from_company_id: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_event(stored_event(4, 1, Stream::CompanyHours, 1, &term_name, term_start, EventPayload::HoursSet { state: crate::domain::history::events::HoursState { awarded_hours: 6, max_hours_snapshot: 6, is_honorary: false, is_locked: false, notes: String::new() }, previous_awarded: None, labels: Labels(Default::default()) }, true))
            .with_event(stored_event(5, 1, Stream::Coordination, 1, &term_name, term_start, EventPayload::CoordinatorAssigned { state: crate::domain::history::events::CoordinationState { teacher_id: 5, visit_day: 1, visit_hour: 3, is_forced: false, force_reason: None }, from_teacher_id: None, labels: Labels(Default::default()) }, true))
            .with_event(stored_event(6, 1, Stream::TeacherLoad, 5, &term_name, term_start, EventPayload::LoadSet { load: TeacherLoad { base_hours: 15, max_extra_hours: 24, other_extra_hours: 0, chief_type: ChiefType::None, employment_type: EmploymentType::Tenured }, previous: None, source: "opening".into(), labels: Labels(Default::default()) }, true))
            .with_change_set(ChangeSetFacts { id: 1, kind: "opening".into(), effective_date: term_start, revokes_change_set_id: None, revoked_by: None })
            .build();

        let mut sim = SimCommit::starting_at(7, 2);
        let october_check = ymd(2026, 10, 15);
        let a_hours_in_october = ctx.timeline::<crate::domain::history::events::HoursState>(Stream::CompanyHours, 1, apply_hours).state_at(october_check).cloned();

        // 2. 5 Kasım: T1'in okuldaki ek dersi 0 -> 6.
        ctx.today = ymd(2026, 11, 5);
        let new_load = TeacherLoad { base_hours: 15, max_extra_hours: 24, other_extra_hours: 6, chief_type: ChiefType::None, employment_type: EmploymentType::Tenured };
        let load_req = ChangeRequest { term: term_name.clone(), effective_date: Some(ymd(2026, 11, 5)), document_date: None, reason: "ek görev".into(), command: ChangeCommand::SetTeacherLoad { teacher_id: 5, load: new_load } };
        let load_decision = decide(&ctx, &load_req).unwrap();
        sim.apply(&mut ctx, load_decision);
        assert_eq!(
            ctx.timeline::<crate::domain::history::events::HoursState>(Stream::CompanyHours, 1, apply_hours).state_at(october_check).cloned(),
            a_hours_in_october,
            "adım 2'den sonra Ekim durumu değişmemeli"
        );

        // 3. 10 Kasım (today): S, yeni işletme C'ye nakledilir; yürürlük 3 Kasım.
        ctx.today = ymd(2026, 11, 10);
        ctx.companies.insert(3, CompanyFacts { name: "Yeni İşletme C".into(), round_trip_km: None, is_active: true });
        ctx.materialized = Materialized { company_id: Some(3), student_id: None, teacher_id: None };
        let new_company = NewCompany { name: "Yeni İşletme C".into(), contact_first_name: String::new(), contact_last_name: String::new(), phone: String::new(), email: String::new(), address_text: String::new(), latitude: None, longitude: None, one_way_distance_km: None, district: String::new(), notes: String::new() };
        let transfer_to_c = ChangeRequest {
            term: term_name.clone(),
            effective_date: Some(ymd(2026, 11, 3)),
            document_date: None,
            reason: "nakil".into(),
            command: ChangeCommand::TransferStudent { student_id: 100, from_company_id: 1, to: TransferTarget::New { company: new_company } },
        };
        let transfer_decision = decide(&ctx, &transfer_to_c).unwrap();
        let transfer_change_set_id = sim.apply(&mut ctx, transfer_decision);
        assert_eq!(
            ctx.timeline::<crate::domain::history::events::HoursState>(Stream::CompanyHours, 1, apply_hours).state_at(october_check).cloned(),
            a_hours_in_october,
            "adım 3'ten sonra Ekim durumu değişmemeli"
        );

        // 4. 12 Kasım (today): düzeltme — nakil aslında MEVCUT işletme B'ye.
        ctx.today = ymd(2026, 11, 12);
        ctx.materialized = Materialized::default();
        let replacement = ChangeCommand::TransferStudent { student_id: 100, from_company_id: 1, to: TransferTarget::Existing { company_id: 2 } };
        let correct_req = ChangeRequest {
            term: term_name.clone(),
            effective_date: Some(ymd(2026, 11, 3)),
            document_date: None,
            reason: "düzeltme".into(),
            command: ChangeCommand::Correct { change_set_id: transfer_change_set_id, replacement: Box::new(replacement) },
        };
        let correct_decision = decide(&ctx, &correct_req).unwrap();

        assert_eq!(correct_decision.change_set.kind, "correct", "düzeltme TEK bir karardır");
        assert_eq!(correct_decision.change_set.revokes_change_set_id, Some(transfer_change_set_id));
        assert!(correct_decision.row_actions.contains(&RowAction::DeactivateCompany(3)), "C işletmesi pasifleşmeli");

        sim.apply(&mut ctx, correct_decision);
        assert_eq!(
            ctx.timeline::<crate::domain::history::events::HoursState>(Stream::CompanyHours, 1, apply_hours).state_at(october_check).cloned(),
            a_hours_in_october,
            "adım 4'ten sonra Ekim durumu değişmemeli"
        );

        // A'nın saati, (3 -> 2 öğrenci) tavanına göre 4'e inmiş olarak doğru kalmalı.
        let a_hours_timeline = ctx.timeline::<crate::domain::history::events::HoursState>(Stream::CompanyHours, 1, apply_hours);
        let a_hours_final = a_hours_timeline.state_at(ymd(2026, 11, 3)).cloned().unwrap();
        assert_eq!(a_hours_final.awarded_hours, 4, "A'nın saati B'ye naklin tavanına göre doğru kalmalı");

        // B, S'yi almış olmalı.
        let b_students = ctx.company_student_count_timeline(2).state_at(ymd(2026, 11, 3)).copied();
        assert_eq!(b_students, Some(1));

        // Hiçbir aralık çakışmaz.
        for w in a_hours_timeline.intervals.windows(2) {
            assert!(w[0].valid_to.is_some_and(|vt| vt <= w[1].valid_from), "A'nın saat aralıkları çakışmamalı");
        }
    }
}
