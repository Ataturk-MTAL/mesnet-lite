//! Rebuild'i "test kâhini" olarak kullanan doğrulama (spec §3, §10):
//! replay ile tablodaki satırları karşılaştırır, hiçbir şey YAZMAZ. Ayrıca
//! her tablo için çakışma sorgusunu çalıştırır — `0006_history.sql`'deki
//! `BEFORE INSERT` tetikleyicisi bunu zaten engeller, bu yalnız bağımsız
//! bir ikinci doğrulamadır.

use chrono::NaiveDate;
use sqlx::SqliteConnection;

use crate::db::change_log;
use crate::domain::history::apply::{apply_coordination, apply_hours, apply_load, apply_placement, apply_schedule};
use crate::domain::history::decide::StreamKey;
use crate::domain::history::events::Stream;
use crate::domain::history::timeline::{fold_intervals, order_events, TimedEvent};
use crate::error::AppResult;

use super::sync::enum_column;

/// Bir akış+öznenin projeksiyonu ile günlükten katlanan durumu arasındaki
/// tutarsızlık. `db/history_context.rs` ve komut katmanı bunu kullanmaz;
/// yalnız operatör doğrulaması ve testler içindir.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectionDrift {
    pub table: String,
    pub subject_id: i64,
    pub detail: String,
}

fn drift(table: &str, subject_id: i64, detail: String) -> ProjectionDrift {
    ProjectionDrift { table: table.to_string(), subject_id, detail }
}

pub(super) async fn verify_stream(
    conn: &mut SqliteConnection,
    key: &StreamKey,
    term_start: NaiveDate,
) -> AppResult<Vec<ProjectionDrift>> {
    let events = change_log::load_stream_events(conn, &key.term, key.stream, key.subject_id).await?;
    let ordered = order_events(&events, term_start);

    match key.stream {
        Stream::Placement => verify_placement(conn, key, &ordered).await,
        Stream::CompanyHours => verify_company_hours(conn, key, &ordered).await,
        Stream::Coordination => verify_coordination(conn, key, &ordered).await,
        Stream::TeacherLoad => verify_teacher_load(conn, key, &ordered).await,
        Stream::TeacherSchedule => verify_teacher_schedule(conn, key, &ordered).await,
    }
}

/// Beklenen ve gerçek aralık sayısı farklıysa tek bir sapma yeterlidir;
/// aynıysa sırayla karşılaştırılır (aynı sırada olmaları gerekir, çünkü
/// ikisi de `valid_from` artan sırayla üretilir/sorgulanır).
fn count_mismatch(table: &str, subject_id: i64, expected: usize, actual: usize) -> Option<ProjectionDrift> {
    if expected == actual {
        None
    } else {
        Some(drift(table, subject_id, format!("beklenen {expected} aralık, tabloda {actual} satır var")))
    }
}

#[derive(sqlx::FromRow)]
struct PlacementRow {
    valid_from: NaiveDate,
    valid_to: Option<NaiveDate>,
    company_id: i64,
    source_event_id: i64,
}

async fn verify_placement(conn: &mut SqliteConnection, key: &StreamKey, ordered: &[TimedEvent]) -> AppResult<Vec<ProjectionDrift>> {
    let expected = fold_intervals(ordered, apply_placement);
    let actual: Vec<PlacementRow> = sqlx::query_as(
        "SELECT valid_from, valid_to, company_id, source_event_id FROM student_placements
         WHERE student_id = ?1 AND term = ?2 ORDER BY valid_from",
    )
    .bind(key.subject_id)
    .bind(&key.term)
    .fetch_all(&mut *conn)
    .await?;

    let mut drifts: Vec<ProjectionDrift> = count_mismatch("student_placements", key.subject_id, expected.len(), actual.len()).into_iter().collect();
    if !drifts.is_empty() {
        return Ok(drifts);
    }
    for (exp, act) in expected.iter().zip(actual.iter()) {
        let matches = exp.valid_from == act.valid_from
            && exp.valid_to == act.valid_to
            && exp.state == act.company_id
            && exp.source_event_id == act.source_event_id;
        if !matches {
            drifts.push(drift("student_placements", key.subject_id, format!("beklenen {exp:?}, tabloda işletme={} kaynak={}", act.company_id, act.source_event_id)));
        }
    }
    Ok(drifts)
}

#[derive(sqlx::FromRow)]
struct CompanyHoursRow {
    valid_from: NaiveDate,
    valid_to: Option<NaiveDate>,
    awarded_hours: i64,
    max_hours_snapshot: i64,
    is_honorary: i64,
    is_locked: i64,
    notes: String,
    source_event_id: i64,
}

