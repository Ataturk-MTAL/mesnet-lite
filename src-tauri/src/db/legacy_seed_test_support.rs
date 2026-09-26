//! SADECE TEST modülü: eski `company_hours::upsert/save_many` ve
//! `assignments::assign` (bu iş onları dondurdu — bkz. `company_hours.rs`/
//! `assignments.rs` başlığı) yerine, dağınık test fixture'larının (PDF/Excel
//! dışa aktarım, komisyon tutanağı, pano testleri) ihtiyaç duyduğu "şu
//! işletmenin şu kadar saati/koordinatörü var" sahnesini kurar.
//!
//! Olaylar tarihçenin GERÇEK kalıcılaştırma yolundan (`change_sets` +
//! `change_events` + `projection::rebuild_streams`) yazılır ki projeksiyonu
//! okuyan her kod (PDF/Excel, pano, tarihçe) tutarlı görsün. `decide()`
//! BİLEREK atlanır: bu bir kullanıcı isteği değil, sabitlenmiş bir test
//! sahnesidir ve tavan/havuz/çakışma denetimlerine (eski yazıcılar da tavan
//! dışında bunlara tabi değildi) tabi tutulması test amacını bozar — ör.
//! `pdf_report.rs`in bazı sahneleri öğrenci OLMADAN saat kaydı kurar
//! (0 öğrenci ⇒ `cap_for` sunucu tavanını 0'a düşürür).

use chrono::NaiveDate;
use sqlx::SqlitePool;

use crate::db::{change_log, projection, terms};
use crate::domain::history::decide::{NewChangeSet, PlannedEvent, StreamKey};
use crate::domain::history::events::{CoordinationState, EventPayload, HoursState, Labels, Stream};

/// Tek bir olayı dönem başında (`terms.start_date`) yazar ve ilgili
/// projeksiyonu yeniden kurar. Panik testte kabul edilebilir (`unwrap`),
/// üretim kodunda değil — bu dosya yalnız `#[cfg(test)]` altında derlenir.
async fn seed_event(pool: &SqlitePool, term: &str, stream: Stream, subject_id: i64, payload: EventPayload) {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let term_start: NaiveDate = terms::get_in(&mut tx, term).await.unwrap().start;

    let change_set = NewChangeSet {
        term: term.to_string(),
        kind: "test_seed".to_string(),
        effective_date: term_start,
        document_date: None,
        reason: "test tohumu".to_string(),
        revokes_change_set_id: None,
    };
    let change_set_id = change_log::append_change_set(&mut tx, &change_set, "test", "{}").await.unwrap();

    let event = PlannedEvent { stream, subject_id, effective_date: term_start, payload, caused_by: None, revokes: None };
    change_log::append_events(&mut tx, change_set_id, term, &[event]).await.unwrap();

    let key = StreamKey { stream, subject_id, term: term.to_string() };
    projection::rebuild_streams(&mut tx, &[key], term_start).await.unwrap();

    tx.commit().await.unwrap();
}

/// Eski `company_hours::upsert` gibi: fahri işaretliyse saat 0'a zorlanır.
pub(crate) async fn seed_hours(pool: &SqlitePool, term: &str, company_id: i64, awarded_hours: i64, is_honorary: bool) {
    let awarded = if is_honorary { 0 } else { awarded_hours };
    let state = HoursState { awarded_hours: awarded, max_hours_snapshot: awarded.max(1), is_honorary, is_locked: false, notes: String::new() };
    let payload = EventPayload::HoursSet { state, previous_awarded: None, labels: Labels(Default::default()) };
    seed_event(pool, term, Stream::CompanyHours, company_id, payload).await;
}

/// Eski `assignments::assign` gibi: bir işletmeyi bir öğretmenin haftalık
/// programına yerleştirir. Çakışma denetimi burada YOKTUR (bkz. modül başı).
pub(crate) async fn seed_coordinator(
    pool: &SqlitePool,
    term: &str,
    company_id: i64,
    teacher_id: i64,
    visit_day: i64,
    visit_hour: i64,
    is_forced: bool,
    force_reason: Option<String>,
) {
    let state = CoordinationState { teacher_id, visit_day, visit_hour, is_forced, force_reason };
    let payload = EventPayload::CoordinatorAssigned { state, from_teacher_id: None, labels: Labels(Default::default()) };
    seed_event(pool, term, Stream::Coordination, company_id, payload).await;
}
