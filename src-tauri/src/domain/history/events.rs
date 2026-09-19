//! Olay akışları ve yükleri (spec §4.2).
//!
//! `EventPayload::decode` günlükten okunan her olay için TEK upcast
//! noktasıdır: bilinmeyen tür ya da sürüm burada bir `AppError` olarak
//! yakalanır, asla panic etmez (MESNET #137 dersi — spec §4.2 "Sürümleme").

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::domain::models::{ChiefType, EmploymentType};
use crate::domain::scheduling::Slot;
use crate::error::{AppError, AppResult};

/// Şu an desteklenen tek `kind_version`. Bir alan eklemek bunu artırmaz
/// (`#[serde(default)]` yeter); kırıcı bir değişiklik yeni bir `XxxV2`
/// yapısı ve `decode` içine yeni bir kol ister (spec §4.2 "Sürümleme").
pub const CURRENT_KIND_VERSION: i64 = 1;

/// Bir öznenin tarihçesinin ait olduğu akış; her akışın kendi projeksiyon
/// tablosu vardır (spec §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stream {
    Placement,
    CompanyHours,
    Coordination,
    TeacherLoad,
    TeacherSchedule,
}

impl Stream {
    pub fn as_str(self) -> &'static str {
        match self {
            Stream::Placement => "placement",
            Stream::CompanyHours => "company_hours",
            Stream::Coordination => "coordination",
            Stream::TeacherLoad => "teacher_load",
            Stream::TeacherSchedule => "teacher_schedule",
        }
    }

    pub fn parse(raw: &str) -> AppResult<Stream> {
        match raw {
            "placement" => Ok(Stream::Placement),
            "company_hours" => Ok(Stream::CompanyHours),
            "coordination" => Ok(Stream::Coordination),
            "teacher_load" => Ok(Stream::TeacherLoad),
            "teacher_schedule" => Ok(Stream::TeacherSchedule),
            other => Err(AppError::Database(format!(
                "Bilinmeyen olay akışı: '{other}'"
            ))),
        }
    }
}

/// `company_hours` akışının durumu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoursState {
    pub awarded_hours: i64,
    pub max_hours_snapshot: i64,
    pub is_honorary: bool,
    pub is_locked: bool,
    pub notes: String,
}

/// `coordination` akışının durumu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinationState {
    pub teacher_id: i64,
    pub visit_day: i64,
    pub visit_hour: i64,
    pub is_forced: bool,
    pub force_reason: Option<String>,
}

/// `teacher_load` akışının durumu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeacherLoad {
    pub base_hours: i64,
    pub max_extra_hours: i64,
    pub other_extra_hours: i64,
    pub chief_type: ChiefType,
    pub employment_type: EmploymentType,
}

/// `teacher_schedule` akışının durumu — boş saatlerin kümesi.
///
/// `scheduling::Slot` yeniden kullanılır, ama JSON biçimi Slot'un kendi
/// `{"dayOfWeek":.., "hour":..}` gösterimi DEĞİL, `db/projection.rs`'in
/// `slots_json` sütunuyla aynı `[[gün, saat], ...]` çift dizisidir
/// (spec §4.1).
#[derive(Debug, Clone, PartialEq)]
pub struct WeeklySchedule(pub BTreeSet<Slot>);

impl Serialize for WeeklySchedule {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let pairs: Vec<(i64, i64)> = self.0.iter().map(|s| (s.day_of_week, s.hour)).collect();
        pairs.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WeeklySchedule {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let pairs = Vec::<(i64, i64)>::deserialize(deserializer)?;
        Ok(WeeklySchedule(
            pairs.into_iter().map(|(day, hour)| Slot::new(day, hour)).collect(),
        ))
    }
}

/// Kayıt anındaki ad anlık görüntüsü (ör. `{"student": "Ahmet Yılmaz"}`).
/// Yeniden adlandırma geçmişi bozmasın diye olayın kendi içinde saklanır.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Labels(pub BTreeMap<String, String>);

