//! Tek değişiklik kapısı (spec §5): önizleme ve kayıt AYNI `decide` üzerinden
//! geçer; yalnız `Committed` bir şey yazar.
//!
//! Adımların hepsi `pool.begin_with("BEGIN IMMEDIATE")` ile açılan tek
//! transaction içinde ve AYNI bağlantı üzerinden çalışır. Transaction açıkken
//! havuz kullanmak yasaktır: havuzdan okumak sessizce eski veriyi döndürür,
//! havuza yazmak 5 sn sonra "database is locked" hatası verir. `IMMEDIATE`
//! yazma kilidini en başta alır; böylece stale kontrolü ile yazma arasına
//! başka bir yazıcı giremez.

use chrono::NaiveDate;
use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};

use super::change_input;
use crate::db::{change_log, companies, history_context, projection, settings, students, teachers};
use crate::domain::history::decide::{decide, ChangeRequest, Decision, DecisionContext, ImpactSummary, RowAction};
use crate::domain::history::rejection::{Rejection, RejectionCode};
use crate::error::{AppError, AppResult};

const STALE_MESSAGE: &str = "Önizlemeden sonra kayıtlar değişti. Değişikliği yeniden önizleyip onaylayın.";

/// `Preview` hiçbir şey yazmaz; `Commit`, önizlemenin gördüğü günlük
/// durumunu (`expected_high_water`) taşıyabilir. `None` ise bayat kontrolü
/// yoktur (ör. eski yazıcıların sarmalayıcısı, CSV içe aktarma).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeMode {
    Preview,
    Commit { expected_high_water: Option<i64> },
}

/// Arayüze giden sonuç (spec §8).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ChangeOutcome {
    Rejected {
        code: RejectionCode,
        reason: String,
        conflicting_change_set_ids: Vec<i64>,
        suggested_date: Option<NaiveDate>,
    },
    Stale {
        message: String,
    },
    Preview {
        impact: ImpactSummary,
        high_water: i64,
    },
    Committed {
        change_set_id: i64,
        impact: ImpactSummary,
    },
}

impl From<Rejection> for ChangeOutcome {
    fn from(rejection: Rejection) -> Self {
        ChangeOutcome::Rejected {
            code: rejection.code,
            reason: rejection.message,
            conflicting_change_set_ids: rejection.conflicting_change_set_ids,
            suggested_date: rejection.suggested_date,
        }
    }
}

/// Bir transaction açar, `execute_in`i çalıştırır ve sonuca göre karar verir:
/// yalnız `Committed` commit edilir; `Preview`, `Rejected` ve `Stale`
/// ROLLBACK ile biter. Hata (`Err`) transaction'ın düşmesiyle geri alınır.
pub async fn execute_change(pool: &SqlitePool, req: ChangeRequest, mode: ChangeMode, today: NaiveDate) -> AppResult<ChangeOutcome> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let outcome = execute_in(&mut tx, req, mode, today).await?;
    match outcome {
        ChangeOutcome::Committed { .. } => tx.commit().await?,
        _ => tx.rollback().await?,
    }
    Ok(outcome)
}

