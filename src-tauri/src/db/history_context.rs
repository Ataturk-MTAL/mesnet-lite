//! `decide()`'ın saf olarak çalışması için gereken TÜM bağlamı, TEK bir
//! bağlantı üzerinden, tek bir dönem için yükler (spec §5 adım 3).
//!
//! Transaction içinde havuz kullanmak yasaktır (plan "Genel Kısıtlar"): bu
//! yüzden burada her okuma `&mut SqliteConnection` üzerindendir, `&SqlitePool`
//! hiç geçmez.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use sqlx::SqliteConnection;

use crate::domain::history::apply::apply_schedule;
use crate::domain::history::decide::{CompanyFacts, DecisionContext, Materialized};
use crate::domain::history::events::{Stream, WeeklySchedule};
use crate::domain::history::timeline::{fold_intervals, order_events};
use crate::domain::hour_rules::HourRule;
use crate::domain::models::Company;
use crate::domain::workload::{statutory_cap, InstitutionType};
use crate::error::AppResult;

use super::{change_log, terms};

const COMPANY_COLUMNS: &str = "id, name, contact_first_name, contact_last_name, phone, email, \
     address_text, latitude, longitude, geocode_status, one_way_distance_km, notes, \
     created_at, updated_at";

/// `source_term` yalnız `copySchedulesFromTerm` içindir: `Some(t)` ise `t`
/// dönemindeki her öğretmenin SON geçerli programı `source_schedules`'e
/// yazılır (R2b brief madde 1). `None` ise alan boş kalır.
pub async fn load(
    conn: &mut SqliteConnection,
    term: &str,
    today: NaiveDate,
    materialized: Materialized,
    source_term: Option<&str>,
) -> AppResult<DecisionContext> {
    let term_dates = terms::get_in(conn, term).await?;
    let rules = load_rules(conn).await?;
    let (statutory_cap, day_end_hour) = load_capacity_settings(conn).await?;
    let companies = load_companies(conn).await?;
    let student_names = load_student_names(conn, term).await?;
    let teacher_names = load_teacher_names(conn).await?;
    let events = change_log::load_term_events(conn, term).await?;
    let change_sets = change_log::load_change_sets(conn, term).await?;
    let high_water = change_log::high_water(conn).await?;
    let source_schedules = match source_term {
        Some(t) => load_source_schedules(conn, t).await?,
        None => BTreeMap::new(),
    };

    Ok(DecisionContext {
        today,
        term: term_dates,
        rules,
        statutory_cap,
        day_end_hour,
        companies,
        student_names,
        teacher_names,
        events,
        change_sets,
        high_water,
        materialized,
        source_schedules,
    })
}

async fn load_rules(conn: &mut SqliteConnection) -> AppResult<Vec<HourRule>> {
    let sql = "SELECT id, min_distance_km, max_distance_km, min_students, max_students, max_hours \
               FROM company_hour_rules ORDER BY min_distance_km, min_students";
    Ok(sqlx::query_as::<_, HourRule>(sql).fetch_all(&mut *conn).await?)
}

async fn setting(conn: &mut SqliteConnection, key: &str) -> AppResult<Option<String>> {
    let value: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?1")
        .bind(key)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(value)
}

/// MADDE 15/2 tavanı ve program ızgarasının bitiş saati — ikisi de
/// `settings` anahtar/değer tablosundan (bkz. `commands/assignment_commands.rs`'in
/// aynı ayarları okuma biçimi; burada AYNI varsayılanlar korunur).
async fn load_capacity_settings(conn: &mut SqliteConnection) -> AppResult<(i64, i64)> {
    let institution_type = InstitutionType::parse(setting(conn, "institution_type").await?.as_deref().unwrap_or("other"));
    let is_metropolitan = setting(conn, "is_metropolitan_district").await?.as_deref() == Some("true");
    let cap = statutory_cap(institution_type, is_metropolitan);

    let day_end_hour = setting(conn, "day_end_hour")
        .await?
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(17);

    Ok((cap, day_end_hour))
}

