use crate::db::company_hours::HoursInput;
use crate::db::hour_rules::select_narrowest;
use crate::db::teaching_load::{self, BranchStudentCounts, TermBranchHoursInput};
use crate::db::{companies, company_hours, hour_rules, settings, students, AppState};
use crate::domain::history::decide::{ChangeCommand, ChangeRequest, CompanyHoursRow};
use crate::domain::hour_distribution::{distribute, DistributionCandidate, DistributionOutcome};
use crate::domain::terms::{parse_date, today_local};
use crate::error::AppResult;
use crate::services::change_service::commit_legacy_change;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::{BTreeMap, BTreeSet};
use tauri::State;

/// Takdir ekranındaki tek bir satır.
/// MESNET'in `İşletme Saat Ayarları` tablosunun sütunlarına karşılık gelir.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoursRow {
    pub company_id: i64,
    pub company_name: String,
    pub address_text: String,
    /// CSV'den gelen tek yön yol mesafesi.
    pub one_way_distance_km: Option<f64>,
    /// Saat kuralında kullanılan değer: tek yönün iki katı.
    pub round_trip_distance_km: Option<f64>,
    pub student_count: i64,
    /// `Verilebilir Maks.` — kural tablosundan hesaplanır.
    /// Kural bulunamazsa None ve satır 0 saatle kalır.
    pub max_hours: Option<i64>,
    /// `Takdir Edilen` — kayıtlı değer; hiç girilmemişse 0.
    pub awarded_hours: i64,
    pub is_honorary: bool,
    pub is_locked: bool,
    pub notes: String,
    /// Takdir daha önce kaydedilmiş mi?
    pub is_saved: bool,
}

/// Takdir ekranının tamamı: satırlar + havuz özeti.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HoursBoard {
    pub term: String,
    pub rows: Vec<HoursRow>,
    /// Koordinatörlük toplam ders yükü (havuz): şeflik saatleri + Σ (haftalık
    /// ders saati × grup sayısı) (OÖKY MADDE 88/2-ç). Ayar girilmemişse 0.
    pub pool_hours: i64,
    /// Takdir edilenlerin toplamı.
    pub total_awarded: i64,
    /// Tüm tavanların toplamı — havuzun yetip yetmediğini gösterir.
    pub total_max: i64,
    pub honorary_count: i64,
    pub locked_count: i64,
    /// Kural bulunamayan işletme sayısı.
    pub without_rule_count: i64,
    pub warnings: Vec<String>,
}