/// Bir akış olayının yükü. Sonuç durumunu (sıralamaya dayanıklı) ve varsa
/// beklenen önceki değeri taşır (spec §4.2).
#[derive(Debug, Clone, PartialEq)]
pub enum EventPayload {
    StudentPlaced {
        to_company_id: i64,
        from_company_id: Option<i64>,
        source: String,
        labels: Labels,
    },
    StudentTransferred {
        from_company_id: i64,
        to_company_id: i64,
        to_company_created: bool,
        labels: Labels,
    },
    StudentLeft {
        from_company_id: i64,
        labels: Labels,
    },
    HoursSet {
        state: HoursState,
        previous_awarded: Option<i64>,
        labels: Labels,
    },
    /// Bir sınırlamadır, anlık görüntü değildir (spec §4.2). Katlamada
    /// `apply::apply_hours` tarafından `min(prev.awarded, cap)` olarak
    /// yorumlanır; burada yalnız kararın kendisi taşınır.
    HoursCapped {
        cap: i64,
        student_count: i64,
        labels: Labels,
    },
    HoursCleared {
        labels: Labels,
    },
    CoordinatorAssigned {
        state: CoordinationState,
        from_teacher_id: Option<i64>,
        labels: Labels,
    },
    CoordinatorEnded {
        from_teacher_id: i64,
        labels: Labels,
    },
    CoordinatorEndedByPolicy {
        from_teacher_id: i64,
        student_count: i64,
        labels: Labels,
    },
    LoadSet {
        load: TeacherLoad,
        previous: Option<TeacherLoad>,
        source: String,
        labels: Labels,
    },
    ScheduleSet {
        schedule: WeeklySchedule,
        previous_slot_count: Option<i64>,
        source: String,
        labels: Labels,
    },
    /// Hedefi `StoredEvent.revokes` sütununda taşınan geri alma işareti.
    /// Yükü yoktur; katlamada hem bu olay hem hedefi atılır (timeline.rs).
    Revoked,
}

/// `encode()` çıktısı: `change_events` tablosunun üç sütununa karşılık gelir.
#[derive(Debug, Clone, PartialEq)]
pub struct EncodedEvent {
    pub kind: &'static str,
    pub version: i64,
    pub json: String,
}

/// Kaydedilmiş, çözülmüş bir olay (günlükten `db::change_log::load_term_events`
/// ile okunur — R3).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEvent {
    pub id: i64,
    pub change_set_id: i64,
    pub stream: Stream,
    pub subject_id: i64,
    pub term: String,
    pub effective_date: NaiveDate,
    pub payload: EventPayload,
    pub caused_by: Option<i64>,
    pub revokes: Option<i64>,
    /// Kümenin `kind`'ı `'opening'` ise `true`. Katlamada saklanan tarih
    /// değil `terms.start_date` kullanılır (spec §5.1).
    pub is_opening: bool,
}

