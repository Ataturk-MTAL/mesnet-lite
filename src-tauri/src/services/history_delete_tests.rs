//! `history_delete` testleri: silinebilirlik kuralının gerçek senaryolarda
//! doğru çalışması, silmenin bugünkü durumu (dönem tabloları) DEĞİŞTİRMEMESİ,
//! ve guard/FK güvencelerinin (migration 0012) kanıtlanması.

use chrono::NaiveDate;
use sqlx::{SqliteConnection, SqlitePool};

use super::change_service_test_support::*;
use super::history_delete::delete_change_set;
use super::history_service::{list_history, HistoryFilter};
use crate::db::terms;
use crate::domain::history::decide::{ChangeCommand, TransferTarget};
use crate::domain::history::events::{EventPayload, HoursState, Labels};
use crate::domain::scheduling::Slot;
use crate::domain::terms::TermDates;
use crate::error::AppError;

fn filter() -> HistoryFilter {
    HistoryFilter {
        term: TERM.to_string(),
        stream: None,
        subject_id: None,
        company_id: None,
        teacher_id: None,
        include_opening: true,
        before_change_set_id: None,
        limit: MAX_LIMIT,
    }
}

const MAX_LIMIT: i64 = 200;

async fn is_deletable_of(pool: &SqlitePool, change_set_id: i64) -> bool {
    let page = list_history(pool, &filter(), november_today()).await.unwrap();
    page.entries.iter().find(|e| e.change_set_id == change_set_id).unwrap().is_deletable
}

async fn open_schedule(pool: &SqlitePool, teacher_id: i64) -> Option<(NaiveDate, String)> {
    sqlx::query_as("SELECT valid_from, slots_json FROM teacher_schedule_periods WHERE teacher_id = ?1 AND term = ?2 AND valid_to IS NULL")
        .bind(teacher_id)
        .bind(TERM)
        .fetch_optional(pool)
        .await
        .unwrap()
}

fn labels() -> Labels {
    Labels(Default::default())
}

fn hours(awarded: i64) -> HoursState {
    HoursState { awarded_hours: awarded, max_hours_snapshot: awarded, is_honorary: false, is_locked: false, notes: String::new() }
}

async fn insert_change_set(conn: &mut SqliteConnection, term: &str, kind: &str, effective_date: NaiveDate, revokes_change_set_id: Option<i64>) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO change_sets (term, kind, effective_date, document_date, reason, actor, recorded_at, revokes_change_set_id, impact_json)
         VALUES (?1, ?2, ?3, NULL, '', 'tester', datetime('now'), ?4, '{}') RETURNING id",
    )
    .bind(term)
    .bind(kind)
    .bind(effective_date)
    .bind(revokes_change_set_id)
    .fetch_one(&mut *conn)
    .await
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
async fn insert_event(
    conn: &mut SqliteConnection,
    change_set_id: i64,
    term: &str,
    stream: &str,
    subject_id: i64,
    effective_date: NaiveDate,
    payload: &EventPayload,
    caused_by: Option<i64>,
    revokes: Option<i64>,
) -> i64 {
    let encoded = payload.encode().unwrap();
    sqlx::query_scalar(
        "INSERT INTO change_events (change_set_id, stream, subject_id, term, kind, kind_version, effective_date, payload, caused_by, revokes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) RETURNING id",
    )
    .bind(change_set_id)
    .bind(stream)
    .bind(subject_id)
    .bind(term)
    .bind(encoded.kind)
    .bind(encoded.version)
    .bind(effective_date)
    .bind(encoded.json)
    .bind(caused_by)
    .bind(revokes)
    .fetch_one(&mut *conn)
    .await
    .unwrap()
}

