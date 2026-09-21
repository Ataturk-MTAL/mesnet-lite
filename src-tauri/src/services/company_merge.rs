//! İşletme birleştirme: CSV içe aktarımından aynı işletmenin farklı yazımla
//! iki kez girilmesini düzeltir (kullanıcı teşhisi, tekrar eden bir ihtiyaç).
//! Hangi kaydın kalacağına kullanıcı karar verir; kaynaktaki HER ŞEY hedefe
//! taşınır ve kaynak PASİFLEŞTİRİLİR (silinmez — geçmişi vardır, bkz.
//! `companies::set_active_in`).
//!
//! Her adım `change_service::execute_in` ÜZERİNDEN, TEK `BEGIN IMMEDIATE`
//! transaction içinde, AYNI `effective_date`/`reason` ile geçer. Bir adım
//! kapı tarafından reddedilirse (`Rejected`/`Stale`) bu, `AppError::Validation`a
//! çevrilir ve `?` transaction'ı DÜŞÜRÜR (sqlx otomatik ROLLBACK yapar) —
//! içe aktarmanın (`csv_import`/`student_list_apply`) "atla ve devam et"
//! davranışının BİLİNÇLİ TERSİ: yarım bir birleşme, iki kaydı da bozulmuş
//! bırakır, bu yüzden ya HEPSİ ya HİÇBİRİ yazılır.

use chrono::NaiveDate;
use serde::Serialize;
use sqlx::SqlitePool;

use crate::db::{companies, settings};
use crate::domain::history::decide::{ChangeCommand, ChangeRequest, CompanyHoursRow, ImpactSummary, TransferTarget};
use crate::domain::models::Company;
use crate::error::{AppError, AppResult};
use crate::services::change_service::{execute_in, ChangeMode, ChangeOutcome};

/// Önizlemede gösterilen tek öğrenci satırı.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeStudentRow {
    pub student_id: i64,
    pub full_name: String,
}

/// `preview_company_merge` yanıtı. Hiçbir şeyi YAZMAZ — yalnız MEVCUT
/// projeksiyonlardan (`student_placements`, `coordination_periods`,
/// `company_hour_periods`) okur. `warnings` bu yüzden şimdilik her zaman
/// boştur: gerçek etki (ör. hedefte kapasite aşımı) yalnız `decide`den
/// geçerek üretilir ve bu yalnız `apply_company_merge` sırasında olur.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanyMergePreview {
    pub from_name: String,
    pub into_name: String,
    pub students: Vec<MergeStudentRow>,
    pub awarded_hours_to_clear: i64,
    pub ends_coordination: bool,
    pub warnings: Vec<String>,
}

/// `apply_company_merge` yanıtı.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanyMergeSummary {
    pub moved_students: i64,
    pub cleared_hours: i64,
    pub ended_coordination: bool,
    pub warnings: Vec<String>,
}

/// Değişiklik YAZMADAN, kaynaktan hedefe taşınacakları gösterir.
pub async fn preview_company_merge(pool: &SqlitePool, from_company_id: i64, into_company_id: i64) -> AppResult<CompanyMergePreview> {
    let (from, into) = validate_pair(pool, from_company_id, into_company_id).await?;
    let term = active_term(pool).await?;

    let students = placed_students(pool, &term, from_company_id).await?;
    let awarded_hours_to_clear = current_awarded_hours(pool, &term, from_company_id).await?;
    let ends_coordination = current_coordinator(pool, &term, from_company_id).await?.is_some();

    Ok(CompanyMergePreview {
        from_name: from.name,
        into_name: into.name,
        students,
        awarded_hours_to_clear,
        ends_coordination,
        warnings: Vec::new(),
    })
}

