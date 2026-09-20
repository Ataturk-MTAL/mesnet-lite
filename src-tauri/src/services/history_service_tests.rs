//! `history_service` testleri: denetim listesi (D1), filtrelerin `LIMIT`'ten
//! önce uygulanması, sayfalama, `isRevocable` ve özne tarihçesi.

use chrono::NaiveDate;

use super::change_service_test_support::*;
use super::history_service::{list_history, subject_history, HistoryEntry, HistoryFilter, MAX_HISTORY_LIMIT};
use crate::domain::history::decide::{ChangeCommand, TransferTarget};
use crate::error::AppError;

fn filter() -> HistoryFilter {
    HistoryFilter {
        term: TERM.to_string(),
        stream: None,
        subject_id: None,
        company_id: None,
        teacher_id: None,
        include_opening: false,
        before_change_set_id: None,
        limit: 50,
    }
}

fn transfer(student_id: i64, from: i64, to: i64) -> ChangeCommand {
    ChangeCommand::TransferStudent { student_id, from_company_id: from, to: TransferTarget::Existing { company_id: to } }
}

async fn entries(pool: &sqlx::SqlitePool, filter: &HistoryFilter, today: NaiveDate) -> Vec<HistoryEntry> {
    list_history(pool, filter, today).await.unwrap().entries
}

fn entry_of_kind<'a>(entries: &'a [HistoryEntry], kind: &str) -> &'a HistoryEntry {
    entries.iter().find(|e| e.kind == kind).unwrap_or_else(|| panic!("'{kind}' türünde kayıt yok: {:?}", entries.iter().map(|e| &e.kind).collect::<Vec<_>>()))
}

/// D1: denetim listesi. Nakil kümesi, "önce → sonra" değerleriyle ve
/// birincil olaya `caused_by` ile bağlı otomatik alt olayla gelir; en yeni
/// küme başta; `actor` işlemin bağlantısından okunan `operator_name`dir.
#[tokio::test]
async fn d1_audit_shows_before_after_automatic_children_and_actor() {
    let w = world().await;
    sqlx::query("UPDATE settings SET value = 'Hakan Gülen' WHERE key = 'operator_name'").execute(&w.pool).await.unwrap();
    let req = request(Some(ymd(2026, 11, 3)), transfer(w.students[0], w.company_a, w.company_b));
    let (set_id, _) = expect_committed(commit(&w.pool, req, november_today()).await);

    let page = entries(&w.pool, &filter(), november_today()).await;

    assert_eq!(page[0].change_set_id, set_id, "en yeni küme başta olmalı");
    let entry = &page[0];
    assert_eq!(entry.kind, "transfer_student");
    assert_eq!(entry.actor, "Hakan Gülen");
    assert_eq!(entry.effective_date, ymd(2026, 11, 3));
    assert!(entry.is_revocable);

    let primary = entry.events.iter().find(|e| e.kind == "student_transferred").expect("birincil nakil olayı");
    assert_eq!(primary.before, Some(w.company_a.to_string()));
    assert_eq!(primary.after, Some(w.company_b.to_string()));
    assert_eq!(primary.subject_label, "Ada Öğrenci");
    assert!(primary.caused_by_event_id.is_none());

    let capped = entry.events.iter().find(|e| e.kind == "hours_capped").expect("otomatik tavan olayı");
    assert_eq!(capped.caused_by_event_id, Some(primary.event_id), "otomatik olay birincile bağlı olmalı");
}

/// Filtre `LIMIT`'ten ÖNCE uygulanır: en yeni iki küme öğretmenle ilgisiz
/// olsa bile, öğretmen filtreli sayfa öğretmenin kayıtlarıyla dolar.
#[tokio::test]
async fn list_history_filters_before_limit() {
    let w = world().await;
    for day in [2, 3, 4] {
        let req = request(Some(ymd(2026, 11, day)), ChangeCommand::SetCompanyHours { rows: vec![hours_row(w.company_b, 0)] });
        expect_committed(commit(&w.pool, req, november_today()).await);
    }

    let mut unfiltered = filter();
    unfiltered.limit = 2;
    let newest = entries(&w.pool, &unfiltered, november_today()).await;
    assert!(newest.iter().all(|e| e.kind == "set_company_hours"), "en yeni iki küme öğretmenle ilgisiz olmalı");

    let mut by_teacher = filter();
    by_teacher.teacher_id = Some(w.teacher);
    by_teacher.limit = 2;
    let page = list_history(&w.pool, &by_teacher, november_today()).await.unwrap();

    let kinds: Vec<&str> = page.entries.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, ["assign_coordinators", "create_teacher"], "LIMIT filtreden önce uygulanırsa sayfa boş kalırdı");
    assert_eq!(page.next_before_change_set_id, None, "eşleşen başka küme yok");
}