/// Brief "Gerçek senaryo": açılış (09-01) + 09-20 kaydı (A) + 09-14
/// değişikliği (B — A'nın olayını AYNI AY kuralıyla otomatik geri alır,
/// bkz. `domain/history/decide/flags.rs`). A silinebilir, B ve açılış
/// DEĞİL; A silinince B'nin programı AYNEN 09-14'ten geçerli kalır.
#[tokio::test]
async fn same_month_auto_revoked_record_is_deletable_and_deleting_it_keeps_the_current_schedule() {
    let w = world().await;
    // Aynı ay içinde kalınmalı (`earliest_allowed`): 25 Eylül'den 14/20
    // Eylül'e giriş yapılabilir, Kasım'dan yapılamaz (önceki ay kapanır).
    let today = ymd(2026, 9, 25);

    let (set_a, _) = expect_committed(
        commit(&w.pool, request(Some(ymd(2026, 9, 20)), ChangeCommand::SetTeacherSchedule { teacher_id: w.teacher, slots: vec![Slot::new(1, 3)] }), today).await,
    );
    let (set_b, _) = expect_committed(
        commit(&w.pool, request(Some(ymd(2026, 9, 14)), ChangeCommand::SetTeacherSchedule { teacher_id: w.teacher, slots: vec![Slot::new(2, 4)] }), today).await,
    );

    assert!(is_deletable_of(&w.pool, set_a).await, "A'nın tek olayı B'nin markörüyle geri alınmış; silinebilir");
    assert!(!is_deletable_of(&w.pool, set_b).await, "B kendi markörünü taşıyor; silinemez");
    let opening_id: i64 = sqlx::query_scalar("SELECT id FROM change_sets WHERE kind = 'opening' LIMIT 1").fetch_one(&w.pool).await.unwrap();
    assert!(!is_deletable_of(&w.pool, opening_id).await, "açılış hiçbir zaman silinemez");

    let before = open_schedule(&w.pool, w.teacher).await.expect("B'nin programı açık olmalı");
    assert_eq!(before.0, ymd(2026, 9, 14), "yürürlükteki program B'ninki olmalı");

    delete_change_set(&w.pool, set_a).await.unwrap();

    let a_events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_events WHERE change_set_id = ?1").bind(set_a).fetch_one(&w.pool).await.unwrap();
    assert_eq!(a_events, 0, "A'nın olayı silinmeli");
    let a_set: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_sets WHERE id = ?1").bind(set_a).fetch_one(&w.pool).await.unwrap();
    assert_eq!(a_set, 0, "A'nın kümesi silinmeli");
    let b_events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_events WHERE change_set_id = ?1").bind(set_b).fetch_one(&w.pool).await.unwrap();
    assert_eq!(b_events, 1, "B'nin markörü gitti, KENDİ olayı duruyor");
    let b_markers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_events WHERE change_set_id = ?1 AND kind = 'revoked'").bind(set_b).fetch_one(&w.pool).await.unwrap();
    assert_eq!(b_markers, 0, "B artık hiçbir şeyi geri almıyor");

    let after = open_schedule(&w.pool, w.teacher).await.expect("B'nin programı hâlâ açık olmalı");
    assert_eq!(after, before, "dönem tabloları silmeden ÖNCEKİYLE BİREBİR aynı kalmalı");
}