async fn load_board(state: &AppState) -> AppResult<HoursBoard> {
    let pool = &state.pool;
    let all_settings = settings::get_all(pool).await?;
    let term = all_settings.get("active_term").cloned().unwrap_or_default();
    // Havuz `settings` ayarlarından değil, döneme bağlı `term_branch_hours`
    // (bkz. migration 0005) ile şeflik projeksiyonundan hesaplanır.
    let as_of = teaching_load::current_as_of(pool, &term).await?;
    let pool_hours = teaching_load::total_pool_hours(pool, &term, as_of).await?;

    // `list` (süzülmüş): saat takdiri havuzu bir yönetim ekranıdır, pasif işletmeye saat takdir edilmez.
    let all_companies = companies::list(pool).await?;
    let rules = hour_rules::list(pool).await?;
    let student_counts: BTreeMap<i64, i64> = students::count_by_company(pool, &term)
        .await?
        .into_iter()
        .collect();
    let saved: BTreeMap<i64, _> = company_hours::list(pool, &term)
        .await?
        .into_iter()
        .map(|row| (row.company_id, row))
        .collect();

    let mut board = HoursBoard {
        term,
        pool_hours,
        ..Default::default()
    };

    for company in all_companies {
        let student_count = student_counts.get(&company.id).copied().unwrap_or(0);
        // Saat kuralları GİDİŞ-DÖNÜŞ mesafeye bakar.
        let round_trip = company.one_way_distance_km.map(|km| km * 2.0);
        let max_hours = round_trip
            .and_then(|km| select_narrowest(&rules, km, student_count))
            .map(|rule| rule.max_hours);

        let existing = saved.get(&company.id);

        board.rows.push(HoursRow {
            company_id: company.id,
            company_name: company.name.clone(),
            address_text: company.address_text.clone(),
            one_way_distance_km: company.one_way_distance_km,
            round_trip_distance_km: round_trip,
            student_count,
            max_hours,
            awarded_hours: existing.map(|row| row.awarded_hours).unwrap_or(0),
            is_honorary: existing.map(|row| row.is_honorary == 1).unwrap_or(false),
            is_locked: existing.map(|row| row.is_locked == 1).unwrap_or(false),
            notes: existing.map(|row| row.notes.clone()).unwrap_or_default(),
            is_saved: existing.is_some(),
        });
    }

    board.rows.sort_by(|a, b| a.company_name.cmp(&b.company_name));

    board.total_awarded = board.rows.iter().map(|r| r.awarded_hours).sum();
    board.total_max = board.rows.iter().filter_map(|r| r.max_hours).sum();
    board.honorary_count = board.rows.iter().filter(|r| r.is_honorary).count() as i64;
    board.locked_count = board.rows.iter().filter(|r| r.is_locked).count() as i64;
    board.without_rule_count = board.rows.iter().filter(|r| r.max_hours.is_none()).count() as i64;

    if board.pool_hours == 0 {
        board.warnings.push(
            "Bu dönem için ders yükü havuzu tanımlanmamış. Takdir edilen saatler bir üst sınırla \
             karşılaştırılamıyor. Önce Ders Yükü ekranından bu dönemin sınıf/dal saatlerini girin."
                .into(),
        );
    } else if board.total_awarded > board.pool_hours {
        // `save_hours`/tarihçe kapısı artık havuzu AŞAN yeni bir kayda izin
        // vermiyor (kullanıcı kuralı "havuz aşılamaz"); bu dal yine de
        // ulaşılabilir çünkü havuzun KENDİSİ sonradan küçültülebilir (ör.
        // Ders Yükü ekranından bir dalın saati düşürülürse, önceden geçerli
        // bir takdir yeni (daha küçük) havuza göre aşkın kalır).
        board.warnings.push(format!(
            "Toplam takdir edilen saat ({}) ders yükü havuzunu ({}) aşıyor!",
            board.total_awarded, board.pool_hours
        ));
    }

    if board.without_rule_count > 0 {
        board.warnings.push(format!(
            "{} işletmenin mesafe ve öğrenci sayısına uyan saat kuralı yok; tavan hesaplanamadı.",
            board.without_rule_count
        ));
    }

    Ok(board)
}

#[tauri::command]
pub async fn get_hours_board(state: State<'_, AppState>) -> AppResult<HoursBoard> {
    load_board(&state).await
}

/// Saatler artık tarihçe kapısından (`change_service::execute_change`) yazılır
/// (brief "MESNET.Lite: pano yazımları tarihçeden geçmiyor"): eski
/// `company_hours::save_many`, `company_term_hours`'a yazıyordu ama tarihçe
/// yalnız `company_hour_periods` projeksiyonunu güncelliyordu — ikisi hiç
/// eşleşmiyordu. Havuz aşımı denetimi (`pool_overrun_reason`) artık yalnız
/// `decide::company::set_company_hours`te yaşar; burada ikinci bir kopyası
/// TUTULMAZ (DRY).
///
/// `effectiveDate`/`reason` isteğe bağlıdır: dönem başlamadıysa boş
/// geçilebilir (varsayılan dönem başıdır); başladıysa `decide` tarih
/// zorunluluğunu (`EffectiveDateRequired`) kendisi uygular.
#[tauri::command]
pub async fn save_company_hours(
    state: State<'_, AppState>,
    rows: Vec<HoursInput>,
    effective_date: Option<String>,
    reason: Option<String>,
) -> AppResult<HoursBoard> {
    let term = settings::get_active_term(&state.pool).await?;
    // `today`, komut sınırında BİR kez hesaplanır (bkz. `services::company_merge`
    // içindeki aynı desen): asıl yazma adımı `today`yi parametre alır, böylece
    // testler gerçek takvim gününe bağlı kalmadan dönem başlangıcı/tarih
    // zorunluluğu senaryolarını sabit bir "bugün" ile sınayabilir.
    save_hours_for_term(&state.pool, &term, &rows, effective_date, reason, today_local()).await?;
    load_board(&state).await
}

