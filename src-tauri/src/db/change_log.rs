//! Değişmez olay günlüğü — `change_sets` + `change_events` (spec §4.1, §5).
//!
//! Bu modül günlüğe YAZAR ve OKUR; katlama ve projeksiyon burada YOKTUR
//! (`db/projection.rs`'in işidir). Her fonksiyon çağıranın verdiği
//! bağlantıyı kullanır — havuzdan asla okumaz/yazmaz (plan "Genel
//! Kısıtlar": transaction içinde havuz kullanmak, eski veri okumaya ya da
//! 5 saniye sonra "database is locked" hatasına yol açar).

use std::collections::BTreeMap;

use chrono::{NaiveDate, SecondsFormat, Utc};
use sqlx::SqliteConnection;

use crate::domain::history::decide::{ChangeSetFacts, NewChangeSet, PlannedEvent};
use crate::domain::history::events::{EventPayload, Stream, StoredEvent};
use crate::error::{AppError, AppResult};

/// `recorded_at` her zaman UTC'dir (yürürlük tarihinden ayrı — plan "Genel
/// Kısıtlar"); milisaniye çözünürlüklü RFC3339.
pub fn recorded_at_now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Yeni bir değişiklik kümesi ekler ve id'sini döner. `actor`,
/// `settings.operator_name`'den aynı bağlantı üzerinden okunup buraya
/// verilir (R4'ün işi); bu fonksiyon yalnız yazar.
pub async fn append_change_set(
    conn: &mut SqliteConnection,
    cs: &NewChangeSet,
    actor: &str,
    impact_json: &str,
) -> AppResult<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO change_sets
            (term, kind, effective_date, document_date, reason, actor, recorded_at,
             revokes_change_set_id, impact_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         RETURNING id",
    )
    .bind(&cs.term)
    .bind(&cs.kind)
    .bind(cs.effective_date)
    .bind(cs.document_date)
    .bind(&cs.reason)
    .bind(actor)
    .bind(recorded_at_now())
    .bind(cs.revokes_change_set_id)
    .bind(impact_json)
    .fetch_one(&mut *conn)
    .await?;
    Ok(id)
}