/// Brief "correct çifti": X (nakil), onu düzelten Y (X'i geri alır + yeni
/// nakil olayı). X silinebilir, Y silinemez; X silinince Y'nin
/// `revokesChangeSetId` NULL'a döner, Y'nin YENİ olayı duruyor, öğrencinin
/// bugünkü yerleşimi (projeksiyon) AYNI kalır.
#[tokio::test]
async fn correcting_pair_x_is_deletable_deleting_x_keeps_ys_new_event_and_current_placement() {
    let w = world().await;
    let today = november_today();
    let student = w.students[0];

    let (x_id, _) = expect_committed(
        commit(&w.pool, request(Some(ymd(2026, 11, 3)), ChangeCommand::TransferStudent { student_id: student, from_company_id: w.company_a, to: TransferTarget::Existing { company_id: w.company_b } }), today).await,
    );
    let company_c = add_company(&w.pool, "İşletme C", 4.0).await;
    let replacement = ChangeCommand::TransferStudent { student_id: student, from_company_id: w.company_a, to: TransferTarget::Existing { company_id: company_c } };
    let (y_id, _) = expect_committed(
        commit(&w.pool, request(Some(ymd(2026, 11, 3)), ChangeCommand::Correct { change_set_id: x_id, replacement: Box::new(replacement) }), today).await,
    );

    assert!(is_deletable_of(&w.pool, x_id).await, "X'in TÜM olayları Y'nin markörlerince geri alınmış; silinebilir");
    assert!(!is_deletable_of(&w.pool, y_id).await, "Y kendi markörlerini taşıyor; silinemez");

    let placement_before = open_placement(&w.pool, student).await;
    assert_eq!(placement_before, Some(company_c), "düzeltmeden sonra öğrenci C'de olmalı");

    delete_change_set(&w.pool, x_id).await.unwrap();

    let x_set: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_sets WHERE id = ?1").bind(x_id).fetch_one(&w.pool).await.unwrap();
    assert_eq!(x_set, 0, "X silinmeli");

    let y_revokes: Option<i64> = sqlx::query_scalar("SELECT revokes_change_set_id FROM change_sets WHERE id = ?1").bind(y_id).fetch_one(&w.pool).await.unwrap();
    assert_eq!(y_revokes, None, "Y artık X'i geri almıyor");
    let y_markers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_events WHERE change_set_id = ?1 AND kind = 'revoked'").bind(y_id).fetch_one(&w.pool).await.unwrap();
    assert_eq!(y_markers, 0, "Y'nin markörleri gitti");
    let y_events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_events WHERE change_set_id = ?1").bind(y_id).fetch_one(&w.pool).await.unwrap();
    assert!(y_events >= 1, "Y'nin YENİ nakil olayı duruyor olmalı");

    let placement_after = open_placement(&w.pool, student).await;
    assert_eq!(placement_after, placement_before, "projeksiyon silmeden ÖNCEKİYLE aynı kalmalı");
}

/// Brief "Salt revoke kümesi R": X'i geri alan R, X silinince kendi
/// markörünü kaybeder ve BOŞ kaldığı için o da silinir.
#[tokio::test]
async fn a_pure_revoke_set_is_removed_once_its_only_target_is_deleted() {
    let w = world().await;
    let today = november_today();

    let (x_id, _) = expect_committed(commit(&w.pool, request(Some(ymd(2026, 11, 3)), ChangeCommand::SetCompanyHours { rows: vec![hours_row(w.company_b, 0)] }), today).await);
    let (r_id, _) = expect_committed(commit(&w.pool, request(Some(ymd(2026, 11, 3)), ChangeCommand::Revoke { change_set_id: x_id }), today).await);

    assert!(is_deletable_of(&w.pool, x_id).await);
    assert!(!is_deletable_of(&w.pool, r_id).await);

    delete_change_set(&w.pool, x_id).await.unwrap();

    let r_set: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_sets WHERE id = ?1").bind(r_id).fetch_one(&w.pool).await.unwrap();
    assert_eq!(r_set, 0, "R, X'in tek hedefiydi; boş kalınca R de silinmeli");
    let r_events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_events WHERE change_set_id = ?1").bind(r_id).fetch_one(&w.pool).await.unwrap();
    assert_eq!(r_events, 0);
}

/// Geçerli (bugünkü durumu belirleyen) bir küme silinmek istenirse
/// Validation hatası döner ve HİÇBİR ŞEY değişmez.
#[tokio::test]
async fn deleting_a_live_set_is_rejected_and_writes_nothing() {
    let w = world().await;
    let today = november_today();
    let (x_id, _) = expect_committed(commit(&w.pool, request(Some(ymd(2026, 11, 3)), ChangeCommand::SetCompanyHours { rows: vec![hours_row(w.company_b, 0)] }), today).await);

    let before = table_counts(&w.pool).await;
    let result = delete_change_set(&w.pool, x_id).await;

    assert!(matches!(result, Err(AppError::Validation(_))), "beklenmeyen sonuç: {result:?}");
    assert_eq!(table_counts(&w.pool).await, before, "reddedilen silme HİÇBİR tabloyu değiştirmemeli");
}

