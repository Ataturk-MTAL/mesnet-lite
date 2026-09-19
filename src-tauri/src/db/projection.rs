//! Beş tarih aralıklı projeksiyonun TEK yazıcısı (spec §4.1, §5 adım 6).
//!
//! `student_placements`, `company_hour_periods`, `coordination_periods`,
//! `teacher_load_periods`, `teacher_schedule_periods` — hiçbiri buradan
//! başka bir yerden YAZILMAZ. Rebuild ÖZNE BAŞINA BİR KEZ yapılır: canlı
//! olaylar `(effective_date, id)` sırasıyla katlanır, o öznenin satırları
//! silinir ve yeniden yazılır (`db/change_log.rs`'in `mod.rs` boyutunu
//! aşmaması için katlama/yazma `sync`'te, doğrulama `verify`'de yaşar —
//! aynı dosya, aynı modül yolu, yalnız iç bölme).

mod sync;
mod verify;

use std::collections::BTreeSet;

use chrono::NaiveDate;
use sqlx::SqliteConnection;

use crate::domain::history::decide::StreamKey;
use crate::domain::history::timeline::order_events;
use crate::error::AppResult;

use super::{change_log, terms};

pub use verify::ProjectionDrift;

/// Bir akış+öznenin canlı olaylarını katlar, satırlarını siler ve yeniden
/// yazar. `term_start` çağıran tarafından verilir (`terms::get_in(term).start`)
/// — açılış olayları katlamada buraya sabitlenir (spec §5.1).
pub async fn rebuild_stream(conn: &mut SqliteConnection, key: &StreamKey, term_start: NaiveDate) -> AppResult<()> {
    let events = change_log::load_stream_events(conn, &key.term, key.stream, key.subject_id).await?;
    let ordered = order_events(&events, term_start);
    sync::sync_stream(conn, key, &ordered).await
}

/// Aynı özne birden çok kez geçse bile rebuild yalnız BİR KEZ çalışır
/// (spec §5 adım 6, "Rebuild olay başına değil, özne başına bir kez").
pub async fn rebuild_streams(conn: &mut SqliteConnection, keys: &[StreamKey], term_start: NaiveDate) -> AppResult<()> {
    let unique: BTreeSet<StreamKey> = keys.iter().cloned().collect();
    for key in &unique {
        rebuild_stream(conn, key, term_start).await?;
    }
    Ok(())
}

/// Bir dönemdeki HER akış+öznenin projeksiyonunu, günlükteki tüm
/// olaylardan yeniden kurar. `update_dates` kabul edilen bir tarih
/// değişikliğinden sonra bunu çağırır (spec §5.1).
pub async fn rebuild_term(conn: &mut SqliteConnection, term: &str) -> AppResult<()> {
    let term_start = terms::get_in(conn, term).await?.start;
    let events = change_log::load_term_events(conn, term).await?;
    let keys: Vec<StreamKey> = events
        .iter()
        .map(|e| StreamKey { stream: e.stream, subject_id: e.subject_id, term: term.to_string() })
        .collect();
    rebuild_streams(conn, &keys, term_start).await
}

/// Her akış+özne için replay sonucunu (yeniden katlanmış hâl) tablodaki
/// içerikle karşılaştırır ve çakışma sorgusunu çalıştırır. Boş dönüş
/// "projeksiyon günlükle tutarlı" demektir (spec §10, DB testleri).
pub async fn verify_term(conn: &mut SqliteConnection, term: &str) -> AppResult<Vec<ProjectionDrift>> {
    let term_start = terms::get_in(conn, term).await?.start;
    let events = change_log::load_term_events(conn, term).await?;
    let keys: BTreeSet<StreamKey> = events
        .iter()
        .map(|e| StreamKey { stream: e.stream, subject_id: e.subject_id, term: term.to_string() })
        .collect();

    let mut drifts = Vec::new();
    for key in &keys {
        drifts.extend(verify::verify_stream(conn, key, term_start).await?);
    }
    drifts.extend(verify::overlap_drifts(conn, term).await?);
    Ok(drifts)
}