/// `save_company_hours`'ın asıl yazma adımı — testte `State<'_, AppState>`
/// kurmadan doğrudan çağrılabilsin diye ayrı tutulur (bkz. `distribute_for_term`'in
/// aynı deseni).
pub(crate) async fn save_hours_for_term(
    pool: &SqlitePool,
    term: &str,
    rows: &[HoursInput],
    effective_date: Option<String>,
    reason: Option<String>,
    today: NaiveDate,
) -> AppResult<()> {
    let effective_date = effective_date.map(|raw| parse_date(&raw)).transpose()?;
    let command_rows = rows.iter().map(to_company_hours_row).collect();
    let request = ChangeRequest {
        term: term.to_string(),
        effective_date,
        document_date: None,
        reason: reason.unwrap_or_default(),
        command: ChangeCommand::SetCompanyHours { rows: command_rows },
    };
    commit_legacy_change(pool, request, today).await
}

/// `HoursInput.maxHoursSnapshot` artık YOK SAYILIR: tavan `decide::company::set_company_hours`
/// içinde `cap_for` ile sunucuda hesaplanır (kullanıcı kuralı: arayüz eski
/// bir tavanla göndermiş olsa bile geçerli kural TEK doğruluk kaynağıdır).
fn to_company_hours_row(input: &HoursInput) -> CompanyHoursRow {
    CompanyHoursRow {
        company_id: input.company_id,
        awarded_hours: input.awarded_hours,
        is_honorary: input.is_honorary,
        is_locked: input.is_locked,
        notes: input.notes.clone(),
    }
}

/// `Otomatik Dağıt` düğmesinin gönderdiği satır.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoDistributeRow {
    pub company_id: i64,
    pub max_hours: i64,
    pub student_count: i64,
    pub is_locked: bool,
    pub current_awarded: i64,
    pub is_honorary: bool,
}

/// Havuzu ağırlığa göre paylaştırır. Sonuç yalnızca öneridir; kaydetmez.
#[tauri::command]
pub async fn auto_distribute_hours(
    state: State<'_, AppState>,
    rows: Vec<AutoDistributeRow>,
) -> AppResult<DistributionOutcome> {
    let term = settings::get_active_term(&state.pool).await?;
    distribute_for_term(&state.pool, &term, rows).await
}

/// Şeflik saatleri dahil TAM havuzu paylaştırır: işletme takdiri havuzdan
/// şeflik saati çıkarılmadan hesaplanır (işletme kararı).
async fn distribute_for_term(
    pool: &sqlx::SqlitePool,
    term: &str,
    rows: Vec<AutoDistributeRow>,
) -> AppResult<DistributionOutcome> {
    let as_of = teaching_load::current_as_of(pool, term).await?;
    let pool_hours = teaching_load::total_pool_hours(pool, term, as_of).await?;

    let candidates: Vec<DistributionCandidate> = rows
        .into_iter()
        .map(|row| DistributionCandidate {
            company_id: row.company_id,
            max_hours: row.max_hours,
            student_count: row.student_count,
            is_locked: row.is_locked,
            current_awarded: row.current_awarded,
            is_honorary: row.is_honorary,
        })
        .collect();

    Ok(distribute(&candidates, pool_hours))
}

/// Ders yükü ekranındaki tek bir satır — ister kayıtlı, ister öneri.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeachingLoadRow {
    /// Kayıtlı satırın kimliği. Öneri satırında yoktur.
    pub id: Option<i64>,
    pub grade: String,
    pub branch: String,
    pub weekly_hours: i64,
    /// ETKİN grup sayısı: elle satırda saklanan değer, otomatik satırda
    /// `auto_group_count` (havuz bu değerle hesaplanır).
    pub group_count: i64,
    /// Öğrenci sayısından Norm Kadro Yön. MADDE 22/1-ç tablosuyla hesaplanan
    /// grup sayısı. Satır elle olsa bile gösterilir; kullanıcı "otomatiğe
    /// dön" derken neye döneceğini görür.
    pub auto_group_count: i64,
    /// Grup sayısı elle mi girildi? Elle değilse öğrenci sayısını izler.
    pub is_group_manual: bool,
    /// Bu satır öğrenci kayıtlarından türetilen bir ÖNERİDİR, henüz
    /// kaydedilmedi — ekran kullanıcıya boru işaretli anahtarı elle
    /// yazdırmamak için bunu önceden doldurur.
    pub is_suggested: bool,
}