async fn verify_company_hours(conn: &mut SqliteConnection, key: &StreamKey, ordered: &[TimedEvent]) -> AppResult<Vec<ProjectionDrift>> {
    let expected = fold_intervals(ordered, apply_hours);
    let actual: Vec<CompanyHoursRow> = sqlx::query_as(
        "SELECT valid_from, valid_to, awarded_hours, max_hours_snapshot, is_honorary, is_locked, notes, source_event_id
         FROM company_hour_periods WHERE company_id = ?1 AND term = ?2 ORDER BY valid_from",
    )
    .bind(key.subject_id)
    .bind(&key.term)
    .fetch_all(&mut *conn)
    .await?;

    let mut drifts: Vec<ProjectionDrift> = count_mismatch("company_hour_periods", key.subject_id, expected.len(), actual.len()).into_iter().collect();
    if !drifts.is_empty() {
        return Ok(drifts);
    }
    for (exp, act) in expected.iter().zip(actual.iter()) {
        let matches = exp.valid_from == act.valid_from
            && exp.valid_to == act.valid_to
            && exp.state.awarded_hours == act.awarded_hours
            && exp.state.max_hours_snapshot == act.max_hours_snapshot
            && exp.state.is_honorary == (act.is_honorary != 0)
            && exp.state.is_locked == (act.is_locked != 0)
            && exp.state.notes == act.notes
            && exp.source_event_id == act.source_event_id;
        if !matches {
            drifts.push(drift("company_hour_periods", key.subject_id, format!("beklenen {:?}, tabloda kaynak={}", exp.state, act.source_event_id)));
        }
    }
    Ok(drifts)
}

#[derive(sqlx::FromRow)]
struct CoordinationRow {
    valid_from: NaiveDate,
    valid_to: Option<NaiveDate>,
    teacher_id: i64,
    visit_day: i64,
    visit_hour: i64,
    is_forced: i64,
    force_reason: Option<String>,
    source_event_id: i64,
}

async fn verify_coordination(conn: &mut SqliteConnection, key: &StreamKey, ordered: &[TimedEvent]) -> AppResult<Vec<ProjectionDrift>> {
    let expected = fold_intervals(ordered, apply_coordination);
    let actual: Vec<CoordinationRow> = sqlx::query_as(
        "SELECT valid_from, valid_to, teacher_id, visit_day, visit_hour, is_forced, force_reason, source_event_id
         FROM coordination_periods WHERE company_id = ?1 AND term = ?2 ORDER BY valid_from",
    )
    .bind(key.subject_id)
    .bind(&key.term)
    .fetch_all(&mut *conn)
    .await?;

    let mut drifts: Vec<ProjectionDrift> = count_mismatch("coordination_periods", key.subject_id, expected.len(), actual.len()).into_iter().collect();
    if !drifts.is_empty() {
        return Ok(drifts);
    }
    for (exp, act) in expected.iter().zip(actual.iter()) {
        let matches = exp.valid_from == act.valid_from
            && exp.valid_to == act.valid_to
            && exp.state.teacher_id == act.teacher_id
            && exp.state.visit_day == act.visit_day
            && exp.state.visit_hour == act.visit_hour
            && exp.state.is_forced == (act.is_forced != 0)
            && exp.state.force_reason == act.force_reason
            && exp.source_event_id == act.source_event_id;
        if !matches {
            drifts.push(drift("coordination_periods", key.subject_id, format!("beklenen {:?}, tabloda kaynak={}", exp.state, act.source_event_id)));
        }
    }
    Ok(drifts)
}

#[derive(sqlx::FromRow)]
struct TeacherLoadRow {
    valid_from: NaiveDate,
    valid_to: Option<NaiveDate>,
    base_hours: i64,
    max_extra_hours: i64,
    other_extra_hours: i64,
    chief_type: String,
    employment_type: String,
    source_event_id: i64,
}

