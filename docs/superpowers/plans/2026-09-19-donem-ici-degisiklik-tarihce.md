# Dönem İçi Değişiklik Tarihçesi — Uygulama Planı (Faz A)

> **Ajanlar için:** Bu plan, `superpowers:subagent-driven-development` akışıyla yürütülür. Her görev bir `mesnet-rust` ya da `mesnet-vue` ajanına brief olarak verilir. Proje `CLAUDE.md` kuralı gereği bu plan **kodu içermez**: dosyaları, imzaları, test adlarını, kapıları ve dokunulmayacak dosyaları verir; kodu ajan yazar. Commit'i orkestratör, doğrulamadan **sonra** atar.

**Hedef:** Öğrenci nakli/ayrılışı ve öğretmen yük, program ve koordinatör değişiklikleri; yürürlük tarihli, değişmez bir olay günlüğüne yazılsın. Durum, herhangi bir gün itibarıyla okunabilsin.

**Mimari:**
- Doğruluk kaynağı `change_sets` ve `change_events` tablolarıdır.
- Beş adet tarih aralıklı projeksiyon tablosu, ilgili öznenin olayları `(effective_date, id)` sırasıyla katlanarak, aynı `BEGIN IMMEDIATE` transaction'ı içinde yeniden kurulur.
- `decide` ve `apply` saf fonksiyonlardır (esrs deseni). Zincir etki kararın içinde, aynı transaction'da çalışır.

**Teknoloji:** Rust (sqlx 0.8.6 SQLite, chrono 0.4, serde_json), Tauri 2, Vue 3 + TypeScript + OpenVue.

**Spec:** `docs/superpowers/specs/2026-09-19-donem-ici-degisiklik-tarihce-design.md`. Bu plandaki her görevin gereksinimi, spec'in ilgili bölümünü kapsar.

## Genel Kısıtlar

- **Yeni crate yok.** Tek bağımlılık değişikliği: `src-tauri/Cargo.toml` içindeki sqlx özelliklerine `"chrono"` eklenir. Test için yeni crate de yok; `proptest` ve `fastrand` kullanılmaz.
- **Transaction açılışı:** `pool.begin_with("BEGIN IMMEDIATE")` kullanılır (sqlx-core 0.8.6, `pool/mod.rs:391`).
- **Transaction içinde havuz (`&SqlitePool`) kullanılmaz.** Her okuma ve yazma aynı `&mut SqliteConnection` üzerinden yapılır.
- **Tarih biçimleri:**
  - Yürürlük tarihi: `'YYYY-MM-DD'` (yerel takvim günü).
  - `recorded_at`: UTC, `Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)`.
- **`today` hesabı:** `chrono::Utc::now().with_timezone(&FixedOffset::east_opt(3 * 3600))`. Türkiye 2016'dan beri sabit UTC+3'tür. `chrono-tz` eklenmez.
- **Ay penceresi:** Dönem başladıysa yürürlük tarihi `>= max(terms.start_date, today'in ayının 1'i)` olmalıdır. Aksi hâlde `previousMonthClosed` döner.
- **Yürürlük tarihi**, yeni durumun geçerli olduğu **ilk gündür**. Aralıklar yarı açıktır: `[valid_from, valid_to)`.
- **SQL tarih kontrolü:** `CHECK (date(x) IS x)`. `= x` kullanılmaz.
- **Uyumluluk:** Bundled SQLite 3.46.0.
- **Göç sırası:** `0006` yalnız ekleme yapar. `0007` en son görevde gelir. Ana tablolar (`students`, `teachers`, `companies`) **asla** `DROP TABLE` ile yeniden kurulmaz.
- **Kapılar:**
  - Rust: `cargo test` ve `cargo check` (`src-tauri/` içinde).
  - Vue: `npx vue-tsc --noEmit` ve `npm run build`.
  - Mevcut testler her görevden sonra yeşil kalmalıdır. Başlangıç: 281 backend, 3 frontend test.
- **Dil ve metin:**
  - Türkçe yorum, tam diakritikle.
  - Kullanıcıya görünen her metin `src/i18n/labels.ts` içinde olur.
  - Rust'taki hata ve `Rejection` mesajları Türkçedir.
- **camelCase sözleşmesi:** Spec §8 birebir uygulanır. Arayüz ajanı alan adı uydurmaz.
- **Eşzamanlılık:** Rust görevleri **sırayla** koşar, çünkü aynı crate üzerinde paralel `cargo test` birbirini bozar. Vue görevleri, V1 bittikten sonra, dosyaları ayrık olmak şartıyla Rust görevleriyle paralel koşabilir.
- **Gerçek veritabanı:** Kullanıcının `mesnet-lite.db` dosyası R6 bitene kadar bu daldaki uygulamayla açılmaz. Orkestratör yalnız bir **kopya** üzerinde doğrulama yapar.

## Dosya haritası

**Yeni — saf çekirdek** (`src-tauri/src/domain/`):

| Dosya | Sorumluluk |
|---|---|
| `terms.rs` | `TermDates`, ay penceresi, yürürlük tarihi çözümü, `today_local()` |
| `hour_rules.rs` | `HourRule` ve `select_narrowest`, `db/hour_rules.rs`'ten **taşınır** |
| `history/mod.rs` | Modül kökü ve dışa açılan öğeler |
| `history/rejection.rs` | `Rejection`, `RejectionCode` |
| `history/events.rs` | `Stream`, durum yapıları, `EventPayload` (12 tür), encode/decode (tek upcast noktası) |
| `history/apply.rs` | Akış başına saf `apply` (hours_capped bir sınırlamadır, kilide bakar) |
| `history/timeline.rs` | `TimedEvent`, `order_events`, `fold_intervals`, `Timeline<S>` |
| `history/impact.rs` | `ImpactSummary`, `ImpactLine`, `ImpactWarning`, `ImpactNotice` |
| `history/policy.rs` | `cap_for`, işletme politikası yürüyüşü, kapasite ve program bayrakları |
| `history/decide/mod.rs` | `ChangeRequest`, `ChangeCommand`, `DecisionContext`, `Decision`, `decide()`, sonraki olay denetimi |
| `history/decide/student.rs` | Yerleştirme, nakil, ayrılış, öğrenci ekleme ve silme |
| `history/decide/company.rs` | Saat ve koordinasyon |
| `history/decide/teacher.rs` | Yük, program, öğretmen ekleme ve silme |
| `history/decide/revoke.rs` | Geri alma ve düzeltme |
| `history/audit.rs` | Denetim listesinde "önce/sonra" betimi |

**Yeni — kalıcılık:**

| Dosya | Sorumluluk |
|---|---|
| `src-tauri/migrations/0006_history.sql` | Tablolar, tetikleyiciler, açılış tohumu |
| `src-tauri/src/db/terms.rs` | `terms` okuma/yazma, tarih güncelleme kuralları |
| `src-tauri/src/db/change_log.rs` | Günlüğe ekleme, okuma, high-water, tarihçe listesi |
| `src-tauri/src/db/projection.rs` | Beş projeksiyonun **tek** yazıcısı: `rebuild_stream`, `rebuild_term`, `verify_term` |
| `src-tauri/src/db/history_context.rs` | Transaction içinde `DecisionContext` yükleme |

**Yeni — servis ve komut:**

| Dosya | Sorumluluk |
|---|---|
| `src-tauri/src/services/change_service.rs` | `execute_change`, `execute_in` (esrs AggregateManager rolü) |
| `src-tauri/src/commands/history_commands.rs` | `preview_change`, `commit_change`, `list_history`, `get_subject_history` |
| `src-tauri/src/commands/term_commands.rs` | `list_terms_with_dates`, `update_term_dates` |