/// Ders yükü ekranının tamamı: satırlar + o dönemin havuzu.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TeachingLoadBoard {
    pub term: String,
    pub rows: Vec<TeachingLoadRow>,
    /// Koordinatörlük toplam ders yükü: `branch_hours + chief_planning_hours`
    /// (OÖKY MADDE 88/2-ç, Norm Kadro Yön. MADDE 6/4).
    pub pool_hours: i64,
    /// Σ (haftalık ders saati × etkin grup sayısı) — yalnızca KAYITLI
    /// satırlardan; öneri satırları henüz kaydedilmediği için havuza katkı vermez.
    pub branch_hours: i64,
    /// Alanın tüm şeflerinin planlama-bakım-onarım ek ders saatleri toplamı.
    pub chief_planning_hours: i64,
}

/// Kayıtlı satırlarla öğrenci kayıtlarından türeyen öneri satırlarını
/// birleştirir. Zaten kayıtlı bir (sınıf, dal) çifti için öneri EKLENMEZ.
/// Etkin ve otomatik grup sayıları `student_counts`'tan hesaplanır; kural
/// `teaching_load::effective_group_count`'ta durur, burada tekrarlanmaz.
fn merge_with_suggestions(
    existing: Vec<teaching_load::TermBranchHours>,
    student_pairs: Vec<(String, String)>,
    student_counts: &BranchStudentCounts,
) -> Vec<TeachingLoadRow> {
    let mut known: BTreeSet<(String, String)> = BTreeSet::new();
    let mut rows: Vec<TeachingLoadRow> = existing
        .into_iter()
        .map(|saved| {
            known.insert((saved.grade.clone(), saved.branch.clone()));
            let auto = teaching_load::auto_group_count(student_counts, &saved.grade, &saved.branch);
            TeachingLoadRow {
                id: Some(saved.id),
                weekly_hours: saved.weekly_hours,
                group_count: teaching_load::effective_group_count(&saved, auto),
                auto_group_count: auto,
                is_group_manual: saved.is_group_manual,
                grade: saved.grade,
                branch: saved.branch,
                is_suggested: false,
            }
        })
        .collect();

    for (grade, branch) in student_pairs {
        if known.contains(&(grade.clone(), branch.clone())) {
            continue;
        }
        let auto = teaching_load::auto_group_count(student_counts, &grade, &branch);
        rows.push(TeachingLoadRow {
            id: None,
            grade,
            branch,
            weekly_hours: 0,
            group_count: auto,
            auto_group_count: auto,
            is_group_manual: false,
            is_suggested: true,
        });
    }

    rows.sort_by(|a, b| a.grade.cmp(&b.grade).then_with(|| a.branch.cmp(&b.branch)));
    rows
}

async fn build_teaching_load_board(
    pool: &sqlx::SqlitePool,
    term: &str,
    as_of: NaiveDate,
) -> AppResult<TeachingLoadBoard> {
    let existing = teaching_load::list_for_term(pool, term).await?;
    let student_pairs = teaching_load::distinct_branches_from_students(pool, term).await?;
    let student_counts = teaching_load::branch_student_counts(pool, term).await?;
    let breakdown = teaching_load::pool_breakdown(pool, term, as_of).await?;

    Ok(TeachingLoadBoard {
        term: term.to_string(),
        rows: merge_with_suggestions(existing, student_pairs, &student_counts),
        pool_hours: breakdown.total(),
        branch_hours: breakdown.branch_hours,
        chief_planning_hours: breakdown.chief_planning_hours,
    })
}

/// Aktif dönemin ders yükü satırlarını getirir; henüz satırı olmayan ama
/// öğrencisi olan (sınıf, dal) çiftleri öneri satırı olarak eklenir.
#[tauri::command]
pub async fn get_teaching_load_board(state: State<'_, AppState>) -> AppResult<TeachingLoadBoard> {
    let term = settings::get_active_term(&state.pool).await?;
    let as_of = teaching_load::current_as_of(&state.pool, &term).await?;
    build_teaching_load_board(&state.pool, &term, as_of).await
}

/// Aktif dönemin ders yükü satırlarını TAMAMEN değiştirir (kısmi güncelleme yok).
#[tauri::command]
pub async fn save_teaching_load(
    state: State<'_, AppState>,
    rows: Vec<TermBranchHoursInput>,
) -> AppResult<TeachingLoadBoard> {
    let term = settings::get_active_term(&state.pool).await?;
    teaching_load::replace_for_term(&state.pool, &term, &rows).await?;
    let as_of = teaching_load::current_as_of(&state.pool, &term).await?;
    build_teaching_load_board(&state.pool, &term, as_of).await
}

#[cfg(test)]
#[path = "hours_commands_tests.rs"]
mod tests;