/// TÜM işletmeler (aktif ve pasif) — `decide`, varlık ve aktiflik
/// denetimlerini ayrı ayrı yapar (`require_company` / `require_active_company`).
async fn load_companies(conn: &mut SqliteConnection) -> AppResult<BTreeMap<i64, CompanyFacts>> {
    let sql = format!("SELECT {COMPANY_COLUMNS} FROM companies");
    let rows: Vec<Company> = sqlx::query_as(&sql).fetch_all(&mut *conn).await?;

    let active_flags: Vec<(i64, i64)> = sqlx::query_as("SELECT id, is_active FROM companies").fetch_all(&mut *conn).await?;
    let active: BTreeMap<i64, bool> = active_flags.into_iter().map(|(id, flag)| (id, flag != 0)).collect();

    Ok(rows
        .into_iter()
        .map(|c| {
            let is_active = active.get(&c.id).copied().unwrap_or(true);
            let facts = CompanyFacts { name: c.name.clone(), round_trip_km: c.round_trip_distance_km(), is_active };
            (c.id, facts)
        })
        .collect())
}

/// Yalnız BU dönemin öğrencileri — öğrenci listesi her yıl yenilenir
/// (`students.term`), geçen yılın öğrencisi bu dönemin komutlarında
/// geçerli bir kimlik değildir.
async fn load_student_names(conn: &mut SqliteConnection, term: &str) -> AppResult<BTreeMap<i64, String>> {
    let rows: Vec<(i64, String, String)> =
        sqlx::query_as("SELECT id, first_name, last_name FROM students WHERE term = ?1")
            .bind(term)
            .fetch_all(&mut *conn)
            .await?;
    Ok(rows.into_iter().map(|(id, first, last)| (id, format!("{first} {last}"))).collect())
}

/// Öğretmenler dönemsiz kalıcıdır (`teachers` tablosunda `term` yok); pasif
/// öğretmen de dahildir — `decide` bugün öğretmen için bir aktiflik
/// denetimi yapmıyor.
async fn load_teacher_names(conn: &mut SqliteConnection) -> AppResult<BTreeMap<i64, String>> {
    let rows: Vec<(i64, String, String)> = sqlx::query_as("SELECT id, first_name, last_name FROM teachers")
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(|(id, first, last)| (id, format!("{first} {last}"))).collect())
}