impl EventPayload {
    pub fn kind(&self) -> &'static str {
        match self {
            EventPayload::StudentPlaced { .. } => "student_placed",
            EventPayload::StudentTransferred { .. } => "student_transferred",
            EventPayload::StudentLeft { .. } => "student_left",
            EventPayload::HoursSet { .. } => "hours_set",
            EventPayload::HoursCapped { .. } => "hours_capped",
            EventPayload::HoursCleared { .. } => "hours_cleared",
            EventPayload::CoordinatorAssigned { .. } => "coordinator_assigned",
            EventPayload::CoordinatorEnded { .. } => "coordinator_ended",
            EventPayload::CoordinatorEndedByPolicy { .. } => "coordinator_ended_by_policy",
            EventPayload::LoadSet { .. } => "load_set",
            EventPayload::ScheduleSet { .. } => "schedule_set",
            EventPayload::Revoked => "revoked",
        }
    }

    /// Olayın hangi projeksiyona katlanacağı. `Revoked` için `None`:
    /// hedefin akışı `StoredEvent.stream`'den zaten bilinir.
    pub fn stream(&self) -> Option<Stream> {
        match self {
            EventPayload::StudentPlaced { .. }
            | EventPayload::StudentTransferred { .. }
            | EventPayload::StudentLeft { .. } => Some(Stream::Placement),
            EventPayload::HoursSet { .. }
            | EventPayload::HoursCapped { .. }
            | EventPayload::HoursCleared { .. } => Some(Stream::CompanyHours),
            EventPayload::CoordinatorAssigned { .. }
            | EventPayload::CoordinatorEnded { .. }
            | EventPayload::CoordinatorEndedByPolicy { .. } => Some(Stream::Coordination),
            EventPayload::LoadSet { .. } => Some(Stream::TeacherLoad),
            EventPayload::ScheduleSet { .. } => Some(Stream::TeacherSchedule),
            EventPayload::Revoked => None,
        }
    }

    pub fn encode(&self) -> AppResult<EncodedEvent> {
        match self {
            EventPayload::StudentPlaced { to_company_id, from_company_id, source, labels } => {
                encode_payload(self.kind(), &StudentPlacedV1 {
                    to_company_id: *to_company_id,
                    from_company_id: *from_company_id,
                    source: source.clone(),
                    labels: labels.clone(),
                })
            }
            EventPayload::StudentTransferred { from_company_id, to_company_id, to_company_created, labels } => {
                encode_payload(self.kind(), &StudentTransferredV1 {
                    from_company_id: *from_company_id,
                    to_company_id: *to_company_id,
                    to_company_created: *to_company_created,
                    labels: labels.clone(),
                })
            }
            EventPayload::StudentLeft { from_company_id, labels } => {
                encode_payload(self.kind(), &StudentLeftV1 { from_company_id: *from_company_id, labels: labels.clone() })
            }
            EventPayload::HoursSet { state, previous_awarded, labels } => {
                encode_payload(self.kind(), &HoursSetV1 { state: state.clone(), previous_awarded: *previous_awarded, labels: labels.clone() })
            }
            EventPayload::HoursCapped { cap, student_count, labels } => {
                encode_payload(self.kind(), &HoursCappedV1 { cap: *cap, student_count: *student_count, labels: labels.clone() })
            }
            EventPayload::HoursCleared { labels } => {
                encode_payload(self.kind(), &HoursClearedV1 { labels: labels.clone() })
            }
            EventPayload::CoordinatorAssigned { state, from_teacher_id, labels } => {
                encode_payload(self.kind(), &CoordinatorAssignedV1 { state: state.clone(), from_teacher_id: *from_teacher_id, labels: labels.clone() })
            }
            EventPayload::CoordinatorEnded { from_teacher_id, labels } => {
                encode_payload(self.kind(), &CoordinatorEndedV1 { from_teacher_id: *from_teacher_id, labels: labels.clone() })
            }
            EventPayload::CoordinatorEndedByPolicy { from_teacher_id, student_count, labels } => {
                encode_payload(self.kind(), &CoordinatorEndedByPolicyV1 { from_teacher_id: *from_teacher_id, student_count: *student_count, labels: labels.clone() })
            }
            EventPayload::LoadSet { load, previous, source, labels } => {
                encode_payload(self.kind(), &LoadSetV1 { load: load.clone(), previous: previous.clone(), source: source.clone(), labels: labels.clone() })
            }
            EventPayload::ScheduleSet { schedule, previous_slot_count, source, labels } => {
                encode_payload(self.kind(), &ScheduleSetV1 { schedule: schedule.clone(), previous_slot_count: *previous_slot_count, source: source.clone(), labels: labels.clone() })
            }
            EventPayload::Revoked => encode_payload(self.kind(), &serde_json::json!({})),
        }
    }

    /// TEK upcast noktası. `version` şu an tüm türler için tek ve global bir
    /// sayaçtır (`CURRENT_KIND_VERSION`); kırıcı bir değişiklik bu kontrolü
    /// tür başına ayırmayı gerektirecektir — bugün gerek yok.
    pub fn decode(kind: &str, version: i64, json: &str) -> AppResult<EventPayload> {
        if version != CURRENT_KIND_VERSION {
            return Err(AppError::Database(format!(
                "Bilinmeyen olay sürümü: '{kind}' v{version} (yalnız v{CURRENT_KIND_VERSION} destekleniyor)"
            )));
        }
        match kind {
            "student_placed" | "student_transferred" | "student_left" => decode_student_event(kind, json),
            "hours_set" | "hours_capped" | "hours_cleared" => decode_hours_event(kind, json),
            "coordinator_assigned" | "coordinator_ended" | "coordinator_ended_by_policy" => {
                decode_coordination_event(kind, json)
            }
            "load_set" | "schedule_set" => decode_teacher_event(kind, json),
            "revoked" => Ok(EventPayload::Revoked),
            other => Err(AppError::Database(format!("Bilinmeyen olay türü: '{other}'"))),
        }
    }
}