#[tokio::test]
async fn pages_continue_with_before_change_set_id() {
    let w = world().await;
    let mut by_teacher = filter();
    by_teacher.teacher_id = Some(w.teacher);
    by_teacher.limit = 1;

    let first = list_history(&w.pool, &by_teacher, november_today()).await.unwrap();
    assert_eq!(first.entries.len(), 1);
    assert_eq!(first.entries[0].kind, "assign_coordinators");
    let cursor = first.next_before_change_set_id.expect("daha eşleşen küme var");
    assert_eq!(cursor, first.entries[0].change_set_id);

    by_teacher.before_change_set_id = Some(cursor);
    let second = list_history(&w.pool, &by_teacher, november_today()).await.unwrap();
    assert_eq!(second.entries.len(), 1);
    assert_eq!(second.entries[0].kind, "create_teacher");
    assert_eq!(second.next_before_change_set_id, None);
}

#[tokio::test]
async fn company_filter_matches_placement_payloads_and_revoke_markers() {
    let w = world().await;
    let (transfer_set, _) = expect_committed(commit(&w.pool, request(Some(ymd(2026, 11, 3)), transfer(w.students[0], w.company_a, w.company_b)), november_today()).await);
    let (revoke_set, _) = expect_committed(commit(&w.pool, request(Some(ymd(2026, 11, 3)), ChangeCommand::Revoke { change_set_id: transfer_set }), november_today()).await);
    // B ile ilgisiz bir küme.
    expect_committed(commit(&w.pool, request(Some(ymd(2026, 11, 6)), ChangeCommand::SetCompanyHours { rows: vec![hours_row(w.company_a, 8)] }), november_today()).await);

    let mut for_b = filter();
    for_b.company_id = Some(w.company_b);
    let ids: Vec<i64> = entries(&w.pool, &for_b, november_today()).await.iter().map(|e| e.change_set_id).collect();

    assert_eq!(ids, [revoke_set, transfer_set], "nakil (yükteki işletme) ve geri alma işareti (hedefi üzerinden) eşleşmeli, A'nın saati eşleşmemeli");
}

#[tokio::test]
async fn stream_and_subject_filter_selects_one_students_sets() {
    let w = world().await;
    expect_committed(commit(&w.pool, request(Some(ymd(2026, 11, 3)), transfer(w.students[1], w.company_a, w.company_b)), november_today()).await);

    let mut f = filter();
    f.stream = Some("placement".to_string());
    f.subject_id = Some(w.students[1]);
    f.include_opening = true;
    let kinds: Vec<String> = entries(&w.pool, &f, november_today()).await.into_iter().map(|e| e.kind).collect();

    assert_eq!(kinds, ["transfer_student", "create_student"], "yalnız Bora'nın kümeleri");
}

#[tokio::test]
async fn opening_sets_are_hidden_unless_requested_and_never_revocable() {
    let (_dir, pool) = test_pool().await;

    assert!(entries(&pool, &filter(), november_today()).await.is_empty(), "açılış kümesi varsayılan olarak gizli");

    let mut with_opening = filter();
    with_opening.include_opening = true;
    let page = entries(&pool, &with_opening, november_today()).await;
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].kind, "opening");
    assert!(!page[0].is_revocable);
    assert_eq!(page[0].actor, "migration");
}

#[tokio::test]
async fn revoked_sets_are_marked_and_not_revocable_again() {
    let w = world().await;
    let (transfer_set, _) = expect_committed(commit(&w.pool, request(Some(ymd(2026, 11, 3)), transfer(w.students[0], w.company_a, w.company_b)), november_today()).await);
    let (revoke_set, _) = expect_committed(commit(&w.pool, request(Some(ymd(2026, 11, 3)), ChangeCommand::Revoke { change_set_id: transfer_set }), november_today()).await);

    let page = entries(&w.pool, &filter(), november_today()).await;

    let revoke = &page[0];
    assert_eq!((revoke.change_set_id, revoke.revokes_change_set_id), (revoke_set, Some(transfer_set)));
    assert!(!revoke.is_revocable, "geri alma kümesi geri alınamaz");
    assert!(revoke.events.iter().all(|e| e.kind == "revoked" || e.caused_by_event_id.is_some()) && !revoke.events.iter().any(|e| e.is_revoked));

    let original = &page[1];
    assert_eq!((original.change_set_id, original.revoked_by_change_set_id), (transfer_set, Some(revoke_set)));
    assert!(!original.is_revocable, "zaten geri alınmış küme tekrar geri alınamaz");
    assert!(original.events.iter().all(|e| e.is_revoked), "geri alınan olaylar işaretli olmalı (üstü çizilir)");
}