async fn verify_teacher_load(conn: &mut SqliteConnection, key: &StreamKey, ordered: &[TimedEvent]) -> AppResult<Vec<ProjectionDrift>> {
    let expected = fold_intervals(ordered, apply_load);
    let actual: Vec<TeacherLoadRow> = sqlx::query_as(
        "SELECT valid_from, valid_to, base_hours, max_extra_hours, other_extra_hours, chief_type, employment_type, source_event_id
         FROM teacher_load_periods WHERE teacher_id = ?1 AND term = ?2 ORDER BY valid_from",
    )
    .bind(key.subject_id)
    .bind(&key.term)
    .fetch_all(&mut *conn)
    .await?;

    let mut drifts: Vec<ProjectionDrift> = count_mismatch("teacher_load_periods", key.subject_id, expected.len(), actual.len()).into_iter().collect();
    if !drifts.is_empty() {
        return Ok(drifts);
    }
    for (exp, act) in expected.iter().zip(actual.iter()) {
        let matches = exp.valid_from == act.valid_from
            && exp.valid_to == act.valid_to
            && exp.state.base_hours == act.base_hours
            && exp.state.max_extra_hours == act.max_extra_hours
            && exp.state.other_extra_hours == act.other_extra_hours
            && enum_column(exp.state.chief_type)? == act.chief_type
            && enum_column(exp.state.employment_type)? == act.employment_type
            && exp.source_event_id == act.source_event_id;
        if !matches {
            drifts.push(drift("teacher_load_periods", key.subject_id, format!("beklenen {:?}, tabloda kaynak={}", exp.state, act.source_event_id)));
        }
    }
    Ok(drifts)
}

#[derive(sqlx::FromRow)]
struct TeacherScheduleRow {
    valid_from: NaiveDate,
    valid_to: Option<NaiveDate>,
    slots_json: String,
    source_event_id: i64,
}

async fn verify_teacher_schedule(conn: &mut SqliteConnection, key: &StreamKey, ordered: &[TimedEvent]) -> AppResult<Vec<ProjectionDrift>> {
    let expected = fold_intervals(ordered, apply_schedule);
    let actual: Vec<TeacherScheduleRow> = sqlx::query_as(
        "SELECT valid_from, valid_to, slots_json, source_event_id
         FROM teacher_schedule_periods WHERE teacher_id = ?1 AND term = ?2 ORDER BY valid_from",
    )
    .bind(key.subject_id)
    .bind(&key.term)
    .fetch_all(&mut *conn)
    .await?;

    let mut drifts: Vec<ProjectionDrift> = count_mismatch("teacher_schedule_periods", key.subject_id, expected.len(), actual.len()).into_iter().collect();
    if !drifts.is_empty() {
        return Ok(drifts);
    }
    for (exp, act) in expected.iter().zip(actual.iter()) {
        let actual_schedule: Result<crate::domain::history::events::WeeklySchedule, _> = serde_json::from_str(&act.slots_json);
        let matches = exp.valid_from == act.valid_from
            && exp.valid_to == act.valid_to
            && actual_schedule.as_ref().map(|s| s == &exp.state).unwrap_or(false)
            && exp.source_event_id == act.source_event_id;
        if !matches {
            drifts.push(drift("teacher_schedule_periods", key.subject_id, format!("beklenen {:?}, tabloda {} kaynak={}", exp.state, act.slots_json, act.source_event_id)));
        }
    }
    Ok(drifts)
}

/// Her projeksiyon tablosu için bağımsız bir çakışma taraması. Aynı özne +
/// dönemde iki satırın tarih aralığı örtüşüyorsa ([valid_from, valid_to)
/// yarı açık) ilgili özne id'si döner. `0006_history.sql`'deki `BEFORE
/// INSERT` tetikleyicisi bunu zaten INSERT anında engeller; bu sorgu
/// yalnız ikinci, bağımsız bir kanıttır.
pub(super) async fn overlap_drifts(conn: &mut SqliteConnection, term: &str) -> AppResult<Vec<ProjectionDrift>> {
    let mut drifts = Vec::new();
    for (table, subject_col) in [
        ("student_placements", "student_id"),
        ("company_hour_periods", "company_id"),
        ("coordination_periods", "company_id"),
        ("teacher_load_periods", "teacher_id"),
        ("teacher_schedule_periods", "teacher_id"),
    ] {
        let sql = format!(
            "SELECT DISTINCT a.{subject_col}
             FROM {table} a JOIN {table} b
               ON a.{subject_col} = b.{subject_col} AND a.term = b.term AND a.id < b.id
             WHERE a.term = ?1
               AND a.valid_from < COALESCE(b.valid_to, '9999-12-31')
               AND b.valid_from < COALESCE(a.valid_to, '9999-12-31')"
        );
        let overlapping: Vec<(i64,)> = sqlx::query_as(&sql).bind(term).fetch_all(&mut *conn).await?;
        for (subject_id,) in overlapping {
            drifts.push(drift(table, subject_id, "tarih aralıkları çakışıyor".to_string()));
        }
    }
    Ok(drifts)
}