fn encode_payload<T: Serialize>(kind: &'static str, value: &T) -> AppResult<EncodedEvent> {
    let json = serde_json::to_string(value)
        .map_err(|e| AppError::Database(format!("Olay yükü kodlanamadı ('{kind}'): {e}")))?;
    Ok(EncodedEvent { kind, version: CURRENT_KIND_VERSION, json })
}

fn decode_payload<T: for<'de> Deserialize<'de>>(kind: &str, json: &str) -> AppResult<T> {
    serde_json::from_str(json)
        .map_err(|e| AppError::Database(format!("Olay yükü çözülemedi ('{kind}'): {e}")))
}

fn decode_student_event(kind: &str, json: &str) -> AppResult<EventPayload> {
    match kind {
        "student_placed" => {
            let v: StudentPlacedV1 = decode_payload(kind, json)?;
            Ok(EventPayload::StudentPlaced {
                to_company_id: v.to_company_id,
                from_company_id: v.from_company_id,
                source: v.source,
                labels: v.labels,
            })
        }
        "student_transferred" => {
            let v: StudentTransferredV1 = decode_payload(kind, json)?;
            Ok(EventPayload::StudentTransferred {
                from_company_id: v.from_company_id,
                to_company_id: v.to_company_id,
                to_company_created: v.to_company_created,
                labels: v.labels,
            })
        }
        _ => {
            let v: StudentLeftV1 = decode_payload(kind, json)?;
            Ok(EventPayload::StudentLeft { from_company_id: v.from_company_id, labels: v.labels })
        }
    }
}

fn decode_hours_event(kind: &str, json: &str) -> AppResult<EventPayload> {
    match kind {
        "hours_set" => {
            let v: HoursSetV1 = decode_payload(kind, json)?;
            Ok(EventPayload::HoursSet { state: v.state, previous_awarded: v.previous_awarded, labels: v.labels })
        }
        "hours_capped" => {
            let v: HoursCappedV1 = decode_payload(kind, json)?;
            Ok(EventPayload::HoursCapped { cap: v.cap, student_count: v.student_count, labels: v.labels })
        }
        _ => {
            let v: HoursClearedV1 = decode_payload(kind, json)?;
            Ok(EventPayload::HoursCleared { labels: v.labels })
        }
    }
}

fn decode_coordination_event(kind: &str, json: &str) -> AppResult<EventPayload> {
    match kind {
        "coordinator_assigned" => {
            let v: CoordinatorAssignedV1 = decode_payload(kind, json)?;
            Ok(EventPayload::CoordinatorAssigned { state: v.state, from_teacher_id: v.from_teacher_id, labels: v.labels })
        }
        "coordinator_ended" => {
            let v: CoordinatorEndedV1 = decode_payload(kind, json)?;
            Ok(EventPayload::CoordinatorEnded { from_teacher_id: v.from_teacher_id, labels: v.labels })
        }
        _ => {
            let v: CoordinatorEndedByPolicyV1 = decode_payload(kind, json)?;
            Ok(EventPayload::CoordinatorEndedByPolicy {
                from_teacher_id: v.from_teacher_id,
                student_count: v.student_count,
                labels: v.labels,
            })
        }
    }
}

