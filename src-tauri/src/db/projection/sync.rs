//! Bir akış+öznenin katlanmış aralıklarını ilgili projeksiyon tablosuna
//! yazar: eski satırlar silinir, yeni aralıklar sırayla eklenir. Çakışma
//! yedeği (`BEFORE INSERT` tetikleyicisi, `0006_history.sql`) burada
//! ÜRETİLMEYEN bir hatayı yakalamak için vardır — `fold_intervals`'ın P1
//! özelliği (aralıklar çakışmaz) zaten doğru girdi altında bunu garanti eder.

use serde::Serialize;
use sqlx::SqliteConnection;

use crate::domain::history::apply::{apply_coordination, apply_hours, apply_load, apply_placement, apply_schedule};
use crate::domain::history::decide::StreamKey;
use crate::domain::history::events::Stream;
use crate::domain::history::timeline::{fold_intervals, TimedEvent};
use crate::error::{AppError, AppResult};

pub(super) async fn sync_stream(conn: &mut SqliteConnection, key: &StreamKey, ordered: &[TimedEvent]) -> AppResult<()> {
    match key.stream {
        Stream::Placement => sync_placement(conn, key, ordered).await,
        Stream::CompanyHours => sync_company_hours(conn, key, ordered).await,
        Stream::Coordination => sync_coordination(conn, key, ordered).await,
        Stream::TeacherLoad => sync_teacher_load(conn, key, ordered).await,
        Stream::TeacherSchedule => sync_teacher_schedule(conn, key, ordered).await,
    }
}

/// Enum sabitlerini (`ChiefType`, `EmploymentType`) veritabanının CHECK
/// kısıtlarındaki metin karşılıklarına çevirir. Eşlemeyi burada elle
/// tekrar YAZMAK yerine mevcut `Serialize` (`#[serde(rename_all = ..)]`)
/// gövdesini kullanır — tek doğruluk kaynağı `domain::models` kalır (DRY).
pub(super) fn enum_column<T: Serialize>(value: T) -> AppResult<String> {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => Ok(s),
        other => Err(AppError::Database(format!("Beklenmeyen sabit listesi değeri: {other:?}"))),
    }
}

async fn sync_placement(conn: &mut SqliteConnection, key: &StreamKey, ordered: &[TimedEvent]) -> AppResult<()> {
    let intervals = fold_intervals(ordered, apply_placement);
    sqlx::query("DELETE FROM student_placements WHERE student_id = ?1 AND term = ?2")
        .bind(key.subject_id)
        .bind(&key.term)
        .execute(&mut *conn)
        .await?;

    for iv in &intervals {
        sqlx::query(
            "INSERT INTO student_placements (student_id, term, company_id, valid_from, valid_to, source_event_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(key.subject_id)
        .bind(&key.term)
        .bind(iv.state)
        .bind(iv.valid_from)
        .bind(iv.valid_to)
        .bind(iv.source_event_id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

async fn sync_company_hours(conn: &mut SqliteConnection, key: &StreamKey, ordered: &[TimedEvent]) -> AppResult<()> {
    let intervals = fold_intervals(ordered, apply_hours);
    sqlx::query("DELETE FROM company_hour_periods WHERE company_id = ?1 AND term = ?2")
        .bind(key.subject_id)
        .bind(&key.term)
        .execute(&mut *conn)
        .await?;

    for iv in &intervals {
        sqlx::query(
            "INSERT INTO company_hour_periods
                (company_id, term, valid_from, valid_to, awarded_hours, max_hours_snapshot,
                 is_honorary, is_locked, notes, source_event_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(key.subject_id)
        .bind(&key.term)
        .bind(iv.valid_from)
        .bind(iv.valid_to)
        .bind(iv.state.awarded_hours)
        .bind(iv.state.max_hours_snapshot)
        .bind(i64::from(iv.state.is_honorary))
        .bind(i64::from(iv.state.is_locked))
        .bind(&iv.state.notes)
        .bind(iv.source_event_id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

async fn sync_coordination(conn: &mut SqliteConnection, key: &StreamKey, ordered: &[TimedEvent]) -> AppResult<()> {
    let intervals = fold_intervals(ordered, apply_coordination);
    sqlx::query("DELETE FROM coordination_periods WHERE company_id = ?1 AND term = ?2")
        .bind(key.subject_id)
        .bind(&key.term)
        .execute(&mut *conn)
        .await?;

    for iv in &intervals {
        sqlx::query(
            "INSERT INTO coordination_periods
                (company_id, term, valid_from, valid_to, teacher_id, visit_day, visit_hour,
                 is_forced, force_reason, source_event_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(key.subject_id)
        .bind(&key.term)
        .bind(iv.valid_from)
        .bind(iv.valid_to)
        .bind(iv.state.teacher_id)
        .bind(iv.state.visit_day)
        .bind(iv.state.visit_hour)
        .bind(i64::from(iv.state.is_forced))
        .bind(&iv.state.force_reason)
        .bind(iv.source_event_id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

async fn sync_teacher_load(conn: &mut SqliteConnection, key: &StreamKey, ordered: &[TimedEvent]) -> AppResult<()> {
    let intervals = fold_intervals(ordered, apply_load);
    sqlx::query("DELETE FROM teacher_load_periods WHERE teacher_id = ?1 AND term = ?2")
        .bind(key.subject_id)
        .bind(&key.term)
        .execute(&mut *conn)
        .await?;

    for iv in &intervals {
        let chief_type = enum_column(iv.state.chief_type)?;
        let employment_type = enum_column(iv.state.employment_type)?;
        sqlx::query(
            "INSERT INTO teacher_load_periods
                (teacher_id, term, valid_from, valid_to, base_hours, max_extra_hours,
                 other_extra_hours, chief_type, employment_type, source_event_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(key.subject_id)
        .bind(&key.term)
        .bind(iv.valid_from)
        .bind(iv.valid_to)
        .bind(iv.state.base_hours)
        .bind(iv.state.max_extra_hours)
        .bind(iv.state.other_extra_hours)
        .bind(chief_type)
        .bind(employment_type)
        .bind(iv.source_event_id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

async fn sync_teacher_schedule(conn: &mut SqliteConnection, key: &StreamKey, ordered: &[TimedEvent]) -> AppResult<()> {
    let intervals = fold_intervals(ordered, apply_schedule);
    sqlx::query("DELETE FROM teacher_schedule_periods WHERE teacher_id = ?1 AND term = ?2")
        .bind(key.subject_id)
        .bind(&key.term)
        .execute(&mut *conn)
        .await?;

    for iv in &intervals {
        let slots_json = serde_json::to_string(&iv.state)
            .map_err(|e| AppError::Database(format!("Program kodlanamadı: {e}")))?;
        sqlx::query(
            "INSERT INTO teacher_schedule_periods (teacher_id, term, valid_from, valid_to, slots_json, source_event_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(key.subject_id)
        .bind(&key.term)
        .bind(iv.valid_from)
        .bind(iv.valid_to)
        .bind(slots_json)
        .bind(iv.source_event_id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}
