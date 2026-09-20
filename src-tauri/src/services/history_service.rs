//! Tarihçe (denetim listesi) okuma yolu — spec §6, §8.
//!
//! Yazma yoluyla (`change_service`) AYNI bağlam yükleyicisini
//! (`history_context::load`) ve AYNI `decide`'ı kullanır; "geri alınabilir
//! mi" sorusunun ikinci bir kopyası yoktur (bkz. `is_revocable`).

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};

use crate::db::{change_log, history_context, terms};
use crate::domain::history::audit::{describe, AuditEvent};
use crate::domain::history::decide::{decide, ChangeCommand, ChangeRequest, DecisionContext, Materialized};
use crate::domain::history::events::{EventPayload, Stream, StoredEvent};
use crate::error::{AppError, AppResult};

/// Bir sayfada dönebilecek en çok değişiklik kümesi. Tarihçe ekranı
/// sayfalıdır (`nextBeforeChangeSetId`); sınırsız istek tüm dönemi
/// bellekte katlatırdı.
pub const MAX_HISTORY_LIMIT: i64 = 200;

/// `list_history({ filter })` isteği (spec §8).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryFilter {
    pub term: String,
    pub stream: Option<String>,
    pub subject_id: Option<i64>,
    pub company_id: Option<i64>,
    pub teacher_id: Option<i64>,
    #[serde(default)]
    pub include_opening: bool,
    pub before_change_set_id: Option<i64>,
    pub limit: i64,
}

/// `entries[].events[]` ve `get_subject_history` öğesi (spec §8).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEventEntry {
    pub event_id: i64,
    pub stream: String,
    pub subject_id: i64,
    pub subject_label: String,
    pub kind: String,
    pub effective_date: NaiveDate,
    pub before: Option<String>,
    pub after: Option<String>,
    pub caused_by_event_id: Option<i64>,
    pub is_revoked: bool,
}

