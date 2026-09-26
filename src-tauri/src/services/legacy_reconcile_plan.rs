//! Eski pano (`company_term_hours`/`assignments`) ile açık projeksiyon
//! satırları arasındaki FARKLARI hesaplar. Hiçbir şey YAZMAZ — yalnız okur.
//! `services::legacy_reconcile` sonucu olay olarak kalıcılaştırır.

use std::collections::BTreeSet;

use sqlx::{Sqlite, SqlitePool};

use crate::domain::history::events::{CoordinationState, HoursState};
use crate::error::AppResult;

/// (işletme, dönem) çifti — eski tablo satırlarını gruplamak için.
type Key = (i64, String);

#[derive(Debug, Clone)]
pub(super) struct HoursDiff {
    pub term: String,
    pub company_id: i64,
    pub state: HoursState,
    pub previous_awarded: Option<i64>,
}

#[derive(Debug, Clone)]
pub(super) struct CoordinatorDiff {
    pub term: String,
    pub company_id: i64,
    pub state: CoordinationState,
    pub from_teacher_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub(super) struct CoordinatorEndDiff {
    pub term: String,
    pub company_id: i64,
    pub from_teacher_id: i64,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ReconcilePlan {
    pub hours: Vec<HoursDiff>,
    pub coordinators: Vec<CoordinatorDiff>,
    pub coordinator_ends: Vec<CoordinatorEndDiff>,
    /// Yalnız rapor amaçlı: eski tabloda satırı olmayan ama projeksiyonda
    /// açık saat satırı olan işletme sayısı (brief: "dokunma, sayısını raporla").
    pub untouched_hours_only_in_projection: i64,
}

impl ReconcilePlan {
    pub(super) fn is_empty(&self) -> bool {
        self.hours.is_empty() && self.coordinators.is_empty() && self.coordinator_ends.is_empty()
    }

    /// Planın dokunduğu tüm dönemler, tekil.
    pub(super) fn terms(&self) -> BTreeSet<String> {
        let mut terms: BTreeSet<String> = BTreeSet::new();
        terms.extend(self.hours.iter().map(|d| d.term.clone()));
        terms.extend(self.coordinators.iter().map(|d| d.term.clone()));
        terms.extend(self.coordinator_ends.iter().map(|d| d.term.clone()));
        terms
    }
}

pub(super) async fn build_plan(pool: &SqlitePool) -> AppResult<ReconcilePlan> {
    let mut plan = ReconcilePlan::default();
    let legacy_hours_keys = diff_hours(pool, &mut plan).await?;
    let legacy_assignment_keys = diff_coordinators(pool, &mut plan).await?;
    diff_coordinator_ends(pool, &legacy_assignment_keys, &mut plan).await?;
    count_projection_only_hours(pool, &legacy_hours_keys, &mut plan).await?;
    Ok(plan)
}

async fn diff_hours(pool: &SqlitePool, plan: &mut ReconcilePlan) -> AppResult<BTreeSet<Key>> {
    let rows: Vec<(i64, String, i64, i64, i64, i64, String)> = sqlx::query_as(
        "SELECT company_id, term, awarded_hours, max_hours_snapshot, is_honorary, is_locked, notes FROM company_term_hours",
    )
    .fetch_all(pool)
    .await?;

    let mut keys = BTreeSet::new();
    for (company_id, term, awarded_hours, max_hours_snapshot, is_honorary, is_locked, notes) in rows {
        keys.insert((company_id, term.clone()));
        let legacy = HoursState { awarded_hours, max_hours_snapshot, is_honorary: is_honorary != 0, is_locked: is_locked != 0, notes };
        let current = fetch_open_hours(pool, company_id, &term).await?;
        let previous_awarded = current.as_ref().map(|s| s.awarded_hours);
        if current.as_ref() != Some(&legacy) {
            plan.hours.push(HoursDiff { term, company_id, state: legacy, previous_awarded });
        }
    }
    Ok(keys)
}

async fn diff_coordinators(pool: &SqlitePool, plan: &mut ReconcilePlan) -> AppResult<BTreeSet<Key>> {
    let rows: Vec<(i64, i64, String, i64, i64, i64, Option<String>)> = sqlx::query_as(
        "SELECT company_id, teacher_id, term, visit_day, visit_hour, is_forced, force_reason FROM assignments",
    )
    .fetch_all(pool)
    .await?;

    let mut keys = BTreeSet::new();
    for (company_id, teacher_id, term, visit_day, visit_hour, is_forced, force_reason) in rows {
        keys.insert((company_id, term.clone()));
        let legacy = CoordinationState { teacher_id, visit_day, visit_hour, is_forced: is_forced != 0, force_reason };
        let current = fetch_open_coordination(pool, company_id, &term).await?;
        let from_teacher_id = current.as_ref().map(|s| s.teacher_id);
        if current.as_ref() != Some(&legacy) {
            plan.coordinators.push(CoordinatorDiff { term, company_id, state: legacy, from_teacher_id });
        }
    }
    Ok(keys)
}

/// Eski tabloda ataması OLMAYAN ama açık koordinatörlük satırı olan işletme
/// için koordinatörlüğü BİTİRİR (brief madde 3).
async fn diff_coordinator_ends(pool: &SqlitePool, legacy_keys: &BTreeSet<Key>, plan: &mut ReconcilePlan) -> AppResult<()> {
    let rows: Vec<(i64, String, i64)> =
        sqlx::query_as("SELECT company_id, term, teacher_id FROM coordination_periods WHERE valid_to IS NULL")
            .fetch_all(pool)
            .await?;
    for (company_id, term, teacher_id) in rows {
        if legacy_keys.contains(&(company_id, term.clone())) {
            continue;
        }
        plan.coordinator_ends.push(CoordinatorEndDiff { term, company_id, from_teacher_id: teacher_id });
    }
    Ok(())
}

/// Eski tabloda satırı OLMAYAN ama projeksiyonda açık saat satırı olan
/// işletmelere DOKUNULMAZ; brief yalnız SAYISINI raporlamayı ister.
async fn count_projection_only_hours(pool: &SqlitePool, legacy_keys: &BTreeSet<Key>, plan: &mut ReconcilePlan) -> AppResult<()> {
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT company_id, term FROM company_hour_periods WHERE valid_to IS NULL")
            .fetch_all(pool)
            .await?;
    plan.untouched_hours_only_in_projection =
        rows.iter().filter(|row| !legacy_keys.contains(&(row.0, row.1.clone()))).count() as i64;
    Ok(())
}

/// Hem plan kurulurken (`&SqlitePool`) hem de kalıcılaştırma transaction'ı
/// İÇİNDE (`&mut SqliteConnection`) çalışır — sqlx'in genel `Executor`
/// arayüzü sayesinde tek gövde iki bağlamda da kullanılır (DRY).
pub(super) async fn fetch_open_hours<'e, E>(executor: E, company_id: i64, term: &str) -> AppResult<Option<HoursState>>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    let row: Option<(i64, i64, i64, i64, String)> = sqlx::query_as(
        "SELECT awarded_hours, max_hours_snapshot, is_honorary, is_locked, notes
         FROM company_hour_periods WHERE company_id = ?1 AND term = ?2 AND valid_to IS NULL",
    )
    .bind(company_id)
    .bind(term)
    .fetch_optional(executor)
    .await?;
    Ok(row.map(|(awarded_hours, max_hours_snapshot, is_honorary, is_locked, notes)| HoursState {
        awarded_hours,
        max_hours_snapshot,
        is_honorary: is_honorary != 0,
        is_locked: is_locked != 0,
        notes,
    }))
}

pub(super) async fn fetch_open_coordination<'e, E>(executor: E, company_id: i64, term: &str) -> AppResult<Option<CoordinationState>>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    let row: Option<(i64, i64, i64, i64, Option<String>)> = sqlx::query_as(
        "SELECT teacher_id, visit_day, visit_hour, is_forced, force_reason
         FROM coordination_periods WHERE company_id = ?1 AND term = ?2 AND valid_to IS NULL",
    )
    .bind(company_id)
    .bind(term)
    .fetch_optional(executor)
    .await?;
    Ok(row.map(|(teacher_id, visit_day, visit_hour, is_forced, force_reason)| CoordinationState {
        teacher_id,
        visit_day,
        visit_hour,
        is_forced: is_forced != 0,
        force_reason,
    }))
}