/// Açılış kümesi hiçbir koşulda silinemez.
#[tokio::test]
async fn opening_set_cannot_be_deleted() {
    let (_dir, pool) = test_pool().await;
    let opening_id: i64 = sqlx::query_scalar("SELECT id FROM change_sets WHERE kind = 'opening' LIMIT 1").fetch_one(&pool).await.unwrap();

    let before = table_counts(&pool).await;
    let result = delete_change_set(&pool, opening_id).await;

    assert!(matches!(result, Err(AppError::Validation(_))));
    assert_eq!(table_counts(&pool).await, before);
}

/// Bağımsız bir öznenin (B işletmesi) olaylarına, ilgisiz bir kümeyi
/// silmek DOKUNMAZ.
#[tokio::test]
async fn deleting_a_set_does_not_touch_an_unrelated_subjects_events() {
    let w = world().await;
    let today = november_today();
    let mut load = standard_load();
    load.other_extra_hours = 3;
    let (set_id, _) = expect_committed(commit(&w.pool, request(Some(ymd(2026, 11, 5)), ChangeCommand::SetTeacherLoad { teacher_id: w.teacher, load }), today).await);
    expect_committed(commit(&w.pool, request(Some(ymd(2026, 11, 5)), ChangeCommand::Revoke { change_set_id: set_id }), today).await);

    let company_b_events_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_events WHERE stream = 'company_hours' AND subject_id = ?1")
        .bind(w.company_b)
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(company_b_events_before, 0, "B'nin zaten hiç saat olayı yoktu");

    delete_change_set(&w.pool, set_id).await.unwrap();

    let company_b_events_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_events WHERE stream = 'company_hours' AND subject_id = ?1")
        .bind(w.company_b)
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(company_b_events_after, 0, "ilgisiz öznenin olayları hâlâ boş, dokunulmamış");
}

/// FK güvencesi: bir markör olayı silinirken, onu `caused_by` ile referans
/// alan KALAN bir olay (ör. geri almanın tetiklediği kapasite kaskadı) FK
/// hatası vermez — `caused_by` NULL'a çekilir, satır YAŞAR (bkz.
/// `history_delete.rs::delete_event`).
#[tokio::test]
async fn deleting_a_marker_nulls_the_caused_by_that_pointed_to_it_instead_of_failing() {
    let (_dir, pool) = test_pool().await;
    let mut conn = pool.acquire().await.unwrap();

    let s_id = insert_change_set(&mut conn, TERM, "set_company_hours", ymd(2026, 11, 3), None).await;
    let e1 = insert_event(&mut conn, s_id, TERM, "company_hours", 1, ymd(2026, 11, 3), &EventPayload::HoursSet { state: hours(6), previous_awarded: None, labels: labels() }, None, None).await;

    let r_id = insert_change_set(&mut conn, TERM, "revoke", ymd(2026, 11, 5), Some(s_id)).await;
    let marker_id = insert_event(&mut conn, r_id, TERM, "company_hours", 1, ymd(2026, 11, 5), &EventPayload::Revoked, None, Some(e1)).await;
    let cascade_id = insert_event(&mut conn, r_id, TERM, "company_hours", 1, ymd(2026, 11, 5), &EventPayload::HoursCapped { cap: 2, student_count: 1, labels: labels() }, Some(marker_id), None).await;
    drop(conn);

    delete_change_set(&pool, s_id).await.unwrap();

    let cascade_caused_by: Option<i64> = sqlx::query_scalar("SELECT caused_by FROM change_events WHERE id = ?1").bind(cascade_id).fetch_one(&pool).await.unwrap();
    assert_eq!(cascade_caused_by, None, "markör silinince onu referans alan olay NULL'a çekilmeli");
    let r_revokes: Option<i64> = sqlx::query_scalar("SELECT revokes_change_set_id FROM change_sets WHERE id = ?1").bind(r_id).fetch_one(&pool).await.unwrap();
    assert_eq!(r_revokes, None);
    let r_event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_events WHERE change_set_id = ?1").bind(r_id).fetch_one(&pool).await.unwrap();
    assert_eq!(r_event_count, 1, "R'nin kaskad olayı ayakta kalmalı; R henüz boşalmadı");
}