/// `isRevocable` kuralın kendisini `decide`'dan alır: bağlı sonraki kaydı
/// olan küme geri alınamaz; ay değişince önceki aya düşen küme de.
#[tokio::test]
async fn is_revocable_follows_the_decide_rules() {
    let w = world().await;
    let (out_set, _) = expect_committed(commit(&w.pool, request(Some(ymd(2026, 11, 3)), transfer(w.students[0], w.company_a, w.company_b)), november_today()).await);
    let (back_set, _) = expect_committed(commit(&w.pool, request(Some(ymd(2026, 11, 20)), transfer(w.students[0], w.company_b, w.company_a)), november_today()).await);

    let page = entries(&w.pool, &filter(), november_today()).await;
    let revocable = |set_id: i64| page.iter().find(|e| e.change_set_id == set_id).unwrap().is_revocable;
    assert!(revocable(back_set), "en sondaki nakil geri alınabilir");
    assert!(!revocable(out_set), "ilk nakle bağlı sonraki nakil var: önce o geri alınmalı");

    let december = ymd(2026, 12, 10);
    let later = entries(&w.pool, &filter(), december).await;
    let back = later.iter().find(|e| e.change_set_id == back_set).expect("küme Aralık'ta da listelenir");
    assert_eq!(back.effective_date, ymd(2026, 11, 20));
    assert!(!back.is_revocable, "Kasım kümeleri Aralık'ta geri alınamaz");
}

#[tokio::test]
async fn warnings_come_from_the_recorded_impact() {
    let w = world().await;
    let mut load = standard_load();
    load.other_extra_hours = 20;
    expect_committed(commit(&w.pool, request(Some(ymd(2026, 11, 5)), ChangeCommand::SetTeacherLoad { teacher_id: w.teacher, load }), november_today()).await);

    let page = entries(&w.pool, &filter(), november_today()).await;

    let warnings = &entry_of_kind(&page, "set_teacher_load").warnings;
    assert!(warnings.iter().any(|x| x["code"] == "capacityExceeded"), "kaydedilen uyarı listelenmeli: {warnings:?}");
    assert!(entry_of_kind(&page, "create_student").warnings.is_empty());
}

#[tokio::test]
async fn invalid_filters_are_validation_errors() {
    let (_dir, pool) = test_pool().await;
    let today = november_today();

    let mut zero = filter();
    zero.limit = 0;
    let mut huge = filter();
    huge.limit = MAX_HISTORY_LIMIT + 1;
    let mut bad_stream = filter();
    bad_stream.stream = Some("nonsense".to_string());
    let mut subject_without_stream = filter();
    subject_without_stream.subject_id = Some(1);

    for bad in [zero, huge, bad_stream, subject_without_stream] {
        let result = list_history(&pool, &bad, today).await;
        assert!(matches!(result, Err(AppError::Validation(_))), "Validation beklenirdi: {result:?}");
    }
}

#[tokio::test]
async fn subject_history_lists_one_subjects_events_in_order() {
    let w = world().await;
    expect_committed(commit(&w.pool, request(Some(ymd(2026, 11, 3)), transfer(w.students[0], w.company_a, w.company_b)), november_today()).await);
    expect_committed(commit(&w.pool, request(Some(ymd(2026, 11, 4)), transfer(w.students[1], w.company_a, w.company_b)), november_today()).await);

    let history = subject_history(&w.pool, "placement", w.students[0], TERM).await.unwrap();

    let kinds: Vec<&str> = history.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, ["student_placed", "student_transferred"]);
    assert!(history.iter().all(|e| e.subject_id == w.students[0]), "başka öznenin olayı sızmamalı");
    assert_eq!((history[1].before.clone(), history[1].after.clone()), (Some(w.company_a.to_string()), Some(w.company_b.to_string())));

    let unknown = subject_history(&w.pool, "nonsense", 1, TERM).await;
    assert!(matches!(unknown, Err(AppError::Validation(_))));
}