impl From<AuditEvent> for HistoryEventEntry {
    fn from(event: AuditEvent) -> Self {
        Self {
            event_id: event.event_id,
            stream: event.stream,
            subject_id: event.subject_id,
            subject_label: event.subject_label,
            kind: event.kind,
            effective_date: event.effective_date,
            before: event.before,
            after: event.after,
            caused_by_event_id: event.caused_by_event_id,
            is_revoked: event.is_revoked,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub change_set_id: i64,
    pub recorded_at: String,
    pub kind: String,
    pub reason: String,
    pub actor: String,
    pub effective_date: NaiveDate,
    pub document_date: Option<NaiveDate>,
    pub revoked_by_change_set_id: Option<i64>,
    pub revokes_change_set_id: Option<i64>,
    pub is_revocable: bool,
    /// Kayıt anındaki etki özetinin `warnings` dizisi, olduğu gibi.
    pub warnings: Vec<serde_json::Value>,
    pub events: Vec<HistoryEventEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub entries: Vec<HistoryEntry>,
    pub next_before_change_set_id: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct ChangeSetRow {
    id: i64,
    kind: String,
    effective_date: NaiveDate,
    document_date: Option<NaiveDate>,
    reason: String,
    actor: String,
    recorded_at: String,
    revokes_change_set_id: Option<i64>,
    impact_json: String,
}

/// Dönemin değişiklik kümeleri, en yeniden eskiye.
async fn load_rows(conn: &mut SqliteConnection, term: &str) -> AppResult<Vec<ChangeSetRow>> {
    Ok(sqlx::query_as(
        "SELECT id, kind, effective_date, document_date, reason, actor, recorded_at,
                revokes_change_set_id, impact_json
         FROM change_sets WHERE term = ?1 ORDER BY id DESC",
    )
    .bind(term)
    .fetch_all(&mut *conn)
    .await?)
}

/// Filtreyi sınırda doğrular; geçerli akışı döner.
fn validate_filter(filter: &HistoryFilter) -> AppResult<Option<Stream>> {
    if !(1..=MAX_HISTORY_LIMIT).contains(&filter.limit) {
        return Err(AppError::Validation(format!("Sayfa boyutu 1 ile {MAX_HISTORY_LIMIT} arasında olmalı")));
    }
    let stream = filter.stream.as_deref().map(parse_stream).transpose()?;
    // Kimlikler tablolar arasında çakışır (öğrenci 5 ile işletme 5); akış
    // olmadan `subjectId` hangi özneyi kastettiğini belirsiz bırakır.
    if filter.subject_id.is_some() && stream.is_none() {
        return Err(AppError::Validation("Özne kimliği için akış da seçilmelidir".to_string()));
    }
    Ok(stream)
}

fn parse_stream(raw: &str) -> AppResult<Stream> {
    Stream::parse(raw).map_err(|_| AppError::Validation(format!("Bilinmeyen akış: '{raw}'")))
}

/// Denetim listesi. Filtreler `LIMIT`'ten ÖNCE uygulanır: sayfa, filtreye
/// uyan kümelerden kesilir; önce kesip sonra süzmek filtreli sayfaları
/// boşaltırdı (spec §6).
pub async fn list_history(pool: &SqlitePool, filter: &HistoryFilter, today: NaiveDate) -> AppResult<HistoryPage> {
    let stream = validate_filter(filter)?;

    // Tek bir okuma anlığı: bağlam ile küme satırları arasına bir yazıcı girmesin.
    let mut tx = pool.begin().await?;
    let ctx = history_context::load(&mut tx, &filter.term, today, Materialized::default(), None).await?;
    let rows = load_rows(&mut tx, &filter.term).await?;
    tx.commit().await?;

    build_page(&ctx, rows, filter, stream)
}

fn build_page(ctx: &DecisionContext, rows: Vec<ChangeSetRow>, filter: &HistoryFilter, stream: Option<Stream>) -> AppResult<HistoryPage> {
    let by_id: BTreeMap<i64, &StoredEvent> = ctx.events.iter().map(|e| (e.id, e)).collect();
    let mut by_set: BTreeMap<i64, Vec<&StoredEvent>> = BTreeMap::new();
    for event in &ctx.events {
        by_set.entry(event.change_set_id).or_default().push(event);
    }

    let mut matching: Vec<ChangeSetRow> = rows
        .into_iter()
        .filter(|row| filter.before_change_set_id.is_none_or(|before| row.id < before))
        .filter(|row| filter.include_opening || row.kind != "opening")
        .filter(|row| set_matches(filter, stream, by_set.get(&row.id).map(Vec::as_slice).unwrap_or(&[]), &by_id))
        .collect();

    let limit = filter.limit as usize;
    let has_more = matching.len() > limit;
    matching.truncate(limit);
    let next_before_change_set_id = if has_more { matching.last().map(|row| row.id) } else { None };

    let page_ids: Vec<i64> = matching.iter().map(|row| row.id).collect();
    let mut audited = group_audit_by_set(describe(&ctx.events, ctx.term.start, &page_ids), &by_id)?;

    let entries = matching
        .into_iter()
        .map(|row| {
            let events = audited.remove(&row.id).unwrap_or_default();
            entry_from_row(ctx, row, events)
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(HistoryPage { entries, next_before_change_set_id })
}

fn group_audit_by_set(audited: Vec<AuditEvent>, by_id: &BTreeMap<i64, &StoredEvent>) -> AppResult<BTreeMap<i64, Vec<HistoryEventEntry>>> {
    let mut grouped: BTreeMap<i64, Vec<HistoryEventEntry>> = BTreeMap::new();
    for event in audited {
        let stored = by_id
            .get(&event.event_id)
            .ok_or_else(|| AppError::Database(format!("#{} numaralı olay günlükte bulunamadı.", event.event_id)))?;
        grouped.entry(stored.change_set_id).or_default().push(event.into());
    }
    Ok(grouped)
}

fn entry_from_row(ctx: &DecisionContext, row: ChangeSetRow, events: Vec<HistoryEventEntry>) -> AppResult<HistoryEntry> {
    let revoked_by = ctx.change_sets.get(&row.id).and_then(|facts| facts.revoked_by);
    Ok(HistoryEntry {
        change_set_id: row.id,
        recorded_at: row.recorded_at,
        kind: row.kind,
        reason: row.reason,
        actor: row.actor,
        effective_date: row.effective_date,
        document_date: row.document_date,
        revoked_by_change_set_id: revoked_by,
        revokes_change_set_id: row.revokes_change_set_id,
        is_revocable: is_revocable(ctx, row.id, revoked_by),
        warnings: warnings_of(row.id, &row.impact_json)?,
        events,
    })
}

/// Kaydın kendi etki özetindeki uyarılar. Göç tohumunun `{}` özeti uyarısızdır.
fn warnings_of(change_set_id: i64, impact_json: &str) -> AppResult<Vec<serde_json::Value>> {
    let broken = |detail: &str| AppError::Database(format!("#{change_set_id} numaralı kaydın etki özeti bozuk: {detail}"));
    let impact: serde_json::Value = serde_json::from_str(impact_json).map_err(|e| broken(&e.to_string()))?;
    match impact.get("warnings") {
        None => Ok(Vec::new()),
        Some(serde_json::Value::Array(items)) => Ok(items.clone()),
        Some(_) => Err(broken("'warnings' bir dizi değil")),
    }
}

/// "Geri alınabilir mi?" — kuralın TEK sahibi `decide`'dır (spec §5.4:
/// açılış/geri alma/önceki aya düşen küme, bağlı sonraki kayıt). Burada
/// aynı kurallar kopyalanmaz: gerçek bir `revoke` kararı denenir ve
/// reddedilip reddedilmediğine bakılır. Zaten geri alınmış bir küme,
/// `decide` bunu denetlemediği için ayrıca elenir.
fn is_revocable(ctx: &DecisionContext, change_set_id: i64, revoked_by: Option<i64>) -> bool {
    if revoked_by.is_some() {
        return false;
    }
    let probe = ChangeRequest {
        term: ctx.term.term.clone(),
        effective_date: None,
        document_date: None,
        reason: String::new(),
        command: ChangeCommand::Revoke { change_set_id },
    };
    decide(ctx, &probe).is_ok()
}

// ------------------------------------------------------------------ filtreler

/// Filtrenin TÜM koşulları AYNI olayda sağlanmalıdır; kümenin bir olayı
/// yeterlidir. Hiç olayı olmayan küme (ör. işletmesiz öğrenci oluşturma)
/// yalnız filtre boşken listelenir.
fn set_matches(filter: &HistoryFilter, stream: Option<Stream>, events: &[&StoredEvent], by_id: &BTreeMap<i64, &StoredEvent>) -> bool {
    let unfiltered = stream.is_none() && filter.company_id.is_none() && filter.teacher_id.is_none();
    if unfiltered {
        return true;
    }
    events.iter().any(|event| event_matches(filter, stream, event, by_id))
}

fn event_matches(filter: &HistoryFilter, stream: Option<Stream>, event: &StoredEvent, by_id: &BTreeMap<i64, &StoredEvent>) -> bool {
    stream.is_none_or(|s| event.stream == s)
        && filter.subject_id.is_none_or(|id| event.subject_id == id)
        && filter.company_id.is_none_or(|id| concerns_company(event, id, by_id))
        && filter.teacher_id.is_none_or(|id| concerns_teacher(event, id, by_id))
}

/// Olay bu işletmeye mi dokunuyor? Yerleştirme olayları işletmeyi yükte
/// taşır (öznesi öğrencidir); saat ve koordinasyon olaylarının öznesi
/// işletmedir. Geri alma işareti, hedefinin işletmesine dokunur.
fn concerns_company(event: &StoredEvent, company_id: i64, by_id: &BTreeMap<i64, &StoredEvent>) -> bool {
    match &event.payload {
        EventPayload::StudentPlaced { to_company_id, from_company_id, .. } => *to_company_id == company_id || *from_company_id == Some(company_id),
        EventPayload::StudentTransferred { from_company_id, to_company_id, .. } => *from_company_id == company_id || *to_company_id == company_id,
        EventPayload::StudentLeft { from_company_id, .. } => *from_company_id == company_id,
        EventPayload::Revoked => revoked_target(event, by_id).is_some_and(|target| concerns_company(target, company_id, by_id)),
        _ => matches!(event.stream, Stream::CompanyHours | Stream::Coordination) && event.subject_id == company_id,
    }
}

/// Olay bu öğretmene mi dokunuyor? Yük ve program olaylarının öznesi
/// öğretmendir; koordinasyon olayları öğretmeni yükünde taşır.
fn concerns_teacher(event: &StoredEvent, teacher_id: i64, by_id: &BTreeMap<i64, &StoredEvent>) -> bool {
    match &event.payload {
        EventPayload::CoordinatorAssigned { state, from_teacher_id, .. } => state.teacher_id == teacher_id || *from_teacher_id == Some(teacher_id),
        EventPayload::CoordinatorEnded { from_teacher_id, .. } | EventPayload::CoordinatorEndedByPolicy { from_teacher_id, .. } => *from_teacher_id == teacher_id,
        EventPayload::Revoked => revoked_target(event, by_id).is_some_and(|target| concerns_teacher(target, teacher_id, by_id)),
        _ => matches!(event.stream, Stream::TeacherLoad | Stream::TeacherSchedule) && event.subject_id == teacher_id,
    }
}

fn revoked_target<'a>(marker: &StoredEvent, by_id: &BTreeMap<i64, &'a StoredEvent>) -> Option<&'a StoredEvent> {
    marker.revokes.and_then(|target_id| by_id.get(&target_id).copied())
}

// ------------------------------------------------------------ özne tarihçesi

/// Tek bir öznenin (akış + kimlik) dönemdeki tüm olayları, kayıt sırasıyla,
/// önce/sonra betimiyle (spec §8, `get_subject_history`).
pub async fn subject_history(pool: &SqlitePool, stream_raw: &str, subject_id: i64, term: &str) -> AppResult<Vec<HistoryEventEntry>> {
    let stream = parse_stream(stream_raw)?;

    let mut tx = pool.begin().await?;
    let term_dates = terms::get_in(&mut tx, term).await?;
    let events = change_log::load_term_events(&mut tx, term).await?;
    tx.commit().await?;

    // `describe` "önce" değerini her öznenin kendi olaylarından katlar; diğer
    // özneleri dışarıda bırakmak sonucu değiştirmez, yalnızca işi azaltır.
    let subject_events: Vec<StoredEvent> = events.into_iter().filter(|e| e.stream == stream && e.subject_id == subject_id).collect();
    let mut change_set_ids: Vec<i64> = subject_events.iter().map(|e| e.change_set_id).collect();
    change_set_ids.sort_unstable();
    change_set_ids.dedup();

    Ok(describe(&subject_events, term_dates.start, &change_set_ids).into_iter().map(Into::into).collect())
}