fn decode_teacher_event(kind: &str, json: &str) -> AppResult<EventPayload> {
    match kind {
        "load_set" => {
            let v: LoadSetV1 = decode_payload(kind, json)?;
            Ok(EventPayload::LoadSet { load: v.load, previous: v.previous, source: v.source, labels: v.labels })
        }
        _ => {
            let v: ScheduleSetV1 = decode_payload(kind, json)?;
            Ok(EventPayload::ScheduleSet {
                schedule: v.schedule,
                previous_slot_count: v.previous_slot_count,
                source: v.source,
                labels: v.labels,
            })
        }
    }
}

// --- Tel (wire) yapıları: yalnız kodlama/çözme için, dışarı açılmaz. ---

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StudentPlacedV1 {
    to_company_id: i64,
    from_company_id: Option<i64>,
    source: String,
    labels: Labels,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StudentTransferredV1 {
    from_company_id: i64,
    to_company_id: i64,
    to_company_created: bool,
    labels: Labels,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StudentLeftV1 {
    from_company_id: i64,
    labels: Labels,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HoursSetV1 {
    state: HoursState,
    previous_awarded: Option<i64>,
    labels: Labels,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HoursCappedV1 {
    cap: i64,
    student_count: i64,
    labels: Labels,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HoursClearedV1 {
    labels: Labels,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoordinatorAssignedV1 {
    state: CoordinationState,
    from_teacher_id: Option<i64>,
    labels: Labels,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoordinatorEndedV1 {
    from_teacher_id: i64,
    labels: Labels,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoordinatorEndedByPolicyV1 {
    from_teacher_id: i64,
    student_count: i64,
    labels: Labels,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadSetV1 {
    load: TeacherLoad,
    previous: Option<TeacherLoad>,
    source: String,
    labels: Labels,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleSetV1 {
    schedule: WeeklySchedule,
    previous_slot_count: Option<i64>,
    source: String,
    labels: Labels,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(pairs: &[(&str, &str)]) -> Labels {
        Labels(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }

    fn sample_hours_state() -> HoursState {
        HoursState { awarded_hours: 6, max_hours_snapshot: 6, is_honorary: false, is_locked: false, notes: String::new() }
    }

    fn sample_coordination_state() -> CoordinationState {
        CoordinationState { teacher_id: 3, visit_day: 1, visit_hour: 3, is_forced: false, force_reason: None }
    }

    fn sample_teacher_load() -> TeacherLoad {
        TeacherLoad { base_hours: 15, max_extra_hours: 24, other_extra_hours: 0, chief_type: ChiefType::None, employment_type: EmploymentType::Tenured }
    }

    fn sample_schedule() -> WeeklySchedule {
        WeeklySchedule(BTreeSet::from([Slot::new(1, 3), Slot::new(2, 4)]))
    }

    fn all_payload_samples() -> Vec<EventPayload> {
        vec![
            EventPayload::StudentPlaced { to_company_id: 5, from_company_id: None, source: "manual".into(), labels: labels(&[("student", "Ahmet")]) },
            EventPayload::StudentTransferred { from_company_id: 5, to_company_id: 9, to_company_created: false, labels: labels(&[("student", "Ahmet")]) },
            EventPayload::StudentLeft { from_company_id: 5, labels: labels(&[("student", "Ahmet")]) },
            EventPayload::HoursSet { state: sample_hours_state(), previous_awarded: Some(4), labels: labels(&[("company", "X")]) },
            EventPayload::HoursCapped { cap: 4, student_count: 2, labels: labels(&[("company", "X")]) },
            EventPayload::HoursCleared { labels: labels(&[("company", "X")]) },
            EventPayload::CoordinatorAssigned { state: sample_coordination_state(), from_teacher_id: None, labels: labels(&[("teacher", "Ali")]) },
            EventPayload::CoordinatorEnded { from_teacher_id: 3, labels: labels(&[("teacher", "Ali")]) },
            EventPayload::CoordinatorEndedByPolicy { from_teacher_id: 3, student_count: 0, labels: labels(&[("teacher", "Ali")]) },
            EventPayload::LoadSet { load: sample_teacher_load(), previous: None, source: "manual".into(), labels: labels(&[("teacher", "Ali")]) },
            EventPayload::ScheduleSet { schedule: sample_schedule(), previous_slot_count: None, source: "manual".into(), labels: labels(&[("teacher", "Ali")]) },
            EventPayload::Revoked,
        ]
    }

    /// Her tür için encode→decode gidiş-dönüşü kayıpsız olmalı.
    #[test]
    fn every_kind_roundtrips_through_encode_and_decode() {
        for payload in all_payload_samples() {
            let encoded = payload.encode().unwrap();
            let decoded = EventPayload::decode(encoded.kind, encoded.version, &encoded.json).unwrap();
            assert_eq!(decoded, payload, "tür: {}", payload.kind());
        }
    }

    /// Her `(kind, version)` için diskteki golden fixture sonsuza kadar
    /// decode edilebilmelidir (spec §4.2 "Sürümleme", MESNET #137 dersi).
    #[test]
    fn golden_fixtures_decode_forever() {
        // NOT: plan metninde "11 dosya" deniyor ama kind() listesi 12 tür
        // içeriyor (revoked dahil); burada 12 tür de kapsanır — bkz. rapor.
        let fixtures: &[(&str, &str)] = &[
            ("student_placed", include_str!("fixtures/student_placed.v1.json")),
            ("student_transferred", include_str!("fixtures/student_transferred.v1.json")),
            ("student_left", include_str!("fixtures/student_left.v1.json")),
            ("hours_set", include_str!("fixtures/hours_set.v1.json")),
            ("hours_capped", include_str!("fixtures/hours_capped.v1.json")),
            ("hours_cleared", include_str!("fixtures/hours_cleared.v1.json")),
            ("coordinator_assigned", include_str!("fixtures/coordinator_assigned.v1.json")),
            ("coordinator_ended", include_str!("fixtures/coordinator_ended.v1.json")),
            ("coordinator_ended_by_policy", include_str!("fixtures/coordinator_ended_by_policy.v1.json")),
            ("load_set", include_str!("fixtures/load_set.v1.json")),
            ("schedule_set", include_str!("fixtures/schedule_set.v1.json")),
            ("revoked", include_str!("fixtures/revoked.v1.json")),
        ];
        assert_eq!(fixtures.len(), 12, "her tür için tam olarak bir golden fixture olmalı");
        for (kind, json) in fixtures {
            let decoded = EventPayload::decode(kind, CURRENT_KIND_VERSION, json);
            assert!(decoded.is_ok(), "fixture decode edilemedi: {kind}: {decoded:?}");
            assert_eq!(decoded.unwrap().kind(), *kind);
        }
    }

    #[test]
    fn unknown_kind_is_an_error_not_a_panic() {
        let result = EventPayload::decode("bilinmeyen_tur", CURRENT_KIND_VERSION, "{}");
        assert!(matches!(result, Err(AppError::Database(_))));
    }

    #[test]
    fn unknown_version_is_an_error() {
        let result = EventPayload::decode("student_placed", 99, "{}");
        assert!(matches!(result, Err(AppError::Database(_))));
    }

    /// `WeeklySchedule` JSON biçimi Slot'un kendi gösterimi değil, projeksiyon
    /// sütunuyla aynı `[[gün, saat], ...]` çift dizisidir.
    #[test]
    fn weekly_schedule_serializes_as_pair_arrays_not_objects() {
        let payload = EventPayload::ScheduleSet {
            schedule: sample_schedule(),
            previous_slot_count: None,
            source: "manual".into(),
            labels: labels(&[]),
        };
        let encoded = payload.encode().unwrap();
        assert!(encoded.json.contains("[[1,3],[2,4]]"), "biçim çift dizisi olmalı: {}", encoded.json);
    }
}