/// Çağıranın açtığı bağlantı üzerinde çalışır; commit/rollback kararı
/// çağırana aittir. `Committed` dışında bir sonuç dönerse çağıran işlemi
/// GERİ ALMALIDIR: yerinde oluşturulan satırlar (öğrenci, işletme, öğretmen)
/// `decide`'dan önce yazıldığı için o bağlantıda kalmıştır.
pub async fn execute_in(conn: &mut SqliteConnection, req: ChangeRequest, mode: ChangeMode, today: NaiveDate) -> AppResult<ChangeOutcome> {
    // Sınırda doğrulama `decide`'dan (ve dolayısıyla `history_context::load`'dan)
    // ÖNCE çalışır; ızgaranın bitişi yine de AYNI bağlantıdan, tek doğruluk
    // kaynağından (`settings::lesson_hour_end_in`) okunur — `decide` bunu
    // ayrıca `DecisionContext.day_end_hour` olarak yükler ama karar
    // fonksiyonları içinde denetlemez, bu yüzden görünmez saati burada
    // engellemek gerekir (bkz. `change_input.rs::validate_slot`).
    let day_end_hour = settings::lesson_hour_end_in(conn).await?;
    change_input::validate_command(&req.command, day_end_hour)?;
    if let Some(stale) = stale_outcome(conn, mode).await? {
        return Ok(stale);
    }

    // Yerinde oluşturma `decide`'dan ÖNCE gelir: karar, yeni satırın kimliğini
    // bağlamdan (`Materialized`) okur. Reddedilirse ya da önizleme ise
    // `execute_change` hepsini geri alır.
    let materialized = change_input::materialize(conn, &req.term, &req.command).await?;
    let source_term = change_input::source_term(&req.command);
    let ctx = history_context::load(conn, &req.term, today, materialized, source_term).await?;

    let decision = match decide(&ctx, &req) {
        Ok(decision) => decision,
        Err(rejection) => return Ok(rejection.into()),
    };

    match mode {
        // `ctx.high_water` bu bağlantıdan, bu transaction'da okundu; yerinde
        // oluşturma günlüğe dokunmadığı için `change_log::high_water(conn)` ile aynıdır.
        ChangeMode::Preview => Ok(ChangeOutcome::Preview { impact: decision.impact, high_water: ctx.high_water }),
        ChangeMode::Commit { .. } => persist(conn, &ctx, decision).await,
    }
}

/// `expected_high_water` verilmişse ve günlük ondan sonra ilerlemişse
/// önizleme bayattır (spec §5 adım 1).
async fn stale_outcome(conn: &mut SqliteConnection, mode: ChangeMode) -> AppResult<Option<ChangeOutcome>> {
    let ChangeMode::Commit { expected_high_water: Some(expected) } = mode else {
        return Ok(None);
    };
    let current = change_log::high_water(conn).await?;
    if current == expected {
        return Ok(None);
    }
    Ok(Some(ChangeOutcome::Stale { message: STALE_MESSAGE.to_string() }))
}

/// Günlüğe ekle, satır eylemlerini uygula, dokunulan akışları özne başına
/// BİR kez yeniden kur (spec §5 adım 5-6).
async fn persist(conn: &mut SqliteConnection, ctx: &DecisionContext, decision: Decision) -> AppResult<ChangeOutcome> {
    let actor = operator_name(conn).await?;
    let impact_json = serde_json::to_string(&decision.impact)
        .map_err(|e| AppError::Database(format!("Etki özeti kaydedilemedi: {e}")))?;

    let change_set_id = change_log::append_change_set(conn, &decision.change_set, &actor, &impact_json).await?;
    change_log::append_events(conn, change_set_id, &decision.change_set.term, &decision.events).await?;
    apply_row_actions(conn, &decision.row_actions).await?;
    projection::rebuild_streams(conn, &decision.touched, ctx.term.start).await?;

    Ok(ChangeOutcome::Committed { change_set_id, impact: decision.impact })
}

/// `change_sets.actor` kaynağı: `settings.operator_name`. Havuzdan değil,
/// işlemin kendi bağlantısından okunur. Ayar boşsa `actor` boş kalır
/// (şemanın varsayılanı).
async fn operator_name(conn: &mut SqliteConnection) -> AppResult<String> {
    let value: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'operator_name'")
        .fetch_optional(&mut *conn)
        .await?;
    Ok(value.unwrap_or_default().trim().to_string())
}

/// Olay günlüğü dışındaki TEK yazma kanalı (`decide::RowAction`): olaylarla
/// aynı transaction'da uygulanır.
async fn apply_row_actions(conn: &mut SqliteConnection, actions: &[RowAction]) -> AppResult<()> {
    for action in actions {
        match action {
            RowAction::DeleteStudent(id) => students::remove_in(conn, *id).await?,
            RowAction::DeleteTeacher(id) => teachers::remove_in(conn, *id).await?,
            RowAction::DeactivateCompany(id) => companies::set_active_in(conn, *id, false).await?,
        }
    }
    Ok(())
}