/// Kaynaktaki her şeyi hedefe taşır, kaynağı pasifleştirir. TEK transaction:
/// bir adım reddedilirse hiçbir şey yazılmaz (bkz. modül yorumu).
///
/// `today`, komut sınırında (`commands/company_merge_commands.rs`) BİR kez
/// hesaplanıp buraya taşınır (`history_commands.rs`'teki desenin aynısı);
/// böylece testler gerçek takvime bağlı kalmadan sabit bir "bugün" ile
/// dönem başlangıcı/tarih zorunluluğu senaryolarını sınayabilir.
pub async fn apply_company_merge(
    pool: &SqlitePool,
    from_company_id: i64,
    into_company_id: i64,
    effective_date: Option<NaiveDate>,
    reason: String,
    today: NaiveDate,
) -> AppResult<CompanyMergeSummary> {
    validate_pair(pool, from_company_id, into_company_id).await?;
    let term = active_term(pool).await?;

    // Taşınacakların MEVCUT durumu, herhangi bir alt adım çalışmadan ÖNCE
    // tek seferde okunur: ilk transferden sonra projeksiyon değişir, o
    // noktadan sonra yeniden okumak artık "birleşmeden önceki" durumu
    // göstermez.
    let students = placed_students(pool, &term, from_company_id).await?;
    let had_coordinator = current_coordinator(pool, &term, from_company_id).await?.is_some();
    let awarded_hours = current_awarded_hours(pool, &term, from_company_id).await?;

    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let mut warnings = Vec::new();

    for student in &students {
        let command = ChangeCommand::TransferStudent {
            student_id: student.student_id,
            from_company_id,
            to: TransferTarget::Existing { company_id: into_company_id },
        };
        let impact = run_step(&mut tx, &term, effective_date, &reason, command, today).await?;
        warnings.extend(impact.warnings.into_iter().map(|w| w.message));
    }

    if had_coordinator {
        let command = ChangeCommand::EndCoordination { company_id: from_company_id };
        let impact = run_step(&mut tx, &term, effective_date, &reason, command, today).await?;
        warnings.extend(impact.warnings.into_iter().map(|w| w.message));
    }

    // Saatleri hedefe TOPLAMAK YERİNE kaynağı 0'a çekiyoruz: birleşme
    // sonrası hedefin öğrenci sayısı (dolayısıyla tavanı) değişti, saati
    // kullanıcı yeniden takdir etmeli — otomatik toplama, tavanı aşan
    // sahte bir değer üretebilirdi.
    if awarded_hours > 0 {
        let command = ChangeCommand::SetCompanyHours {
            rows: vec![CompanyHoursRow { company_id: from_company_id, awarded_hours: 0, is_honorary: false, is_locked: false, notes: String::new() }],
        };
        let impact = run_step(&mut tx, &term, effective_date, &reason, command, today).await?;
        warnings.extend(impact.warnings.into_iter().map(|w| w.message));
    }

    companies::set_active_in(&mut tx, from_company_id, false).await?;
    tx.commit().await?;

    Ok(CompanyMergeSummary { moved_students: students.len() as i64, cleared_hours: awarded_hours, ended_coordination: had_coordinator, warnings })
}

/// Tek bir alt komutu `execute_in` ÜZERİNDEN çalıştırır. `Committed`
/// dışındaki her sonuç `AppError::Validation`a çevrilir; `?` bunu
/// `apply_company_merge`in transaction'ını düşürüp geri almaya zorlar.
async fn run_step(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    term: &str,
    effective_date: Option<NaiveDate>,
    reason: &str,
    command: ChangeCommand,
    today: NaiveDate,
) -> AppResult<ImpactSummary> {
    let request = ChangeRequest { term: term.to_string(), effective_date, document_date: None, reason: reason.to_string(), command };
    let outcome = execute_in(tx, request, ChangeMode::Commit { expected_high_water: None }, today).await?;
    match outcome {
        ChangeOutcome::Committed { impact, .. } => Ok(impact),
        ChangeOutcome::Rejected { reason, .. } => Err(AppError::Validation(reason)),
        ChangeOutcome::Stale { message } => Err(AppError::Validation(message)),
        ChangeOutcome::Preview { .. } => unreachable!("ChangeMode::Commit ile çağrıldığında Preview dönmez"),
    }
}