/// `decide()`'ın planladığı olayları sırayla ekler. `PlannedEvent.caused_by`
/// bu ÇAĞRIDAKİ `events` dizisinin bir İNDEKSİDİR (spec §5 adım 5,
/// "caused_by indeksleri gerçek id'ye çevrilir"); neden olan olay her zaman
/// kendisine bağlı olaydan ÖNCE gelir (`domain::history::decide` bunu
/// garanti eder — `build_policy_events` her zaman birincil olayın indeksini
/// `0` olarak alır ve olayı listeye ÖNCE ekler), bu yüzden tek geçişte
/// çözülebilir.
pub async fn append_events(
    conn: &mut SqliteConnection,
    change_set_id: i64,
    term: &str,
    events: &[PlannedEvent],
) -> AppResult<Vec<i64>> {
    let mut ids: Vec<i64> = Vec::with_capacity(events.len());
    for event in events {
        let encoded = event.payload.encode()?;
        let caused_by_id = resolve_caused_by(&ids, event.caused_by)?;

        let id: i64 = sqlx::query_scalar(
            "INSERT INTO change_events
                (change_set_id, stream, subject_id, term, kind, kind_version, effective_date,
                 payload, caused_by, revokes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             RETURNING id",
        )
        .bind(change_set_id)
        .bind(event.stream.as_str())
        .bind(event.subject_id)
        .bind(term)
        .bind(encoded.kind)
        .bind(encoded.version)
        .bind(event.effective_date)
        .bind(&encoded.json)
        .bind(caused_by_id)
        .bind(event.revokes)
        .fetch_one(&mut *conn)
        .await?;
        ids.push(id);
    }
    Ok(ids)
}

fn resolve_caused_by(already_inserted: &[i64], caused_by: Option<usize>) -> AppResult<Option<i64>> {
    match caused_by {
        None => Ok(None),
        Some(index) => already_inserted.get(index).copied().map(Some).ok_or_else(|| {
            AppError::Database(format!(
                "caused_by indeksi ({index}) henüz eklenmemiş bir olaya işaret ediyor."
            ))
        }),
    }
}

/// Bir dönemin TÜM olayları, `id` sırasıyla (`decide::DecisionContext.events`
/// bunu doğrudan besler; sıralama/katlama `order_events`'in işidir, burada
/// YOKTUR). Geri alınmış olaylar da DAHİLDİR — `order_events` bunları eler.
pub async fn load_term_events(conn: &mut SqliteConnection, term: &str) -> AppResult<Vec<StoredEvent>> {
    let rows: Vec<EventRow> = sqlx::query_as(
        "SELECT e.id, e.change_set_id, e.stream, e.subject_id, e.term, e.kind, e.kind_version,
                e.effective_date, e.payload, e.caused_by, e.revokes,
                CASE WHEN cs.kind = 'opening' THEN 1 ELSE 0 END AS is_opening
         FROM change_events e
         JOIN change_sets cs ON cs.id = e.change_set_id
         WHERE e.term = ?1
         ORDER BY e.id",
    )
    .bind(term)
    .fetch_all(&mut *conn)
    .await?;

    rows.into_iter().map(EventRow::into_stored_event).collect()
}

/// Yalnız BİR akış + öznenin olayları — `db/projection.rs::rebuild_stream`
/// bunu kullanır; tüm dönemi okumak özne başına gereksiz iş demektir.
pub(crate) async fn load_stream_events(
    conn: &mut SqliteConnection,
    term: &str,
    stream: Stream,
    subject_id: i64,
) -> AppResult<Vec<StoredEvent>> {
    let rows: Vec<EventRow> = sqlx::query_as(
        "SELECT e.id, e.change_set_id, e.stream, e.subject_id, e.term, e.kind, e.kind_version,
                e.effective_date, e.payload, e.caused_by, e.revokes,
                CASE WHEN cs.kind = 'opening' THEN 1 ELSE 0 END AS is_opening
         FROM change_events e
         JOIN change_sets cs ON cs.id = e.change_set_id
         WHERE e.term = ?1 AND e.stream = ?2 AND e.subject_id = ?3
         ORDER BY e.id",
    )
    .bind(term)
    .bind(stream.as_str())
    .bind(subject_id)
    .fetch_all(&mut *conn)
    .await?;

    rows.into_iter().map(EventRow::into_stored_event).collect()
}

/// Bir dönemin tüm değişiklik kümeleri, denetim/politika için gereken
/// olgularla (`revoked_by`, `revokes_change_set_id`'nin TERSİ — bir küme
/// başka bir kümeyi geri aldıysa onun `revokes_change_set_id`'si hedefi
/// gösterir; hedefin kendisi `revoked_by` alanını buradan öğrenir).
pub async fn load_change_sets(
    conn: &mut SqliteConnection,
    term: &str,
) -> AppResult<BTreeMap<i64, ChangeSetFacts>> {
    let rows: Vec<ChangeSetRow> = sqlx::query_as(
        "SELECT id, kind, effective_date, revokes_change_set_id
         FROM change_sets WHERE term = ?1",
    )
    .bind(term)
    .fetch_all(&mut *conn)
    .await?;

    let revoked_by: BTreeMap<i64, i64> = rows
        .iter()
        .filter_map(|r| r.revokes_change_set_id.map(|target| (target, r.id)))
        .collect();

    Ok(rows
        .into_iter()
        .map(|r| {
            let facts = ChangeSetFacts {
                id: r.id,
                kind: r.kind,
                effective_date: r.effective_date,
                revokes_change_set_id: r.revokes_change_set_id,
                revoked_by: revoked_by.get(&r.id).copied(),
            };
            (r.id, facts)
        })
        .collect())
}

/// `MAX(change_events.id)` — `Stale` kontrolünün dayandığı yüksek su
/// işareti (spec §5 adım 1). Günlük boşsa 0.
pub async fn high_water(conn: &mut SqliteConnection) -> AppResult<i64> {
    let value: Option<i64> = sqlx::query_scalar("SELECT MAX(id) FROM change_events")
        .fetch_one(&mut *conn)
        .await?;
    Ok(value.unwrap_or(0))
}

#[derive(sqlx::FromRow)]
struct EventRow {
    id: i64,
    change_set_id: i64,
    stream: String,
    subject_id: i64,
    term: String,
    kind: String,
    kind_version: i64,
    effective_date: NaiveDate,
    payload: String,
    caused_by: Option<i64>,
    revokes: Option<i64>,
    is_opening: i64,
}

impl EventRow {
    fn into_stored_event(self) -> AppResult<StoredEvent> {
        let stream = Stream::parse(&self.stream)?;
        let payload = EventPayload::decode(&self.kind, self.kind_version, &self.payload)?;
        Ok(StoredEvent {
            id: self.id,
            change_set_id: self.change_set_id,
            stream,
            subject_id: self.subject_id,
            term: self.term,
            effective_date: self.effective_date,
            payload,
            caused_by: self.caused_by,
            revokes: self.revokes,
            is_opening: self.is_opening != 0,
        })
    }
}

#[derive(sqlx::FromRow)]
struct ChangeSetRow {
    id: i64,
    kind: String,
    effective_date: NaiveDate,
    revokes_change_set_id: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;
    use crate::domain::history::events::Labels;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::{Connection, SqlitePool};
    use std::collections::BTreeMap as Map;

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn labels() -> crate::domain::history::events::Labels {
        Labels(Map::new())
    }

    fn sample_change_set(term: &str, date: NaiveDate) -> NewChangeSet {
        NewChangeSet {
            term: term.to_string(),
            kind: "place_student".to_string(),
            effective_date: date,
            document_date: None,
            reason: "test".to_string(),
            revokes_change_set_id: None,
        }
    }

    /// `change_sets` ve `change_events` DEĞİŞMEZDİR: bir hatayı düzeltmenin
    /// tek yolu yeni bir kayıt eklemektir (spec §4.1).
    #[tokio::test]
    async fn change_log_rejects_update_and_delete() {
        let (_dir, pool) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let term = "2026-2027/1";
        sqlx::query("INSERT OR IGNORE INTO terms (term, start_date, end_date, dates_confirmed, created_at) VALUES (?1, '2026-09-01', '2027-01-31', 0, datetime('now'))")
            .bind(term)
            .execute(&mut *conn)
            .await
            .unwrap();

        let cs_id = append_change_set(&mut conn, &sample_change_set(term, ymd(2026, 9, 1)), "tester", "{}")
            .await
            .unwrap();

        let update_err = sqlx::query("UPDATE change_sets SET reason = 'x' WHERE id = ?1")
            .bind(cs_id)
            .execute(&mut *conn)
            .await
            .unwrap_err();
        assert!(update_err.to_string().contains("değişmezdir"));

        let delete_err = sqlx::query("DELETE FROM change_sets WHERE id = ?1")
            .bind(cs_id)
            .execute(&mut *conn)
            .await
            .unwrap_err();
        assert!(delete_err.to_string().contains("değişmezdir"));

        let events = vec![PlannedEvent {
            stream: Stream::Placement,
            subject_id: 1,
            effective_date: ymd(2026, 9, 1),
            payload: EventPayload::StudentPlaced { to_company_id: 1, from_company_id: None, source: "manual".into(), labels: labels() },
            caused_by: None,
            revokes: None,
        }];
        let ids = append_events(&mut conn, cs_id, term, &events).await.unwrap();

        let update_err = sqlx::query("UPDATE change_events SET term = term WHERE id = ?1")
            .bind(ids[0])
            .execute(&mut *conn)
            .await
            .unwrap_err();
        assert!(update_err.to_string().contains("değişmezdir"));

        let delete_err = sqlx::query("DELETE FROM change_events WHERE id = ?1")
            .bind(ids[0])
            .execute(&mut *conn)
            .await
            .unwrap_err();
        assert!(delete_err.to_string().contains("değişmezdir"));
    }

    /// `caused_by`, `Decision.events` içindeki İNDEKSİ taşır; günlüğe
    /// eklenince gerçek `change_events.id`'ye çevrilmelidir.
    #[tokio::test]
    async fn append_events_resolves_caused_by_indices() {
        let (_dir, pool) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let term = "2026-2027/1";
        sqlx::query("INSERT OR IGNORE INTO terms (term, start_date, end_date, dates_confirmed, created_at) VALUES (?1, '2026-09-01', '2027-01-31', 0, datetime('now'))")
            .bind(term)
            .execute(&mut *conn)
            .await
            .unwrap();
        let cs_id = append_change_set(&mut conn, &sample_change_set(term, ymd(2026, 11, 3)), "tester", "{}")
            .await
            .unwrap();

        let events = vec![
            PlannedEvent {
                stream: Stream::CompanyHours,
                subject_id: 1,
                effective_date: ymd(2026, 11, 3),
                payload: EventPayload::HoursSet {
                    state: crate::domain::history::events::HoursState { awarded_hours: 4, max_hours_snapshot: 4, is_honorary: false, is_locked: false, notes: String::new() },
                    previous_awarded: None,
                    labels: labels(),
                },
                caused_by: None,
                revokes: None,
            },
            PlannedEvent {
                stream: Stream::CompanyHours,
                subject_id: 1,
                effective_date: ymd(2026, 11, 3),
                payload: EventPayload::HoursCapped { cap: 4, student_count: 1, labels: labels() },
                caused_by: Some(0),
                revokes: None,
            },
        ];

        let ids = append_events(&mut conn, cs_id, term, &events).await.unwrap();
        assert_eq!(ids.len(), 2);

        let caused_by: Option<i64> = sqlx::query_scalar("SELECT caused_by FROM change_events WHERE id = ?1")
            .bind(ids[1])
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(caused_by, Some(ids[0]), "caused_by gerçek id'ye çevrilmeli");
    }

    /// `caused_by` bilinmeyen bir indekse işaret ederse (programlama hatası)
    /// sessizce yutulmamalı, açık bir `AppError` dönmelidir.
    #[tokio::test]
    async fn append_events_rejects_out_of_range_caused_by() {
        let (_dir, pool) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let term = "2026-2027/1";
        sqlx::query("INSERT OR IGNORE INTO terms (term, start_date, end_date, dates_confirmed, created_at) VALUES (?1, '2026-09-01', '2027-01-31', 0, datetime('now'))")
            .bind(term)
            .execute(&mut *conn)
            .await
            .unwrap();
        let cs_id = append_change_set(&mut conn, &sample_change_set(term, ymd(2026, 11, 3)), "tester", "{}")
            .await
            .unwrap();

        let events = vec![PlannedEvent {
            stream: Stream::CompanyHours,
            subject_id: 1,
            effective_date: ymd(2026, 11, 3),
            payload: EventPayload::HoursCapped { cap: 4, student_count: 1, labels: labels() },
            caused_by: Some(99),
            revokes: None,
        }];

        let err = append_events(&mut conn, cs_id, term, &events).await.unwrap_err();
        assert!(matches!(err, AppError::Database(_)));
    }

    /// Ayrı bir bağlantıyla (`connect()`), migration'dan bağımsız olarak
    /// göç dosyasının `date()` CHECK kısıtını kanıtlar: `= x` OLSAYDI
    /// `'garbage'` ve boşluksuz `'2026-1-3'` sessizce kabul edilirdi.
    #[tokio::test]
    async fn date_check_rejects_garbage_and_unpadded_dates() {
        let (_dir, pool) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        for bad_date in ["garbage", "2026-1-3", "2026-13-01", ""] {
            let result = sqlx::query(
                "INSERT INTO terms (term, start_date, end_date, dates_confirmed, created_at)
                 VALUES ('bozuk-test', ?1, '2027-01-31', 0, datetime('now'))",
            )
            .bind(bad_date)
            .execute(&mut *conn)
            .await;
            assert!(result.is_err(), "kabul edilmemeliydi: '{bad_date}'");
        }

        // Doğru biçim kabul edilmeli.
        sqlx::query(
            "INSERT INTO terms (term, start_date, end_date, dates_confirmed, created_at)
             VALUES ('iyi-test', '2026-09-01', '2027-01-31', 0, datetime('now'))",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
    }

    /// `max_connections(1)` ile `create_in`in havuzdan değil VERİLEN
    /// bağlantıdan yazdığını kanıtlar — aynı bağlantı üzerinde açık bir
    /// `BEGIN IMMEDIATE` varken pool tükenmişse bile çalışmalıdır.
    #[tokio::test]
    async fn create_in_uses_the_given_connection() {
        let dir = tempfile::tempdir().unwrap();
        let options = SqliteConnectOptions::new()
            .filename(dir.path().join("single.db"))
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let mut conn = pool.acquire().await.unwrap();
        conn.transaction(|tx| {
            Box::pin(async move {
                let company = crate::db::companies::create_in(
                    tx,
                    &crate::domain::models::NewCompany {
                        name: "Tek Bağlantı A.Ş.".into(),
                        contact_first_name: String::new(),
                        contact_last_name: String::new(),
                        phone: String::new(),
                        email: String::new(),
                        address_text: "Adres".into(),
                        latitude: None,
                        longitude: None,
                        one_way_distance_km: Some(4.0),
                        district: String::new(),
                        notes: String::new(),
                    },
                )
                .await?;
                assert_eq!(company.name, "Tek Bağlantı A.Ş.");
                Ok::<_, AppError>(())
            })
        })
        .await
        .unwrap();
    }
}