**Yeni — arayüz** (`src/`):
- `api/history.ts`
- `composables/useAsOfDate.ts`, `composables/useChange.ts`
- `components/history/{ImpactDialog,EffectiveDateField,AsOfDatePicker,HistoryEntryCard}.vue`
- `components/student/StudentChangeDialog.vue`
- `components/teacher/TeacherLoadDialog.vue`
- `views/HistoryView.vue`

---

## Görev R1 — Saf çekirdek I: dönem tarihleri, olaylar, katlama

**Ajan:** `mesnet-rust`

**Dosyalar:**
- **Oluştur:**
  - `src-tauri/src/domain/terms.rs`
  - `src-tauri/src/domain/hour_rules.rs`
  - `src-tauri/src/domain/history/{mod.rs, rejection.rs, events.rs, apply.rs, timeline.rs}`
  - `src-tauri/src/domain/history/fixtures/*.json` (golden fixture'lar)
- **Değiştir:**
  - `src-tauri/src/domain/mod.rs`: `pub mod terms; pub mod hour_rules; pub mod history;`
  - `src-tauri/src/db/hour_rules.rs`: `HourRule`, `select_narrowest` ve yardımcıları `domain/hour_rules.rs`'e taşınır. Burada yalnız `pub use crate::domain::hour_rules::{HourRule, select_narrowest};` kalır, böylece mevcut çağıranlar değişmeden derlenir.
  - `src-tauri/src/domain/workload.rs`: `teacher_capacity` eklenir (aşağıda).
  - `src-tauri/Cargo.toml`: sqlx özelliklerine `"chrono"` eklenir.
- **Dokunma:** `db/` altında yalnız `db/hour_rules.rs` dosyasına dokunulur. `commands/`, `services/`, `lib.rs`, `migrations/` ve `src/` (Vue) dışarıda kalır.

**Üretir** (sonraki görevler bu adlara dayanır; birebir uy):

```rust
// domain/terms.rs
pub struct TermDates { pub term: String, pub start: NaiveDate, pub end: NaiveDate, pub dates_confirmed: bool }
impl TermDates {
    pub fn default_for(term: &str) -> TermDates;   // "YYYY-YYYY/1" → YYYY-09-01..(YYYY+1)-01-31 ; "/2" → (YYYY+1)-02-01..(YYYY+1)-06-30 ; bozuk → 2000-01-01..2099-12-31
    pub fn is_planning(&self, today: NaiveDate) -> bool;              // today < start
    pub fn default_as_of(&self, today: NaiveDate) -> NaiveDate;       // today, [start, end] aralığına sıkıştırılır
    pub fn earliest_allowed(&self, today: NaiveDate) -> NaiveDate;    // planlamada start; değilse max(start, first_of_month(today))
    pub fn resolve_effective_date(&self, requested: Option<NaiveDate>, today: NaiveDate) -> Result<NaiveDate, Rejection>;
}
pub fn first_of_month(d: NaiveDate) -> NaiveDate;
pub fn today_local() -> NaiveDate;   // UTC+3 sabit
pub fn parse_date(raw: &str) -> AppResult<NaiveDate>;   // 'YYYY-MM-DD' dışında AppError::Validation

// domain/history/rejection.rs
#[derive(Debug, Clone, PartialEq, Serialize)] #[serde(rename_all = "camelCase")]
pub enum RejectionCode { PreviousMonthClosed, EffectiveDateRequired, OutOfTerm, FactNotTrueAtDate,
    ConflictsWithLaterChange, AboveCap, BlockOverlap, NotRevocable, HasDependents, HasHistory, PlanningOnly }
#[derive(Debug, Clone, PartialEq)]
pub struct Rejection { pub code: RejectionCode, pub message: String, pub conflicting_change_set_ids: Vec<i64>, pub suggested_date: Option<NaiveDate> }

// domain/history/events.rs
pub enum Stream { Placement, CompanyHours, Coordination, TeacherLoad, TeacherSchedule }   // as_str(): "placement" | "company_hours" | "coordination" | "teacher_load" | "teacher_schedule"; parse(&str) -> AppResult<Stream>
pub struct HoursState { pub awarded_hours: i64, pub max_hours_snapshot: i64, pub is_honorary: bool, pub is_locked: bool, pub notes: String }
pub struct CoordinationState { pub teacher_id: i64, pub visit_day: i64, pub visit_hour: i64, pub is_forced: bool, pub force_reason: Option<String> }
pub struct TeacherLoad { pub base_hours: i64, pub max_extra_hours: i64, pub other_extra_hours: i64, pub chief_type: ChiefType, pub employment_type: EmploymentType }
pub struct WeeklySchedule(pub BTreeSet<Slot>);   // scheduling::Slot yeniden kullanılır; JSON biçimi [[gün, saat], ...]
pub struct Labels(pub BTreeMap<String, String>); // ör. {"student": "Ahmet Yılmaz", "to_company": "X A.Ş."}
pub enum EventPayload {
    StudentPlaced { to_company_id: i64, from_company_id: Option<i64>, source: String, labels: Labels },
    StudentTransferred { from_company_id: i64, to_company_id: i64, to_company_created: bool, labels: Labels },
    StudentLeft { from_company_id: i64, labels: Labels },
    HoursSet { state: HoursState, previous_awarded: Option<i64>, labels: Labels },
    HoursCapped { cap: i64, student_count: i64, labels: Labels },
    HoursCleared { labels: Labels },
    CoordinatorAssigned { state: CoordinationState, from_teacher_id: Option<i64>, labels: Labels },
    CoordinatorEnded { from_teacher_id: i64, labels: Labels },
    CoordinatorEndedByPolicy { from_teacher_id: i64, student_count: i64, labels: Labels },
    LoadSet { load: TeacherLoad, previous: Option<TeacherLoad>, source: String, labels: Labels },
    ScheduleSet { schedule: WeeklySchedule, previous_slot_count: Option<i64>, source: String, labels: Labels },
    Revoked,
}
impl EventPayload {
    pub fn kind(&self) -> &'static str;    // "student_placed", "student_transferred", "student_left", "hours_set", "hours_capped", "hours_cleared", "coordinator_assigned", "coordinator_ended", "coordinator_ended_by_policy", "load_set", "schedule_set", "revoked"
    pub fn stream(&self) -> Option<Stream>;   // Revoked için None (hedefin akışını taşır)
    pub fn encode(&self) -> AppResult<EncodedEvent>;   // EncodedEvent { kind: &'static str, version: i64, json: String }
    pub fn decode(kind: &str, version: i64, json: &str) -> AppResult<EventPayload>;   // TEK upcast noktası; bilinmeyen tür → AppError::Database, asla panic
}
pub const CURRENT_KIND_VERSION: i64 = 1;
pub struct StoredEvent { pub id: i64, pub change_set_id: i64, pub stream: Stream, pub subject_id: i64, pub term: String,
    pub effective_date: NaiveDate, pub payload: EventPayload, pub caused_by: Option<i64>, pub revokes: Option<i64>, pub is_opening: bool }

// domain/history/apply.rs — saf, girdiyi değiştirmez, yeni değer döner
pub fn apply_placement(prev: Option<&i64>, e: &EventPayload) -> Option<i64>;
pub fn apply_hours(prev: Option<&HoursState>, e: &EventPayload) -> Option<HoursState>;   // HoursCapped: prev yoksa None; prev.is_locked ise prev AYNEN; değilse awarded=min(prev.awarded,cap), max_hours_snapshot=min(prev.max,cap), diğerleri taşınır
pub fn apply_coordination(prev: Option<&CoordinationState>, e: &EventPayload) -> Option<CoordinationState>;
pub fn apply_load(prev: Option<&TeacherLoad>, e: &EventPayload) -> Option<TeacherLoad>;
pub fn apply_schedule(prev: Option<&WeeklySchedule>, e: &EventPayload) -> Option<WeeklySchedule>;

// domain/history/timeline.rs
pub struct TimedEvent { pub id: i64, pub effective_date: NaiveDate, pub payload: EventPayload }
/// Geri alınmış olayları ve geri alma işaretlerini atar, açılış olaylarının tarihini term_start yapar, (effective_date, id) sırasıyla dizer.
pub fn order_events(events: &[StoredEvent], term_start: NaiveDate) -> Vec<TimedEvent>;
pub struct Interval<S> { pub valid_from: NaiveDate, pub valid_to: Option<NaiveDate>, pub state: S, pub source_event_id: i64 }
/// Olaylar sırayla uygulanır. Aynı günün olayları id sırasıyla uygulanır ve o günün SON durumu geçerli olur.
/// Durum değişmediyse yeni aralık açılmaz. None bir boşluk bırakır. Sıfır uzunluklu aralık üretilmez.
pub fn fold_intervals<S: Clone + PartialEq>(events: &[TimedEvent], apply: impl Fn(Option<&S>, &EventPayload) -> Option<S>) -> Vec<Interval<S>>;
pub struct Timeline<S> { pub intervals: Vec<Interval<S>> }
impl<S: Clone> Timeline<S> {
    pub fn state_at(&self, d: NaiveDate) -> Option<&S>;          // yarı açık: valid_to günü dahil değil
    pub fn change_dates_from(&self, d: NaiveDate) -> Vec<NaiveDate>;   // valid_from ve valid_to değerlerinden >= d olanlar, sıralı ve tekil
}

// domain/workload.rs (ek) — üç komut dosyasındaki kopya türetmenin tek yeri
pub fn teacher_capacity(load: &TeacherLoad, statutory_cap: i64) -> i64;   // coordinator_capacity(max_extra, chief_type.weekly_hours(), other_extra, statutory_cap)
```

**Adımlar:**

- [ ] **1. Önce testler.** Aşağıdaki testleri yaz ve **düştüklerini gör**.
  - **`terms.rs`:**
    - `default_for_first_term_spans_september_to_january`
    - `default_for_second_term_spans_february_to_june`
    - `default_for_malformed_term_is_wide`
    - `planning_defaults_missing_date_to_start`
    - `running_term_requires_a_date`
    - `date_before_first_of_current_month_is_rejected_with_suggestion`: today 2026-11-10, istek 2026-10-28 → `PreviousMonthClosed`, `suggested_date` 2026-11-01.
    - `date_in_current_month_before_today_is_accepted`: today 2026-10-25, istek 2026-10-10 → kabul.
    - `future_date_inside_term_is_accepted`
    - `date_outside_term_is_rejected`
    - `earliest_allowed_never_precedes_term_start`
  - **`events.rs`:**
    - Her türün encode→decode gidiş-dönüşü.
    - `golden_fixtures_decode_forever`: `fixtures/<kind>.v1.json` dosyalarının her biri decode edilmeli. 11 dosya olur (`revoked` için `{}`).
    - `unknown_kind_is_an_error_not_a_panic`
    - `unknown_version_is_an_error`
  - **`apply.rs`:**
    - `hours_capped_clamps_down_only`
    - `hours_capped_does_not_touch_a_locked_row`
    - `hours_capped_without_prior_state_is_none`
    - `hours_capped_carries_notes_and_flags`
    - Her akış için "sonuç durumunu taşıyan olay önceki durumdan bağımsızdır" testi.
  - **`timeline.rs`:**
    - `fold_produces_half_open_sorted_intervals`
    - `same_day_later_recorded_event_wins`
    - `unchanged_state_opens_no_new_interval`
    - `none_leaves_a_gap`
    - `state_at_valid_to_is_excluded`
    - `revoked_events_and_markers_are_dropped`
    - `opening_events_are_placed_at_term_start`
  - **Özellik testleri** (`#[cfg(test)]` içinde elle yazılmış, tohumlu bir LCG; yeni crate yok):
    - **P1:** Aralıklar çakışmaz, sıralıdır, her `source_event_id` en fazla bir kez geçer.
    - **P2:** Farklı `(date, id)` değerli olayların giriş sırası karıştırılınca `order_events` ve `fold` sonucu aynı kalır. 6 olayın 720 permütasyonunun **hepsi** denenir.
    - **P3:** `state_at(x)`, tarihi `x` veya daha önce olan son olay uygulandıktan sonraki durumdur.
    - **P4 (ay değişmezliği):** Tarihi ≥ 2026-11-01 olan olaylar eklenince, `x ≤ 2026-10-31` için `state_at(x)` değişmez.
  - **`workload.rs`:** `teacher_capacity_matches_coordinator_capacity` (24/10/4/20 → 10).
- [ ] **2. Testleri çalıştır:** `cd src-tauri && cargo test domain::` → yeni testler DÜŞMELİ.
- [ ] **3. Uygula.** En küçük implementasyonu yaz. `db/hour_rules.rs` içindeki taşımayı yap; mevcut `hour_rules` testleri değişmeden yeşil kalmalı.
- [ ] **4. Tam kapı:** `cargo test && cargo check` → hepsi geçer. Mevcut 281 testin hiçbiri kırılmaz.
- [ ] **5. Rapor.**
  - Yeni test sayısı.
  - Değişen mevcut test (olmamalı).
  - Bu görevde üretim çağıranı olmayan fonksiyonlar. Bunlar R2 ve R3'te bağlanacak; ölü kod sayılmaz, ama listele.

**Orkestratör doğrulaması:**
- `cargo test` sayısını kendin çalıştır.
- `rg "select_narrowest" src-tauri/src` → tek tanım `domain/hour_rules.rs`'te olmalı.
- Fixture'ların gerçekten diskte olduğunu ve testin onları okuduğunu kontrol et.

**Commit:** `feat: add the pure history core — term dates, events, fold`

---

## Görev R2 — Saf çekirdek II: etki özeti, politika, karar

**Ajan:** `mesnet-rust` (R1'den sonra)

**Dosyalar:**
- **Oluştur:** `src-tauri/src/domain/history/{impact.rs, policy.rs, audit.rs}`, `src-tauri/src/domain/history/decide/{mod.rs, student.rs, company.rs, teacher.rs, revoke.rs}`
- **Değiştir:** `src-tauri/src/domain/history/mod.rs` (yeni modüller)
- **Dokunma:** `db/`, `commands/`, `services/`, `lib.rs`, `migrations/`, `src/`. R1 dosyalarının imzaları değişmez; eksik görürsen dur ve raporla.

**Tüketir:** R1'in bütün "Üretir" öğeleri. Ayrıca:
- `validation::check_teacher_totals(teacher_id, teacher_name, capacity, awarded_total, hours_by_day: &BTreeMap<i64,i64>, is_forced) -> Vec<Violation>`
- `scheduling::Block::{from_start, overlaps, exceeds_day_end, cells}`
- `models::Company::round_trip_distance_km()`

**Üretir:**

```rust
// impact.rs — hepsi Serialize, #[serde(rename_all = "camelCase")]; alanlar spec §8'e birebir uyar
pub struct ImpactSummary { pub effective_date: NaiveDate, pub is_planning: bool, pub shadowed_until: Option<NaiveDate>,
    pub primary: Vec<ImpactLine>, pub automatic: Vec<ImpactLine>, pub warnings: Vec<ImpactWarning>, pub notices: Vec<ImpactNotice> }
pub struct ImpactLine { pub kind: String, pub stream: String, pub subject_id: i64, pub subject_label: String, pub effective_date: NaiveDate, pub before: Option<String>, pub after: Option<String> }
pub enum WarningCode { LockedAboveCap, CapacityExceeded, DailyCapExceeded, BlockOutsideFreeSlots, CapUnknown }   // camelCase serileşir
pub struct ImpactWarning { pub code: WarningCode, pub message: String, pub subject_label: String, pub from_date: NaiveDate, pub to_date: Option<NaiveDate> }
pub enum NoticeCode { CapIncreased, ReducedBelowCap, NewCompanyNeedsSetup, FutureDated, Shadowed }
pub struct ImpactNotice { pub code: NoticeCode, pub message: String, pub subject_label: String, pub date: NaiveDate }

// policy.rs
pub fn cap_for(rules: &[HourRule], round_trip_km: Option<f64>, student_count: i64) -> Option<i64>;   // 0 öğrenci → Some(0); mesafe None veya kural yok → None
pub struct CompanyPolicyPlan { pub reductions: Vec<(NaiveDate, i64 /*cap*/, i64 /*n*/)>, pub coordinator_end: Option<NaiveDate>,
    pub warnings: Vec<ImpactWarning>, pub notices: Vec<ImpactNotice> }
pub fn plan_company_policies(label: &str, round_trip_km: Option<f64>, counts: &Timeline<i64>, hours: &Timeline<HoursState>,
    coordination: &Timeline<CoordinationState>, capped_history: &[NaiveDate], rules: &[HourRule], from: NaiveDate) -> CompanyPolicyPlan;
pub fn capacity_flags(teacher_label: &str, teacher_id: i64, load: &Timeline<TeacherLoad>, assigned_hours: &Timeline<i64>,
    hours_by_day: &dyn Fn(NaiveDate) -> BTreeMap<i64, i64>, statutory_cap: i64, from: NaiveDate) -> Vec<ImpactWarning>;
pub fn schedule_flags(teacher_label: &str, schedule: &Timeline<WeeklySchedule>, blocks: &[(String /*işletme*/, Timeline<Block>)], from: NaiveDate) -> Vec<ImpactWarning>;

// decide/mod.rs
pub struct ChangeRequest { pub term: String, pub effective_date: Option<NaiveDate>, pub document_date: Option<NaiveDate>, pub reason: String, pub command: ChangeCommand }
// Deserialize, #[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")] — spec §8'deki 16 tür, alan adları birebir
pub enum ChangeCommand { CreateStudent{..}, PlaceStudent{..}, TransferStudent{..}, StudentLeaves{..}, DeleteStudent{..},
    SetCompanyHours{..}, AssignCoordinators{..}, EndCoordination{..}, ClearCoordination, CreateTeacher{..}, SetTeacherLoad{..},
    SetTeacherSchedule{..}, CopySchedulesFromTerm{..}, DeleteTeacher{..}, Revoke{..}, Correct{ change_set_id: i64, replacement: Box<ChangeCommand> } }
pub enum TransferTarget { Existing { company_id: i64 }, New { company: NewCompany } }   // tag = "type": "existing" | "new"
pub struct ChangeSetFacts { pub id: i64, pub kind: String, pub effective_date: NaiveDate, pub revokes_change_set_id: Option<i64>, pub revoked_by: Option<i64> }
pub struct CompanyFacts { pub name: String, pub round_trip_km: Option<f64>, pub is_active: bool }
pub struct DecisionContext { pub today: NaiveDate, pub term: TermDates, pub rules: Vec<HourRule>, pub statutory_cap: i64, pub day_end_hour: i64,
    pub companies: BTreeMap<i64, CompanyFacts>, pub student_names: BTreeMap<i64, String>, pub teacher_names: BTreeMap<i64, String>,
    pub events: Vec<StoredEvent>, pub change_sets: BTreeMap<i64, ChangeSetFacts>, pub high_water: i64,
    /// Yerinde oluşturulan satırların id'leri (decide öncesinde create_in ile açılır). Anahtar: komuttaki yer.
    pub materialized: Materialized }
pub struct Materialized { pub company_id: Option<i64>, pub student_id: Option<i64>, pub teacher_id: Option<i64> }
pub struct PlannedEvent { pub stream: Stream, pub subject_id: i64, pub effective_date: NaiveDate, pub payload: EventPayload,
    pub caused_by: Option<usize> /* aynı karardaki olay indeksi */, pub revokes: Option<i64> }
pub struct NewChangeSet { pub term: String, pub kind: String, pub effective_date: NaiveDate, pub document_date: Option<NaiveDate>, pub reason: String, pub revokes_change_set_id: Option<i64> }
pub enum RowAction { DeleteStudent(i64), DeleteTeacher(i64), DeactivateCompany(i64) }
pub struct StreamKey { pub stream: Stream, pub subject_id: i64, pub term: String }
pub struct Decision { pub change_set: NewChangeSet, pub events: Vec<PlannedEvent>, pub impact: ImpactSummary, pub row_actions: Vec<RowAction>, pub touched: Vec<StreamKey> }
pub fn decide(ctx: &DecisionContext, req: &ChangeRequest) -> Result<Decision, Rejection>;

// audit.rs
pub struct AuditEvent { pub event_id: i64, pub stream: String, pub subject_id: i64, pub subject_label: String, pub kind: String,
    pub effective_date: NaiveDate, pub before: Option<String>, pub after: Option<String>, pub caused_by_event_id: Option<i64>, pub is_revoked: bool }
pub fn describe(events: &[StoredEvent], term_start: NaiveDate, change_set_ids: &[i64]) -> Vec<AuditEvent>;   // "önce" = (d, id) öncesindeki katlama durumu
```

**Karar kuralları** (spec §5.1–5.4'ün tamamı; özet):

- **Tarih.** `resolve_effective_date` çağrılır. `Correct` komutunda hem hedef kümenin tarihi hem yeni tarih pencere içinde olmalıdır.
- **Önceki durum.** Birincil olay `d⁻` anındaki katlama durumuna göre doğrulanır; tutmuyorsa `FactNotTrueAtDate`.
- **Sonraki olaylar.** Aday olay `(d, +∞)` konumuna yerleştirilir. `from_*` taşıyan sonraki olaylar tek tek denetlenir; ilk çelişkide `ConflictsWithLaterChange` döner ve çelişen küme id'si verilir.
- **Politika yürüyüşü.** Etkilenen her işletme için `plan_company_policies(from = d)` çağrılır. Her `reduction` bir `HoursCapped` olayına, `coordinator_end` bir `CoordinatorEndedByPolicy` olayına dönüşür; ikisinde de `caused_by` birincil olayın indeksidir.
  - 0 öğrenci kalınca: önce `HoursCapped{cap: 0}`, ardından koordinatör biter.
- **Saat girişi.**
  - Tavan sunucuda `cap_for` ile hesaplanır.
  - Saati artırıp yeni değeri tavanın **üstüne** çıkarmak `AboveCap` ile reddedilir.
  - Kilitli ve tavanın üstündeki bir satırda not değişikliği, kilit kaldırma ve saat düşürme serbesttir.
  - Fahri ziyarette saat 0'a zorlanır.
- **Koordinasyon.** Aynı öğretmenin, tarih aralıkları örtüşen başka bir işletme bloğuyla çakışma `BlockOverlap` ile reddedilir.
- **Etki satırları.** Öğretmen kapasitesi ve programı için `capacity_flags` ve `schedule_flags` sonuçları etkiye eklenir.
- **Geri alma** (`Revoke`):
  - `opening` ve `revoke` kümeleri ile tarihi pencere dışında kalan kümeler `NotRevocable`.
  - Hedefi çıkarınca sonraki bir kümenin `from_*` beklentisi bozuluyorsa `HasDependents`.
  - Aksi hâlde hedefin **her** canlı olayı için bir `Revoked` olayı yazılır; hedefin kendi zincir olayları da buna dahildir.
  - Politika, en erken tarihten yeniden yürütülür.
  - Nakil yerinde oluşturulmuş bir işletmeye yapılmışsa ve o işletmeye başka referans yoksa `RowAction::DeactivateCompany`.
- **`Correct`.** Tek bir karar, geri alma olaylarını ve yeni olayları **birlikte** üretir. Kümenin `kind` alanı `correct` olur ve `revokes_change_set_id` alanı dolar.
- **Silme** (`DeleteStudent`, `DeleteTeacher`):
  - Açılış dışında olayı olan özne `HasHistory`.
  - Yalnız açılış olayı olan öğrenci silinebilir; önce olayları geri alınır, sonra `RowAction` uygulanır.
- **Planlamaya özel işler.** `ClearCoordination` ve `HoursCleared` yalnız planlama evresinde yapılır; dönem başladıysa `PlanningOnly`.
- **`touched`.** Her olayın `(stream, subject, term)` anahtarı `touched` içinde tekil olarak yer alır.

**Adımlar:**

- [ ] **1. Önce testler** (saf, DB yok). Her karar için bir test yaz; düştüklerini gör.
  - **`policy`:**
    - `leave_lowers_unlocked_hours_to_new_cap_on_same_date`
    - `locked_row_only_warns`
    - `zero_students_caps_to_zero_and_ends_coordinator`
    - `cap_increase_is_notice_only`
    - `reduced_below_cap_notice_after_other_cause_revoked`
    - `walk_caps_later_manual_raise_at_its_own_date`
    - `other_extra_hours_rise_flags_capacity_without_drop` (24/10/4 → 10)
    - `schedule_change_flags_blocks_outside_free_slots`
    - `cap_unknown_warns_when_distance_missing`
  - **`decide`:**
    - `transfer_to_existing_company_emits_transfer_and_cap`
    - `transfer_to_new_company_uses_materialized_id_and_notices_setup`
    - `transfer_rejected_when_student_not_at_from_company_on_date`
    - `transfer_rejected_when_later_leave_expects_old_company` (çelişen id doğrulanır)
    - `previous_month_date_rejected_with_suggestion`
    - `manual_raise_above_cap_rejected`
    - `locked_above_cap_row_can_change_notes_and_unlock`
    - `honorary_forces_zero`
    - `coordinator_block_overlap_rejected_only_when_dates_overlap`
    - `revoke_rejected_when_later_set_depends_on_it`
    - `opening_revoke_and_previous_month_sets_are_not_revocable`
    - `revoke_takes_own_cascade_with_it`
    - `correct_is_one_change_set_with_revokes_and_new_events`
    - `revoking_transfer_to_created_company_deactivates_it`
    - `clear_coordination_only_while_planning`
    - `delete_student_with_history_rejected`
  - **Kasım senaryosu (tek test):**
    1. Dönem 2026-09-15'te başlar. S öğrencisi A işletmesinde; A'da 3 öğrenci var, tavan 6, takdir 6, koordinatör T1 Pazartesi 3. saat.
    2. 2026-11-05: T1'in `other_extra_hours` değeri 0 → 6.
    3. 2026-11-10 today iken S, yeni işletme C'ye nakledilir, yürürlük 2026-11-03.
    4. 2026-11-12 today iken düzeltme yapılır: nakil aslında mevcut işletme B'ye.

    Beklenenler:
    - Ekim durumu hiçbir adımda değişmez.
    - Düzeltme tek kümedir.
    - C işletmesi pasifleşir.
    - A'nın saati, B'ye naklin tavanına göre doğru kalır.
    - Hiçbir aralık çakışmaz.
  - **`audit`:** `describe_shows_before_and_after_from_fold`
- [ ] **2. Çalıştır:** `cargo test domain::history` → düşmeli.
- [ ] **3. Uygula.** Hiçbir fonksiyon 50 satırı, hiçbir dosya 800 satırı geçmemeli. Gerekirse yardımcıları alt modüllere böl.
- [ ] **4. Tam kapı:** `cargo test && cargo check` yeşil.
- [ ] **5. Rapor.** R1'deki maddelere ek olarak: `decide` hangi `ChangeCommand` türlerini tam kapsıyor, hangilerini kapsamıyor.

**Orkestratör doğrulaması:**
- Kasım senaryo testini oku; beklentilerin gerçekten doğrulandığını gör. `assert!(true)` benzeri boş doğrulama olmamalı.
- `rg "unwrap\(\)|expect\(" src-tauri/src/domain/history --glob '!**/tests*'` ile üretim kodunu tara.

**Commit:** `feat: decide changes and their cascade as pure functions`

---

## Görev R3 — Kalıcılık: 0006 göçü, günlük, projeksiyon

**Ajan:** `mesnet-rust` (R2'den sonra)

**Dosyalar:**
- **Oluştur:**
  - `src-tauri/migrations/0006_history.sql`
  - `src-tauri/src/db/{terms.rs, change_log.rs, projection.rs, history_context.rs}`
- **Değiştir:**
  - `src-tauri/src/db/mod.rs`: modüller eklenir; `init_pool_creates_schema…` testindeki tablo listesine yeni tablolar girer.
  - `src-tauri/src/db/companies.rs`: yalnız `create_in` ve `set_active_in` eklenir; mevcut fonksiyonlar değişmez.
  - `src-tauri/src/db/students.rs` ve `src-tauri/src/db/teachers.rs`: yalnız `create_in` ve `remove_in` eklenir.
- **Dokunma:** `commands/`, `services/`, `lib.rs`, `domain/` (R1 ve R2'nin imzaları sabittir), `src/`.

**Şema:** Spec §4.1 birebir uygulanır.
- Tablolar: `terms`, `change_sets` (`document_date` dahil), `change_events`.
- Değişmezlik tetikleyicileri.
- Beş projeksiyon: hepsinde `source_event_id`, açık satır için kısmi unique index, `UNIQUE(özne, term, valid_from)` ve `valid_to > valid_from` kontrolü.
- Her projeksiyonda **çakışma yedeği** olarak bir `BEFORE INSERT` tetikleyicisi: aynı özne ve dönemde tarih aralığı örtüşen satır varsa `RAISE(ABORT, 'Tarih aralıkları çakışıyor')`.
- `ALTER TABLE companies ADD COLUMN is_active INTEGER NOT NULL DEFAULT 1`.
- `settings` tablosuna `operator_name` eklenir.
- Tohum: spec §9 adım 1–5. Açılış olaylarının JSON biçimi, R1'deki `EventPayload::encode` çıktısıyla **birebir aynı** olmalıdır. Bunu bir test doğrular.

**Üretir:**

```rust
// db/terms.rs
pub async fn get_in(conn: &mut SqliteConnection, term: &str) -> AppResult<TermDates>;
pub async fn list(pool: &SqlitePool) -> AppResult<Vec<TermDates>>;
pub async fn insert_in(conn: &mut SqliteConnection, dates: &TermDates) -> AppResult<()>;
pub async fn update_dates(pool: &SqlitePool, term: &str, start: NaiveDate, end: NaiveDate, confirm: bool, today: NaiveDate) -> AppResult<TermDates>;   // spec §5.1 kuralları; kabulde aynı tx'te projection::rebuild_term

// db/change_log.rs
pub async fn append_change_set(conn: &mut SqliteConnection, cs: &NewChangeSet, actor: &str, impact_json: &str) -> AppResult<i64>;
pub async fn append_events(conn: &mut SqliteConnection, change_set_id: i64, term: &str, events: &[PlannedEvent]) -> AppResult<Vec<i64>>;   // caused_by indeksi → gerçek id
pub async fn load_term_events(conn: &mut SqliteConnection, term: &str) -> AppResult<Vec<StoredEvent>>;   // is_opening = kümenin kind'ı 'opening'
pub async fn load_change_sets(conn: &mut SqliteConnection, term: &str) -> AppResult<BTreeMap<i64, ChangeSetFacts>>;
pub async fn high_water(conn: &mut SqliteConnection) -> AppResult<i64>;   // MAX(id), boşsa 0
pub fn recorded_at_now() -> String;

// db/projection.rs — beş tablonun TEK yazıcısı
pub async fn rebuild_stream(conn: &mut SqliteConnection, key: &StreamKey, term_start: NaiveDate) -> AppResult<()>;
pub async fn rebuild_streams(conn: &mut SqliteConnection, keys: &[StreamKey], term_start: NaiveDate) -> AppResult<()>;   // özne başına BİR kez
pub async fn rebuild_term(conn: &mut SqliteConnection, term: &str) -> AppResult<()>;
pub struct ProjectionDrift { pub table: String, pub subject_id: i64, pub detail: String }
pub async fn verify_term(conn: &mut SqliteConnection, term: &str) -> AppResult<Vec<ProjectionDrift>>;   // replay ile tablo içeriği karşılaştırılır; ayrıca çakışma sorgusu

// db/history_context.rs
pub async fn load(conn: &mut SqliteConnection, term: &str, today: NaiveDate, materialized: Materialized) -> AppResult<DecisionContext>;
```

**Adımlar:**

- [ ] **1. Önce testler:**
  - `migration_0006_seeds_opening_events_from_a_0005_database`: `0001`–`0005` göçlerini dosya adıyla uygula; bunun için `teaching_load.rs` testlerindeki yolu izle. Veri ekle, ardından `0006`'yı çalıştır. Beklenenler: sayılar tutar, `verify_term` boş döner, her yük decode edilir.
  - `opening_payload_json_equals_rust_encoding`
  - `malformed_legacy_term_gets_wide_dates`
  - `change_log_rejects_update_and_delete` (tetikleyici mesajı)
  - `date_check_rejects_garbage_and_unpadded_dates`
  - `projection_overlap_trigger_aborts`
  - `append_events_resolves_caused_by_indices`
  - `rebuild_stream_is_idempotent`
  - `update_dates_rejects_changes_to_a_past_month`
  - `update_dates_allows_moving_start_past_opening_events`
  - `update_dates_rejects_start_after_a_non_opening_event`
  - `create_in_uses_the_given_connection` (`max_connections(1)` havuzuyla; transaction açıkken çalışmalı)
- [ ] **2. Çalıştır, düştüğünü gör.**
- [ ] **3. Uygula.**
- [ ] **4. Tam kapı:** `cargo test && cargo check`. Mevcut 281 test yeşil kalır; yalnız tablo listesi testi genişler.
- [ ] **5. Rapor.**

**Orkestratör doğrulaması:**
- Kullanıcının veritabanının **kopyasını** al:
  `cp "$HOME/Library/Application Support/<tauri identifier>/mesnet-lite.db" scratchpad/copy.db`
  Yolu `tauri.conf.json` içindeki `identifier` alanından doğrula.
- Kopyaya `sqlx migrate` ile ya da bir test harness'i üzerinden `0006`'yı uygula.
- `verify_term` boş dönmeli; `SELECT COUNT(*)` sayıları eski tablolarla eşleşmeli.

**Commit:** `feat: store the change log and rebuild effective-dated projections`

---

## Görev R4 — Komut kapısı: change service, tarihçe ve dönem komutları

**Ajan:** `mesnet-rust` (R3'ten sonra)

**Dosyalar:**
- **Oluştur:**
  - `src-tauri/src/services/change_service.rs`
  - `src-tauri/src/commands/history_commands.rs`
  - `src-tauri/src/commands/term_commands.rs`
- **Değiştir:**
  - `src-tauri/src/services/mod.rs`
  - `src-tauri/src/commands/mod.rs`
  - `src-tauri/src/lib.rs`: yalnız `generate_handler!` listesine `preview_change`, `commit_change`, `list_history`, `get_subject_history`, `list_terms_with_dates` ve `update_term_dates` eklenir.
- **Dokunma:** mevcut komut dosyaları, `db/` (R3'ün imzaları sabittir), `domain/`, `src/`.

**Üretir:**

```rust
pub enum ChangeMode { Preview, Commit { expected_high_water: Option<i64> } }
// Serialize, tag = "status", rename_all = "camelCase", rename_all_fields = "camelCase" — spec §8
pub enum ChangeOutcome { Rejected { code: RejectionCode, reason: String, conflicting_change_set_ids: Vec<i64>, suggested_date: Option<NaiveDate> },
    Stale { message: String }, Preview { impact: ImpactSummary, high_water: i64 }, Committed { change_set_id: i64, impact: ImpactSummary } }
pub async fn execute_change(pool: &SqlitePool, req: ChangeRequest, mode: ChangeMode, today: NaiveDate) -> AppResult<ChangeOutcome>;
pub async fn execute_in(conn: &mut SqliteConnection, req: ChangeRequest, mode: ChangeMode, today: NaiveDate) -> AppResult<ChangeOutcome>;
```

- **Akış:** Spec §5, adım 1–7.
  - `Preview` ve `Rejected` her zaman **ROLLBACK** ile biter.
  - Yalnız `Committed` commit edilir.
  - `actor` değeri `settings.operator_name`'den, aynı bağlantı üzerinden okunur.
- **Komutlar** (`#[tauri::command]`, `State<AppState>`):
  - `preview_change(request)`
  - `commit_change(request, expected_high_water)`
  - `list_history(filter)`: spec §8'deki şekil. Filtreler `LIMIT`'ten **önce** uygulanır. `isRevocable` alanı, `decide` ile aynı kuraldan hesaplanır.
  - `get_subject_history(stream, subject_id, term)`
  - `list_terms_with_dates()`: `earliestAllowedDate` ve `defaultAsOf` alanlarını içerir.
  - `update_term_dates(term, start_date, end_date, confirm)`
- **Tarih ayrıştırma** sınırda yapılır: `terms::parse_date`. Geçersiz tarih `AppError::Validation` döner.

**Adımlar:**

- [ ] **1. Önce testler:**
  - `preview_writes_nothing` (tüm tablolarda satır sayısı aynı kalır)
  - `commit_is_atomic`: projeksiyonda zorlanmış bir FK hatası olunca change set de, yerinde oluşturulan işletme de kalmaz.
  - `stale_high_water_returns_stale`
  - `rejected_rolls_back`
  - `no_pool_use_inside_tx`: `max_connections(1)` havuzuyla yeni işletmeye nakil yapılır. Transaction içinde havuz kullanılırsa test zaman aşımıyla düşer.
  - `list_history_filters_before_limit`
  - Karar başına bir senaryo testi (D1 denetim, D2 ay penceresi, D3 zincir, D4 öğretmen, D6 sınır günleri).
  - **P6** (tohumlu LCG): 50 dizi × 30 rastgele komut çalıştırılır; reddedilenler de sayılır. Her commit'ten sonra `verify_term` boş dönmeli ve çakışma sorgusu 0 satır vermeli.
- [ ] **2–4.** Düşür, uygula, tam kapıyı çalıştır.
- [ ] **5. Rapor.** Yeni komutların Tauri'ye kayıtlı olduğunu `lib.rs` satır numarasıyla göster.

**Orkestratör doğrulaması:**
- P6'yı `--release` olmadan kendin çalıştır; süreyi not et.
- `ChangeOutcome` için `serde_json::to_string` çıktısını spec §8'le karşılaştır; bunu bir test zaten yapmalı.

**Commit:** `feat: route every change through one previewable, atomic command`

---

## Görev R5 — Tarih itibarıyla okuma ve mevcut yazıcıların kapıya bağlanması

Bu iş dosya bakımından ayrık dört alt göreve bölünür. **Sırayla** koşar (aynı crate).

Her alt görevde geçerli kurallar:
- Okuma komutları `as_of: Option<String>` alır. Değer yoksa `TermDates::default_as_of(today)` kullanılır. Yanıtlar `asOf` ve `isPlanning` alanlarını döndürür.
- Mevcut yazma komutları imzalarını korur, ama içleri `execute_change(Commit{None})` sarmalayıcısına döner. Buna ek olarak isteğe bağlı bir `effective_date: Option<String>` argümanı alırlar.
  - `Rejected` → `AppError::Validation(reason)`.
  - `Committed` → eski dönüş tipi.
- `lib.rs` dosyasına dokunulmaz.

### R5a — Öğrenciler ve içe aktarma

- **Dosyalar:**
  - `src-tauri/src/db/students.rs`: `list_by_term_as_of`, `count_by_company_as_of`.
  - `src-tauri/src/commands/student_commands.rs`
  - `src-tauri/src/services/import_apply.rs`: tek bir `BEGIN IMMEDIATE` içinde `create_in` ve `execute_in(PlaceStudent…)`.
  - `src-tauri/src/commands/import_commands.rs`
- **Sözleşme:**
  - Öğrenci DTO'sunda `companyId` adı kalır; değer projeksiyondan gelir. Yeni alan `placementFrom`.
  - Öğrenci silme `deleteStudent` komutuna gider.
- **Testler:**
  - `students_list_as_of_reads_placement_projection`
  - `import_is_one_transaction_and_places_via_events`
  - `update_student_company_change_goes_through_transfer`

### R5b — Saat ve atama

- **Dosyalar:**
  - `src-tauri/src/db/company_hours.rs`: `list_as_of`, `total_awarded_as_of`.
  - `src-tauri/src/db/assignments.rs`: `list_as_of`, `awarded_hours_by_teacher_as_of`, `awarded_hours_by_teacher_and_day_as_of`.
  - `src-tauri/src/commands/hours_commands.rs`
  - `src-tauri/src/commands/assignment_commands.rs`
- **Sözleşme:**
  - `HoursInput.maxHoursSnapshot` artık **yok sayılır**; tavan sunucuda hesaplanır.
  - `clear_term` ve öneriyi uygulama yalnız planlamada çalışır.
- **Kapasite:** Komutlardaki kopya kapasite türetmesi silinir; yerine `workload::teacher_capacity` gelir.
- **Testler:**
  - `hours_board_reads_as_of`
  - `assign_goes_through_change_service`
  - `apply_proposal_rejected_after_term_start`

### R5c — Öğretmen, müsaitlik, dönem ve ayarlar

- **Dosyalar:**
  - `src-tauri/src/db/teachers.rs`: `list_with_load_as_of`.
  - `src-tauri/src/db/availability.rs`: `list_all_as_of`.
  - `src-tauri/src/commands/teacher_commands.rs`
  - `src-tauri/src/commands/availability_commands.rs`
  - `src-tauri/src/commands/settings_commands.rs`: `create_term` bir `terms` satırı, açılış kümesi ve kopyalanan `load_set` olayları yazar. `known_terms` değeri `terms` tablosundan gelir.
  - `src-tauri/src/db/settings.rs`
  - `src-tauri/src/commands/company_commands.rs`: silme bekçisi, `hasHistory` mesajı. İşletme listesi varsayılan olarak `is_active = 0` olanları göstermez; `include_inactive: Option<bool>` argümanıyla gösterir. Yanıta `isActive` alanı eklenir.
- **Sözleşme:** `TeacherWithCapacity`, yük alanlarını o gün geçerli olan dönemden alır; yeni alan `loadValidFrom`.
- **Testler:**
  - `teacher_load_read_as_of`
  - `create_term_copies_last_load`
  - `delete_company_with_history_is_refused`

### R5d — Raporlar, dışa aktarma, Genel Bakış

- **Dosyalar:**
  - `src-tauri/src/services/pdf_report.rs` ve `src-tauri/src/services/excel_export.rs`: `load_context` fonksiyonu `as_of` alır.
  - `src-tauri/src/commands/report_commands.rs`
  - `src-tauri/src/commands/export_commands.rs`
  - `src-tauri/src/commands/dashboard_commands.rs`
- **Testler:**
  - `assignment_sheet_as_of_october_unchanged_by_november_transfer`
  - `dashboard_reads_as_of`

**Her alt görevin kapısı:** `cargo test && cargo check`. Değişen mevcut fixture'lar gerekçesiyle raporlanır.

**Orkestratör doğrulaması:** Her alt görevden sonra, o alt görevin dosyalarında şu arama yapılır:

```
rg "company_term_hours|FROM assignments|teacher_availability|students\.company_id|base_hours|other_extra_hours" <o alt görevin dosyaları>
```

Eski depolamaya kalan okuma ve yazmalar bulunur. İzin verilen tek şey: R6'ya bırakılan ve raporda listelenen satırlar.

**Commit** (her alt görev için ayrı):
- `feat: read students and imports through the placement history`
- `feat: route hours and coordinator changes through the change log`
- `feat: date teacher load and schedules`
- `feat: produce reports and the dashboard as of a date`

---

## Görev R6 — Temizlik: 0007 göçü ve eski depolamanın kaldırılması

**Ajan:** `mesnet-rust` (R5a–d ve V2, V3, V5 bittikten sonra)

**Dosyalar:**
- **Oluştur:** `src-tauri/migrations/0007_history_cleanup.sql` (spec §9).
- **Değiştir:**
  - `src-tauri/src/db/{students, company_hours, assignments, teachers, availability, mod}.rs`: eski SQL ve ölü fonksiyonlar silinir.
  - `src-tauri/src/domain/models.rs`: `Student.company_id` kalkar, `StudentAsOf` gelir. `Teacher`'ın yük alanları kalkar. `NewTeacher` yerine `NewTeacherProfile` + `TeacherLoad` gelir.
  - Kırılan her fixture düzeltilir.
- **Testler:**
  - `migration_0007_keeps_rows_and_drops_columns`: bundled 3.46 ile, `PRAGMA foreign_key_check` boş, `pragma_table_info` kaldırılan sütunları göstermez.
- **Kapı:**
  - `cargo test && cargo check`.
  - `rg 'company_term_hours|teacher_availability|FROM assignments|students\.company_id' src-tauri/src` yalnız göçlerde ve yorumlarda eşleşmeli.

**Orkestratör doğrulaması:**
- Gerçek veritabanının **kopyasına** `0006` + `0007` uygulanır.
- `verify_term` boş dönmeli.
- Öğrenci ve öğretmen sayıları aynı kalmalı.

**Commit:** `refactor: drop the pre-history tables now that everything reads the log`

---

## Görev V1 — Arayüz sözleşme katmanı

**Ajan:** `mesnet-vue` (R4'ün sözleşmesi dondurulunca; R5 ile paralel koşabilir)

**Dosyalar:**
- **Oluştur:**
  - `src/api/history.ts`: `previewChange`, `commitChange`, `listHistory`, `getSubjectHistory`.
  - `src/composables/useAsOfDate.ts`: varsayılan tarih, dönem aralığına sıkıştırılmış bugündür. Sidebar'da seçilir ve modül düzeyinde tek bir `ref` olarak paylaşılır.
  - `src/composables/useChange.ts`: önizle → `ImpactDialog` → onayla. `stale` durumunda otomatik yeniden önizler. `rejected` + `suggestedDate` durumunda "bu ayın 1'inden gir" seçeneğini sunar ve `documentDate` alanını doldurur.
- **Değiştir:**
  - `src/api/terms.ts`: `listTermsWithDates`, `updateTermDates`.
  - `src/types/models.ts`: spec §8'deki tipler birebir.
  - `src/i18n/labels.ts`: **bu işin tüm yeni etiketleri burada**, sonraki V görevleri buraya dokunmaz. İçerik:
    - olay türü etiketleri (spec §7),
    - `rejected.code`, `warning.code` ve `notice.code` başlıkları,
    - tarih alanı etiketleri ("Sözleşme başlangıç tarihi", "Sözleşme fesih tarihi", "Geçerlilik tarihi"),
    - `ImpactDialog` bölüm başlıkları,
    - Tarihçe ekranı metinleri.
- **Dokunma:** views, mevcut components, router, `src-tauri/`.
- **Testler (vitest):**
  - `useChange re-previews on stale`
  - `useChange offers first-of-month on previousMonthClosed`
  - `useAsOfDate clamps today to the term`
- **Kapı:** `npx vue-tsc --noEmit`, `npm run build`, `npx vitest run`.

## Görev V2 — Ortak tarih bileşenleri

**Ajan:** `mesnet-vue` (V1'den sonra)

- **Oluştur:**
  - `src/components/history/ImpactDialog.vue`: dört bölüm. `rejected` durumunda onay kapalıdır.
  - `src/components/history/EffectiveDateField.vue`:
    - `min = earliestAllowedDate`, `max = endDate`.
    - Etiket prop ile gelir.
    - Planlamada gizlenir.
  - `src/components/history/AsOfDatePicker.vue`
- **Dokunma:** V1 dosyaları, views, `labels.ts`.
- **Testler:**
  - `ImpactDialog renders four sections and disables commit on rejected`
  - `EffectiveDateField hides while planning`

## Görev V3 — Öğrenci nakli ve ayrılışı

**Ajan:** `mesnet-vue` (V2'den sonra; R5a birleştikten sonra canlı test edilir)

- **Oluştur:** `src/components/student/StudentChangeDialog.vue`
  - Nakil ya da ayrılış seçilir.
  - Hedef işletme mevcut listeden seçilir veya `CompanyFormDialog` ile yerinde oluşturulur. İstekte `to: {type:'new', company}` gönderilir.
  - Tarih etiketi olaya göre "Sözleşme başlangıç tarihi" ya da "Sözleşme fesih tarihi" olur.
- **Değiştir:**
  - `src/components/student/StudentFormDialog.vue`: işletme alanı yalnız yeni öğrencide görünür.
  - `src/views/StudentsView.vue`: satır menüsüne "Nakil / Ayrılış" eklenir.
- **Dokunma:** diğer views, V1 ve V2 dosyaları, `labels.ts`.

## Görev V4 — Öğretmen yükü ve müsaitlik

**Ajan:** `mesnet-vue` (V2'den sonra; V3 ile paralel)

- **Oluştur:** `src/components/teacher/TeacherLoadDialog.vue`
- **Değiştir:**
  - `src/components/teacher/TeacherFormDialog.vue`: yük alanları yalnız yeni öğretmende görünür.
  - `src/views/TeachersView.vue`
  - `src/views/AvailabilityView.vue`: dönem başladıysa kaydederken `EffectiveDateField` gösterilir.
- **Dokunma:** diğer views, V1 ve V2 dosyaları, `labels.ts`.

## Görev V5 — Tarihçe ekranı ve gezinme

**Ajan:** `mesnet-vue` (V2'den sonra; V3 ve V4 ile paralel)

- **Oluştur:**
  - `src/views/HistoryView.vue`
  - `src/components/history/HistoryEntryCard.vue`:
    - `caused_by` ilişkisi iç içe gösterilir.
    - Geri alınanlar üstü çizilir.
    - "Geri al" ve "Düzelt" yalnız `isRevocable` olan kayıtlarda görünür.
    - "Düzelt", ilgili değişiklik penceresini eski değerlerle açar ve `correct` komutu gönderir.
- **Değiştir:**
  - `src/router/index.ts`
  - `src/components/layout/AppSidebar.vue`: Tarihçe bağlantısı ve `AsOfDatePicker`.
- **Dokunma:** diğer views, V1 ve V2 dosyaları, `labels.ts`.
- **Test:** `HistoryEntryCard nests caused events and strikes revoked`

## Görev V6 — Dağıtım, saat, pano, içe aktarma, dönem ekranları

**Ajan:** `mesnet-vue` (V3, V4 ve V5 bittikten sonra; R5b–d birleştikten sonra)

- **Değiştir:**
  - `src/views/AllocationView.vue`: **bölme yok**, net büyüme en fazla 20 satır. Okumalarda `asOf` gönderilir. `asOf` bugünden farklıysa ekran salt okunurdur. Dönem başladıysa koordinatör değişikliği `useChange` üzerinden yapılır.
  - `src/views/CompanyHoursView.vue`: `maxHoursSnapshot` gönderilmez; tarih ve etki penceresi eklenir.
  - `src/views/DashboardView.vue`
  - `src/views/ImportExportView.vue`: raporlara `asOf`, içe aktarmaya `effectiveDate`.
  - `src/views/TermManagementView.vue`: dönem başı ve sonu, "onaylandı" işareti, uyarı başlığı.
  - `src/views/SettingsView.vue`: `operator_name`.
- **Dokunma:** V1–V5 dosyaları, `labels.ts`. Eksik bir etiket olursa dur ve raporla; orkestratör V1 kapsamında ekler.

**Her V görevi için:**
- **Kapı:** `npx vue-tsc --noEmit && npm run build && npx vitest run`.
- **Orkestratör doğrulaması:**
  - `rg "'[A-ZÇĞİÖŞÜ][a-zçğıöşü]+ " src/views src/components` ile şablonda gömülü Türkçe metin aranır; `labels.ts` dışında metin kalmamalı.
  - Uygulama çalıştırılıp akış canlı denenir: nakil → etki → onay → tarihçe.

---

## Sıra ve paralellik

```
R1 → R2 → R3 → R4 ─┬→ R5a → R5b → R5c → R5d ─┐
                   └→ V1 → V2 ─┬→ V3 ─┐        ├→ V6 → R6 → son doğrulama
                               ├→ V4 ─┤        │
                               └→ V5 ─┘────────┘
```

**Son doğrulama:**
- Gerçek veritabanının kopyasında `0006` + `0007` uygulanır, `verify_term` çalıştırılır.
- Uygulama kopya veritabanıyla açılıp spec §2'deki her karar elle denenir.
- Kod incelemesi: `ecc:rust-reviewer` ve `ecc:vue-reviewer`.
- Kullanıcıya, gerçek veritabanının yedeği alınmadan dalın çalıştırılmaması gerektiği söylenir.