/// İki kimliğin bu birleştirme için uygunluğunu denetler (sınırda
/// doğrulama): farklı olmalı, ikisi de var olmalı, ikisi de aktif olmalı.
/// Kaynağın aktiflik şartı brief'in asgari listesinde açıkça yazmaz ama
/// testin istediği gerçek davranıştır: kaynak zaten pasifse bu birleştirme
/// DAHA ÖNCE yapılmış demektir (adım 4) ve tekrarı anlamsız bir sonuç
/// üretir (taşınacak öğrenci yok, saat zaten 0) — kullanıcıya AÇIKÇA
/// söylenir.
async fn validate_pair(pool: &SqlitePool, from_id: i64, into_id: i64) -> AppResult<(Company, Company)> {
    if from_id == into_id {
        return Err(AppError::Validation("Kaynak ve hedef işletme aynı olamaz".into()));
    }
    let from = fetch_or_validation_error(pool, from_id, "Kaynak").await?;
    let into = fetch_or_validation_error(pool, into_id, "Hedef").await?;

    if !companies::is_active(pool, from_id).await? {
        return Err(AppError::Validation(format!("Kaynak işletme ({}) zaten pasif; bu birleştirme daha önce yapılmış olabilir.", from.name)));
    }
    if !companies::is_active(pool, into_id).await? {
        return Err(AppError::Validation(format!("Hedef işletme ({}) pasif; birleştirme için aktif bir işletme seçin.", into.name)));
    }
    Ok((from, into))
}

async fn fetch_or_validation_error(pool: &SqlitePool, id: i64, role: &str) -> AppResult<Company> {
    companies::get(pool, id).await.map_err(|_| AppError::Validation(format!("{role} işletme bulunamadı: {id}")))
}

/// Ayarlardaki aktif dönem; boşsa (hiç dönem seçilmemişse) açık bir hata.
async fn active_term(pool: &SqlitePool) -> AppResult<String> {
    let term = settings::get_active_term(pool).await?;
    if term.trim().is_empty() {
        return Err(AppError::Validation("Aktif dönem tanımlı değil; önce Ayarlar'dan bir dönem seçin.".into()));
    }
    Ok(term)
}

/// Kaynak işletmede o an (`valid_to IS NULL`) yerleşik öğrenciler.
async fn placed_students(pool: &SqlitePool, term: &str, company_id: i64) -> AppResult<Vec<MergeStudentRow>> {
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT s.id, s.first_name, s.last_name
         FROM student_placements sp
         JOIN students s ON s.id = sp.student_id
         WHERE sp.term = ?1 AND sp.company_id = ?2 AND sp.valid_to IS NULL
         ORDER BY s.first_name, s.last_name",
    )
    .bind(term)
    .bind(company_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|(student_id, first, last)| MergeStudentRow { student_id, full_name: format!("{first} {last}") }).collect())
}

/// Kaynak işletmenin o anki koordinatörü (varsa).
async fn current_coordinator(pool: &SqlitePool, term: &str, company_id: i64) -> AppResult<Option<i64>> {
    Ok(
        sqlx::query_scalar("SELECT teacher_id FROM coordination_periods WHERE company_id = ?1 AND term = ?2 AND valid_to IS NULL")
            .bind(company_id)
            .bind(term)
            .fetch_optional(pool)
            .await?,
    )
}

/// Kaynak işletmenin o anki takdir edilen saati; hiç takdir yoksa 0.
async fn current_awarded_hours(pool: &SqlitePool, term: &str, company_id: i64) -> AppResult<i64> {
    let awarded: Option<i64> =
        sqlx::query_scalar("SELECT awarded_hours FROM company_hour_periods WHERE company_id = ?1 AND term = ?2 AND valid_to IS NULL")
            .bind(company_id)
            .bind(term)
            .fetch_optional(pool)
            .await?;
    Ok(awarded.unwrap_or(0))
}
