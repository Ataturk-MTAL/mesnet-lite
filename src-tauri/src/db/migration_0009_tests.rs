//! Migration 0009 (ders saati numaralandırmasını 1'den başlatma) doğrulama
//! testleri.
//!
//! `init_pool` her testte TÜM göçleri (0009 dahil) BOŞ bir veritabanına
//! uygular; bu yüzden gerçek göç senaryosunu sınamak için ÖNCE elle
//! "eski (8-16) numaralandırmayla yazılmış" veri eklenir, SONRA 0009'un SQL
//! dosyası `sqlx::raw_sql` ile YENİDEN çalıştırılır — `db/teaching_load.rs`
//! içindeki `apply_0008`/`migration_0008_is_idempotent` ile AYNI desen.

use chrono::NaiveDate;
use sqlx::SqlitePool;

use crate::db::teaching_load_test_support::{seed_teacher, TERM};
use crate::db::{init_pool, projection};
use crate::domain::models::ChiefType;

async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
    (dir, pool)
}

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

/// `init_pool`'un tohumladığı dönemin başlangıcı (bkz. `teaching_load_test_support`).
fn term_start() -> NaiveDate {
    ymd(2026, 9, 1)
}

async fn apply_0009(pool: &SqlitePool) {
    let sql = std::fs::read_to_string("migrations/0009_lesson_numbering.sql").unwrap();
    sqlx::raw_sql(&sql).execute(pool).await.unwrap();
}

fn pairs_to_json(pairs: &[(i64, i64)]) -> String {
    let items: Vec<String> = pairs.iter().map(|(d, h)| format!("[{d},{h}]")).collect();
    format!("[{}]", items.join(","))
}