/// Guard varsayılan olarak 0'dır: migration 0012'den SONRA bile, servisin
/// DIŞINDAN yapılan doğrudan bir `DELETE` hâlâ reddedilir (bkz.
/// `db/change_log.rs::change_log_rejects_update_and_delete`, AYNI kural).
#[tokio::test]
async fn direct_delete_is_still_rejected_while_the_guard_is_off() {
    let (_dir, pool) = test_pool().await;
    let guard: i64 = sqlx::query_scalar("SELECT active FROM history_purge_guard WHERE id = 1").fetch_one(&pool).await.unwrap();
    assert_eq!(guard, 0, "guard varsayılan olarak kapalı");

    let opening_id: i64 = sqlx::query_scalar("SELECT id FROM change_sets WHERE kind = 'opening' LIMIT 1").fetch_one(&pool).await.unwrap();
    let err = sqlx::query("DELETE FROM change_sets WHERE id = ?1").bind(opening_id).execute(&pool).await.unwrap_err();
    assert!(err.to_string().contains("değişmezdir"));
}

/// Silme adımlarının ORTASINDA gerçek bir hata çıkarsa (burada: başka bir
/// döneme ait, `clear_term_projection`in görmediği bir projeksiyon satırı
/// hâlâ silinecek bir olayı gösteriyor — FK ihlali) transaction'ın TAMAMI
/// geri alınır: guard 0'a döner, HİÇBİR satır kalıcı olarak değişmez.
#[tokio::test]
async fn a_mid_transaction_failure_rolls_back_everything_including_the_guard() {
    let (_dir, pool) = test_pool().await;
    let mut conn = pool.acquire().await.unwrap();

    let s_id = insert_change_set(&mut conn, TERM, "set_company_hours", ymd(2026, 11, 3), None).await;
    let e1 = insert_event(&mut conn, s_id, TERM, "company_hours", 1, ymd(2026, 11, 3), &EventPayload::HoursSet { state: hours(6), previous_awarded: None, labels: labels() }, None, None).await;
    let r_id = insert_change_set(&mut conn, TERM, "revoke", ymd(2026, 11, 5), Some(s_id)).await;
    insert_event(&mut conn, r_id, TERM, "company_hours", 1, ymd(2026, 11, 5), &EventPayload::Revoked, None, Some(e1)).await;

    let other_term = "2026-2027/2";
    terms::insert_in(&mut conn, &TermDates { term: other_term.to_string(), start: ymd(2027, 2, 1), end: ymd(2027, 6, 30), dates_confirmed: true }).await.unwrap();
    sqlx::query(
        "INSERT INTO company_hour_periods (company_id, term, valid_from, valid_to, awarded_hours, max_hours_snapshot, is_honorary, is_locked, notes, source_event_id)
         VALUES (999, ?1, ?2, NULL, 4, 4, 0, 0, '', ?3)",
    )
    .bind(other_term)
    .bind(ymd(2027, 2, 1))
    .bind(e1)
    .execute(&mut *conn)
    .await
    .unwrap();
    drop(conn);

    let before = table_counts(&pool).await;
    let guard_before: i64 = sqlx::query_scalar("SELECT active FROM history_purge_guard WHERE id = 1").fetch_one(&pool).await.unwrap();
    assert_eq!(guard_before, 0);

    let result = delete_change_set(&pool, s_id).await;
    assert!(result.is_err(), "başka dönemin projeksiyon satırı E1'i hâlâ gösteriyor; silme FK'yi ihlal edip başarısız olmalı");

    assert_eq!(table_counts(&pool).await, before, "hata olunca HİÇBİR satır kalıcı olarak değişmemeli");
    let guard_after: i64 = sqlx::query_scalar("SELECT active FROM history_purge_guard WHERE id = 1").fetch_one(&pool).await.unwrap();
    assert_eq!(guard_after, 0, "rollback guard'ı da geri almalı");
}