/// `source_term`'deki her öğretmenin SON (kapanmamış) `teacher_schedule`
/// aralığı. Kaynak dönem `terms` tablosunda yoksa ya da hiç program
/// olayı yoksa boş harita döner — `decide::teacher::copy_schedules_from_term`
/// bunu zaten `FactNotTrueAtDate` ile karşılıyor, burada hata sayılmaz.
async fn load_source_schedules(conn: &mut SqliteConnection, source_term: &str) -> AppResult<BTreeMap<i64, WeeklySchedule>> {
    let source_dates = match terms::get_in(conn, source_term).await {
        Ok(dates) => dates,
        Err(crate::error::AppError::NotFound(_)) => return Ok(BTreeMap::new()),
        Err(other) => return Err(other),
    };

    let events = change_log::load_term_events(conn, source_term).await?;
    let mut by_teacher: BTreeMap<i64, Vec<_>> = BTreeMap::new();
    for event in events.into_iter().filter(|e| e.stream == Stream::TeacherSchedule) {
        by_teacher.entry(event.subject_id).or_default().push(event);
    }

    let mut result = BTreeMap::new();
    for (teacher_id, teacher_events) in by_teacher {
        let ordered = order_events(&teacher_events, source_dates.start);
        let intervals = fold_intervals(&ordered, apply_schedule);
        if let Some(last) = intervals.last() {
            result.insert(teacher_id, last.state.clone());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;
    use crate::domain::history::decide::{ChangeRequest, ChangeCommand};
    use sqlx::SqlitePool;

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// `load` — hiçbir öğrenci/öğretmen/işletme olmadan bile — dönemin
    /// varsayılan ayarlarıyla (MADDE 15/2 tavanı, ızgara bitişi) dolu bir
    /// bağlam üretmeli; boş bağlam `decide`'ı besleyemez.
    #[tokio::test]
    async fn load_returns_seeded_defaults_for_a_fresh_database() {
        let (_dir, pool) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let ctx = load(&mut conn, "2026-2027/1", ymd(2026, 10, 1), Materialized::default(), None).await.unwrap();

        // seed: institution_type=other, is_metropolitan_district=true => 20 (MADDE 15/2-b-1)
        assert_eq!(ctx.statutory_cap, 20);
        assert_eq!(ctx.day_end_hour, 17);
        assert_eq!(ctx.rules.len(), 16, "seed 16 saat kuralı yazmalı");
        assert!(ctx.companies.is_empty());
        assert!(ctx.source_schedules.is_empty(), "source_term verilmedi, boş kalmalı");
    }

    /// `source_term` verilince kaynak dönemdeki öğretmenin SON programı
    /// taşınır; hedef dönemde HİÇBİR şey değişmemiş olmalı.
    #[tokio::test]
    async fn load_fills_source_schedules_from_the_given_term() {
        let (_dir, pool) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        insert_in_test_term(&mut conn, "2025-2026/1", ymd(2025, 9, 1), ymd(2026, 1, 31)).await;
        let teacher_id = insert_teacher(&mut conn, "Ali", "Veli").await;
        insert_schedule_event(&mut conn, "2025-2026/1", teacher_id, ymd(2025, 9, 1), "[[1,3]]").await;
        insert_schedule_event(&mut conn, "2025-2026/1", teacher_id, ymd(2025, 10, 1), "[[2,4]]").await;

        let ctx = load(&mut conn, "2026-2027/1", ymd(2026, 10, 1), Materialized::default(), Some("2025-2026/1")).await.unwrap();

        let schedule = ctx.source_schedules.get(&teacher_id).expect("öğretmenin programı taşınmalı");
        assert_eq!(schedule.0.len(), 1, "yalnız SON aralık taşınmalı");
        assert!(schedule.0.iter().any(|s| s.day_of_week == 2 && s.hour == 4));

        // `ChangeRequest` içe aktarılabilirliğini de kanıtlar (ölü import kalmasın).
        let _ = ChangeRequest {
            term: "2026-2027/1".into(),
            effective_date: None,
            document_date: None,
            reason: String::new(),
            command: ChangeCommand::ClearCoordination,
        };
    }

    async fn insert_in_test_term(conn: &mut SqliteConnection, term: &str, start: NaiveDate, end: NaiveDate) {
        terms::insert_in(conn, &crate::domain::terms::TermDates { term: term.to_string(), start, end, dates_confirmed: true })
            .await
            .unwrap();
    }

    async fn insert_teacher(conn: &mut SqliteConnection, first: &str, last: &str) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO teachers (first_name, last_name, registry_no, field, branches, employment_type, base_hours, max_extra_hours, other_extra_hours, chief_type, is_active)
             VALUES (?1, ?2, '1', 'Elektrik', '[]', 'tenured', 15, 24, 0, 'none', 1) RETURNING id",
        )
        .bind(first)
        .bind(last)
        .fetch_one(&mut *conn)
        .await
        .unwrap()
    }

    async fn insert_schedule_event(conn: &mut SqliteConnection, term: &str, teacher_id: i64, effective_date: NaiveDate, schedule_json: &str) {
        let cs_id: i64 = sqlx::query_scalar(
            "INSERT INTO change_sets (term, kind, effective_date, document_date, reason, actor, recorded_at, revokes_change_set_id, impact_json)
             VALUES (?1, 'set_teacher_schedule', ?2, NULL, '', 'tester', datetime('now'), NULL, '{}') RETURNING id",
        )
        .bind(term)
        .bind(effective_date)
        .fetch_one(&mut *conn)
        .await
        .unwrap();

        let payload = format!(r#"{{"schedule":{schedule_json},"previousSlotCount":null,"source":"manual","labels":{{}}}}"#);
        sqlx::query(
            "INSERT INTO change_events (change_set_id, stream, subject_id, term, kind, kind_version, effective_date, payload, caused_by, revokes)
             VALUES (?1, 'teacher_schedule', ?2, ?3, 'schedule_set', 1, ?4, ?5, NULL, NULL)",
        )
        .bind(cs_id)
        .bind(teacher_id)
        .bind(term)
        .bind(effective_date)
        .bind(payload)
        .execute(&mut *conn)
        .await
        .unwrap();
    }
}