/// Eski (8-16) numaralandırmayla bir `schedule_set` açılış olayı VE ona
/// karşılık gelen açık projeksiyon satırını elle yazar — migration 0006'nın
/// "Adım 3e"/"Adım 4"ünün ürettiği durumun AYNISI, yalnız saatler henüz
/// kaydırılmamış (gerçek veritabanında bulunan durumun kopyası).
async fn seed_legacy_schedule(pool: &SqlitePool, teacher_id: i64, pairs: &[(i64, i64)]) {
    let schedule_json = pairs_to_json(pairs);
    let cs_id: i64 = sqlx::query_scalar(
        "INSERT INTO change_sets (term, kind, effective_date, document_date, reason, actor, recorded_at, revokes_change_set_id, impact_json)
         VALUES (?1, 'opening', ?2, NULL, 'test', 'tester', datetime('now'), NULL, '{}') RETURNING id",
    )
    .bind(TERM)
    .bind(term_start())
    .fetch_one(pool)
    .await
    .unwrap();

    let payload = format!(
        r#"{{"schedule":{schedule_json},"previousSlotCount":null,"source":"opening","labels":{{}}}}"#
    );
    let event_id: i64 = sqlx::query_scalar(
        "INSERT INTO change_events (change_set_id, stream, subject_id, term, kind, kind_version, effective_date, payload, caused_by, revokes)
         VALUES (?1, 'teacher_schedule', ?2, ?3, 'schedule_set', 1, ?4, ?5, NULL, NULL) RETURNING id",
    )
    .bind(cs_id)
    .bind(teacher_id)
    .bind(TERM)
    .bind(term_start())
    .bind(&payload)
    .fetch_one(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO teacher_schedule_periods (teacher_id, term, valid_from, valid_to, slots_json, source_event_id)
         VALUES (?1, ?2, ?3, NULL, ?4, ?5)",
    )
    .bind(teacher_id)
    .bind(TERM)
    .bind(term_start())
    .bind(&schedule_json)
    .bind(event_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn schedule_slots_json(pool: &SqlitePool, teacher_id: i64) -> String {
    sqlx::query_scalar("SELECT slots_json FROM teacher_schedule_periods WHERE teacher_id = ?1")
        .bind(teacher_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn event_schedule_json(pool: &SqlitePool, teacher_id: i64) -> String {
    sqlx::query_scalar(
        "SELECT json_extract(payload, '$.schedule') FROM change_events
         WHERE kind = 'schedule_set' AND subject_id = ?1",
    )
    .bind(teacher_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Gerçek senaryonun kopyası (brief): 8-16 arası saatlerle yazılmış bir
/// program (hem olay hem projeksiyon) göç sonrası 1-9 olur; slot sayısı
/// değişmez ve `verify_term` sürüklenme bulmaz.
#[tokio::test]
async fn migration_0009_shifts_schedule_hours_and_keeps_the_projection_consistent_with_the_log() {
    let (_dir, pool) = test_pool().await;
    let teacher_id = seed_teacher(&pool, "Eski", ChiefType::None).await;
    seed_legacy_schedule(&pool, teacher_id, &[(1, 8), (2, 9), (5, 16)]).await;

    apply_0009(&pool).await;

    let slots_json = schedule_slots_json(&pool, teacher_id).await;
    let parsed: Vec<(i64, i64)> = serde_json::from_str(&slots_json).unwrap();
    assert_eq!(parsed, vec![(1, 1), (2, 2), (5, 9)], "8->1, 9->2, 16->9 (kayma -7)");
    assert_eq!(parsed.len(), 3, "slot sayısı değişmemeli");

    let event_json = event_schedule_json(&pool, teacher_id).await;
    let event_parsed: Vec<(i64, i64)> = serde_json::from_str(&event_json).unwrap();
    assert_eq!(event_parsed, parsed, "olay yükü projeksiyonla AYNI kaymayı taşımalı");

    let mut conn = pool.acquire().await.unwrap();
    let drifts = projection::verify_term(&mut conn, TERM).await.unwrap();
    assert!(drifts.is_empty(), "göç sonrası projeksiyon günlükle tutarsız: {drifts:?}");
}

/// Göç ikinci kez uygulanınca saatleri TEKRAR kaydırmamalı: birinci
/// uygulamadan sonra tüm değerler zaten 8'in altındadır, `WHERE ... >= 8`
/// koşulu ikinci koşuyu doğal olarak no-op yapar.
#[tokio::test]
async fn migration_0009_is_idempotent() {
    let (_dir, pool) = test_pool().await;
    let teacher_id = seed_teacher(&pool, "Eski", ChiefType::None).await;
    seed_legacy_schedule(&pool, teacher_id, &[(1, 8), (2, 9)]).await;

    apply_0009(&pool).await;
    let after_first = schedule_slots_json(&pool, teacher_id).await;

    apply_0009(&pool).await;
    let after_second = schedule_slots_json(&pool, teacher_id).await;

    let parsed: Vec<(i64, i64)> = serde_json::from_str(&after_second).unwrap();
    assert_eq!(parsed, vec![(1, 1), (2, 2)]);
    assert_eq!(
        serde_json::from_str::<Vec<(i64, i64)>>(&after_first).unwrap(),
        parsed,
        "ikinci uygulama saatleri tekrar kaydırmamalı"
    );
}

/// 8'den küçük bir saat, satırın ZATEN yeni numaralandırmayla yazıldığı
/// anlamına gelir; göç böyle bir satıra dokunmaz.
#[tokio::test]
async fn migration_0009_does_not_touch_hours_already_below_eight() {
    let (_dir, pool) = test_pool().await;
    let teacher_id = seed_teacher(&pool, "Yeni", ChiefType::None).await;
    seed_legacy_schedule(&pool, teacher_id, &[(1, 3), (2, 5)]).await;

    apply_0009(&pool).await;

    let slots_json = schedule_slots_json(&pool, teacher_id).await;
    let parsed: Vec<(i64, i64)> = serde_json::from_str(&slots_json).unwrap();
    assert_eq!(parsed, vec![(1, 3), (2, 5)], "1..7 aralığındaki saatler değişmemeli");
}

/// `coordination_periods.visit_hour`, `change_events.payload.state.visitHour`
/// ve eski `assignments.visit_hour` tablosu da AYNI kaymayı taşımalı — bu
/// veritabanında 0 satırdır, ama başka bir kurulumda dolu olabilir (brief).
#[tokio::test]
async fn migration_0009_shifts_coordination_and_legacy_assignment_hours_too() {
    let (_dir, pool) = test_pool().await;
    let teacher_id = seed_teacher(&pool, "Koordinatör", ChiefType::None).await;
    let company_id: i64 = sqlx::query_scalar(
        "INSERT INTO companies (name, address_text, created_at, updated_at)
         VALUES ('Test İşletme', 'Adres', datetime('now'), datetime('now')) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    // coordination_periods + karşılık gelen change_events(coordinator_assigned).
    let cs_id: i64 = sqlx::query_scalar(
        "INSERT INTO change_sets (term, kind, effective_date, document_date, reason, actor, recorded_at, revokes_change_set_id, impact_json)
         VALUES (?1, 'opening', ?2, NULL, 'test', 'tester', datetime('now'), NULL, '{}') RETURNING id",
    )
    .bind(TERM)
    .bind(term_start())
    .fetch_one(&pool)
    .await
    .unwrap();
    let payload = format!(
        r#"{{"state":{{"teacherId":{teacher_id},"visitDay":1,"visitHour":12,"isForced":false,"forceReason":null}},"fromTeacherId":null,"labels":{{}}}}"#
    );
    let event_id: i64 = sqlx::query_scalar(
        "INSERT INTO change_events (change_set_id, stream, subject_id, term, kind, kind_version, effective_date, payload, caused_by, revokes)
         VALUES (?1, 'coordination', ?2, ?3, 'coordinator_assigned', 1, ?4, ?5, NULL, NULL) RETURNING id",
    )
    .bind(cs_id)
    .bind(company_id)
    .bind(TERM)
    .bind(term_start())
    .bind(&payload)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO coordination_periods (company_id, term, valid_from, valid_to, teacher_id, visit_day, visit_hour, is_forced, force_reason, source_event_id)
         VALUES (?1, ?2, ?3, NULL, ?4, 1, 12, 0, NULL, ?5)",
    )
    .bind(company_id)
    .bind(TERM)
    .bind(term_start())
    .bind(teacher_id)
    .bind(event_id)
    .execute(&pool)
    .await
    .unwrap();

    // Eski (event-sourced olmayan) `assignments` tablosu.
    sqlx::query(
        "INSERT INTO assignments (teacher_id, company_id, term, visit_day, visit_hour, is_forced, force_reason, created_at, updated_at)
         VALUES (?1, ?2, ?3, 1, 15, 0, NULL, datetime('now'), datetime('now'))",
    )
    .bind(teacher_id)
    .bind(company_id)
    .bind(TERM)
    .execute(&pool)
    .await
    .unwrap();

    apply_0009(&pool).await;

    let coordination_hour: i64 = sqlx::query_scalar("SELECT visit_hour FROM coordination_periods WHERE company_id = ?1")
        .bind(company_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(coordination_hour, 5, "12 - 7 = 5");

    let event_hour: i64 = sqlx::query_scalar("SELECT json_extract(payload, '$.state.visitHour') FROM change_events WHERE id = ?1")
        .bind(event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(event_hour, 5, "olay yükü projeksiyonla AYNI kaymayı taşımalı");

    let assignment_hour: i64 = sqlx::query_scalar("SELECT visit_hour FROM assignments WHERE company_id = ?1")
        .bind(company_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(assignment_hour, 8, "15 - 7 = 8");
}
