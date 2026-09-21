use crate::db::company_hours::HoursInput;
use crate::db::hour_rules::select_narrowest;
use crate::db::teaching_load::{self, BranchStudentCounts, TermBranchHoursInput};
use crate::db::{companies, company_hours, hour_rules, settings, students, AppState};
use crate::domain::hour_distribution::{distribute, pool_overrun_reason, DistributionCandidate, DistributionOutcome};
use crate::error::{AppError, AppResult};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
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

#[tauri::command]
pub async fn save_company_hours(
    state: State<'_, AppState>,
    rows: Vec<HoursInput>,
) -> AppResult<HoursBoard> {
    let term = settings::get_active_term(&state.pool).await?;
    save_hours_for_term(&state.pool, &term, &rows).await?;
    load_board(&state).await
}

/// `save_company_hours`'ın havuz denetimli asıl yazma adımı — testte
/// `State<'_, AppState>` kurmadan doğrudan çağrılabilsin diye ayrı tutulur
/// (bkz. `distribute_for_term`'in aynı deseni).
async fn save_hours_for_term(pool: &sqlx::SqlitePool, term: &str, rows: &[HoursInput]) -> AppResult<()> {
    reject_if_pool_overrun(pool, term, rows).await?;
    company_hours::save_many(pool, term, rows).await?;
    Ok(())
}

/// Kullanıcı kuralı "havuz aşılamaz": doğrudan kaydetme yolu, tarihçe
/// kapısındaki `decide::company::set_company_hours` ile AYNI kuralı
/// (`hour_distribution::pool_overrun_reason`) kullanır — ikisi ayrı kopya
/// tutarsa biri unutulur (DRY). `rows`'ta OLMAYAN işletmelerin kayıtlı
/// takdiri değişmeden kalır; bu yüzden eski/yeni toplam hesaplanırken
/// `rows`'taki işletmeler DB'deki eski değerleriyle değil, gönderilen yeni
/// değerleriyle sayılır.
async fn reject_if_pool_overrun(pool: &sqlx::SqlitePool, term: &str, rows: &[HoursInput]) -> AppResult<()> {
    let as_of = teaching_load::current_as_of(pool, term).await?;
    let pool_hours = teaching_load::total_pool_hours(pool, term, as_of).await?;

    let touched: BTreeSet<i64> = rows.iter().map(|r| r.company_id).collect();
    let existing = company_hours::list(pool, term).await?;
    let old_total: i64 = existing.iter().map(|e| e.awarded_hours).sum();
    let untouched_total: i64 = existing
        .iter()
        .filter(|e| !touched.contains(&e.company_id))
        .map(|e| e.awarded_hours)
        .sum();
    let new_rows_total: i64 = rows.iter().map(|r| if r.is_honorary { 0 } else { r.awarded_hours }).sum();

    if let Some(reason) = pool_overrun_reason(old_total, untouched_total + new_rows_total, pool_hours) {
        return Err(AppError::Validation(reason));
    }
    Ok(())
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
mod tests {
    use super::*;
    use crate::db::init_pool;
    use crate::db::teaching_load_test_support::{seed_teacher, ymd, TERM};
    use crate::domain::models::ChiefType;
    use sqlx::SqlitePool;
    use teaching_load::TermBranchHours;

    fn saved(id: i64, grade: &str, branch: &str, weekly: i64, groups: i64) -> TermBranchHours {
        TermBranchHours {
            id,
            term: "2026-2027/1".into(),
            grade: grade.into(),
            branch: branch.into(),
            weekly_hours: weekly,
            group_count: groups,
            is_group_manual: true,
            created_at: "2026-09-01T00:00:00Z".into(),
            updated_at: "2026-09-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn merge_marks_saved_rows_as_not_suggested() {
        let existing = vec![saved(1, "12/C", "Elektronik Haberleşme", 24, 2)];
        let rows = merge_with_suggestions(existing, vec![], &BranchStudentCounts::new());

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, Some(1));
        assert!(!rows[0].is_suggested);
    }

    /// Öğrencisi olup henüz satırı olmayan çift öneri olarak eklenir.
    #[test]
    fn merge_adds_suggestion_for_branch_without_a_saved_row() {
        let rows = merge_with_suggestions(
            vec![],
            vec![("12/D".to_string(), "Endüstriyel Bakım Onarım".to_string())],
            &BranchStudentCounts::new(),
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, None);
        assert!(rows[0].is_suggested);
        assert_eq!(rows[0].weekly_hours, 0);
        assert_eq!(rows[0].group_count, 0);
    }

    /// Zaten kayıtlı bir (sınıf, dal) için ikinci bir öneri satırı EKLENMEZ.
    #[test]
    fn merge_does_not_duplicate_a_branch_that_already_has_a_saved_row() {
        let existing = vec![saved(1, "12/C", "Elektronik Haberleşme", 24, 2)];
        let rows = merge_with_suggestions(
            existing,
            vec![("12/C".to_string(), "Elektronik Haberleşme".to_string())],
            &BranchStudentCounts::new(),
        );

        assert_eq!(rows.len(), 1, "aynı çift için ikinci satır eklenmemeli");
        assert!(!rows[0].is_suggested);
    }

    #[test]
    fn merge_sorts_rows_by_grade_then_branch() {
        let rows = merge_with_suggestions(
            vec![],
            vec![
                ("12/D".to_string(), "Dal B".to_string()),
                ("12/C".to_string(), "Dal A".to_string()),
            ],
            &BranchStudentCounts::new(),
        );

        assert_eq!(rows[0].grade, "12/C");
        assert_eq!(rows[1].grade, "12/D");
    }

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    /// 24×2 + 24×1 = 72 saatlik Σ ve alan şefi (10) + atölye şefi (6) = 16.
    async fn pool_with_chiefs() -> (tempfile::TempDir, SqlitePool) {
        let (dir, pool) = test_pool().await;
        let input = |grade: &str, weekly, groups| TermBranchHoursInput {
            grade: grade.into(),
            branch: "Dal".into(),
            weekly_hours: weekly,
            group_count: groups,
            is_group_manual: true,
        };
        teaching_load::replace_for_term(&pool, TERM, &[input("12/C", 24, 2), input("12/D", 24, 1)])
            .await
            .unwrap();
        seed_teacher(&pool, "Alan", ChiefType::Department).await;
        seed_teacher(&pool, "Atolye", ChiefType::WorkshopLab).await;
        (dir, pool)
    }

    /// Ders yükü tahtası: `poolHours` toplamdır, iki kalemi ayrı da verir.
    #[tokio::test]
    async fn teaching_load_board_pool_is_branch_hours_plus_chief_hours() {
        let (_dir, pool) = pool_with_chiefs().await;

        let board = build_teaching_load_board(&pool, TERM, ymd(2026, 10, 1)).await.unwrap();

        assert_eq!(board.branch_hours, 72);
        assert_eq!(board.chief_planning_hours, 16);
        assert_eq!(board.pool_hours, 88);
        assert_eq!(board.branch_hours + board.chief_planning_hours, board.pool_hours);
    }

    /// İşletme takdir tahtası ve aşım uyarısı TAM havuza (şeflik dahil) bakar.
    #[tokio::test]
    async fn hours_board_pool_includes_chief_hours() {
        let (_dir, pool) = pool_with_chiefs().await;

        let board = load_board(&AppState { pool }).await.unwrap();

        assert_eq!(board.pool_hours, 72 + 16);
    }

    /// Otomatik dağıtım tam havuzu paylaştırır: tek işletmenin tavanı havuzdan
    /// büyükse dağıtılan saat şeflik dahil toplam havuza eşit olmalı.
    #[tokio::test]
    async fn auto_distribute_shares_the_full_pool_including_chief_hours() {
        let (_dir, pool) = pool_with_chiefs().await;
        let rows = vec![AutoDistributeRow {
            company_id: 1,
            max_hours: 1000,
            student_count: 5,
            is_locked: false,
            current_awarded: 0,
            is_honorary: false,
        }];

        let outcome = distribute_for_term(&pool, TERM, rows).await.unwrap();

        assert_eq!(outcome.distributed_hours, 72 + 16);
    }

    /// Şeflik yokken havuz yalnız Σ(saat × grup); kayıtlı satırlar bozulmaz.
    #[tokio::test]
    async fn teaching_load_board_pool_is_only_the_branch_sum_without_chiefs() {
        let (_dir, pool) = test_pool().await;
        teaching_load::replace_for_term(
            &pool,
            TERM,
            &[TermBranchHoursInput { grade: "12/C".into(), branch: "Dal".into(), weekly_hours: 24, group_count: 2, is_group_manual: true }],
        )
        .await
        .unwrap();

        let board = build_teaching_load_board(&pool, TERM, ymd(2026, 10, 1)).await.unwrap();

        assert_eq!((board.branch_hours, board.chief_planning_hours, board.pool_hours), (48, 0, 48));
    }

    // --- Otomatik grup sayısı (Norm Kadro Yön. MADDE 22/1-ç, migration 0007) ---

    fn counts(entries: &[(&str, &str, i64)]) -> BranchStudentCounts {
        entries
            .iter()
            .map(|(grade, branch, n)| ((grade.to_string(), branch.to_string()), *n))
            .collect()
    }

    fn auto_input(grade: &str, branch: &str, weekly: i64) -> TermBranchHoursInput {
        TermBranchHoursInput {
            grade: grade.into(),
            branch: branch.into(),
            weekly_hours: weekly,
            group_count: 99, // otomatik satırda yok sayılmalı
            is_group_manual: false,
        }
    }

    async fn n_students(pool: &SqlitePool, grade: &str, branch: &str, count: usize) {
        for _ in 0..count {
            crate::db::students::create(
                pool,
                &crate::domain::models::NewStudent {
                    first_name: "Test".into(),
                    last_name: "Ogrenci".into(),
                    student_no: None,
                    grade: grade.into(),
                    branch: branch.into(),
                    company_id: None,
                    submitted_at: None,
                    term: TERM.into(),
                },
            )
            .await
            .unwrap();
        }
    }

    /// Öneri satırı (kayıtsız): grup sayısı tablodan gelir, saat 0 kalır.
    #[test]
    fn merge_suggestion_carries_the_automatic_group_count_and_zero_hours() {
        let rows = merge_with_suggestions(
            vec![],
            vec![("12/D".to_string(), "Dal B".to_string())],
            &counts(&[("12/D", "Dal B", 17)]),
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].group_count, 2);
        assert_eq!(rows[0].auto_group_count, 2);
        assert!(!rows[0].is_group_manual);
        assert_eq!(rows[0].weekly_hours, 0);
    }

    /// Otomatik kayıtlı satır: `groupCount` etkin (hesaplanan) değerdir,
    /// saklanan önbellek değil.
    #[test]
    fn merge_automatic_saved_row_shows_the_computed_group_count_not_the_cache() {
        let mut stored = saved(1, "12/C", "Dal A", 24, 1);
        stored.is_group_manual = false;

        let rows = merge_with_suggestions(vec![stored], vec![], &counts(&[("12/C", "Dal A", 20)]));

        assert_eq!(rows[0].group_count, 2, "önbellek 1 ama 20 öğrenci ⇒ 2 grup");
        assert_eq!(rows[0].auto_group_count, 2);
        assert!(!rows[0].is_group_manual);
    }

    /// Elle kayıtlı satır: `groupCount` saklanan değerdir; `autoGroupCount`
    /// yine tablodan hesaplanıp gösterilir ("otomatiğe dön" önizlemesi).
    #[test]
    fn merge_manual_saved_row_keeps_the_stored_count_and_still_reports_the_auto_one() {
        let rows = merge_with_suggestions(
            vec![saved(1, "12/C", "Dal A", 24, 3)],
            vec![],
            &counts(&[("12/C", "Dal A", 16)]),
        );

        assert_eq!(rows[0].group_count, 3);
        assert_eq!(rows[0].auto_group_count, 1);
        assert!(rows[0].is_group_manual);
    }

    /// Gerçek senaryo tahtada: 12/C 16 öğrenci, 12/D'de iki dal 7 ve 9 öğrenci,
    /// hepsi otomatik, ders saati 24 ⇒ Σ 72; atölye şefi 6 ile toplam 78.
    #[tokio::test]
    async fn teaching_load_board_real_scenario_is_72_branch_hours_and_78_in_total() {
        let (_dir, pool) = test_pool().await;
        n_students(&pool, "12/C", "Elektronik Haberleşme", 16).await;
        n_students(&pool, "12/D", "Endüstriyel Bakım Onarım", 7).await;
        n_students(&pool, "12/D", "Bilişim Teknolojileri", 9).await;
        teaching_load::replace_for_term(
            &pool,
            TERM,
            &[
                auto_input("12/C", "Elektronik Haberleşme", 24),
                auto_input("12/D", "Endüstriyel Bakım Onarım", 24),
                auto_input("12/D", "Bilişim Teknolojileri", 24),
            ],
        )
        .await
        .unwrap();
        seed_teacher(&pool, "Atolye", ChiefType::WorkshopLab).await;

        let board = build_teaching_load_board(&pool, TERM, ymd(2026, 10, 1)).await.unwrap();

        assert_eq!((board.branch_hours, board.chief_planning_hours, board.pool_hours), (72, 6, 78));
        assert!(board.rows.iter().all(|r| r.group_count == 1 && r.auto_group_count == 1));
        assert!(board.rows.iter().all(|r| !r.is_group_manual && !r.is_suggested));
    }

    /// Kaydedilmemiş öneri satırı `autoGroupCount` ile dolar ama saati 0
    /// olduğu için havuza katkı vermez.
    #[tokio::test]
    async fn suggested_rows_are_filled_with_the_auto_count_and_add_nothing_to_the_pool() {
        let (_dir, pool) = test_pool().await;
        n_students(&pool, "12/C", "Dal A", 20).await;

        let board = build_teaching_load_board(&pool, TERM, ymd(2026, 10, 1)).await.unwrap();

        assert_eq!(board.rows.len(), 1);
        let suggested = &board.rows[0];
        assert!(suggested.is_suggested && !suggested.is_group_manual);
        assert_eq!((suggested.group_count, suggested.auto_group_count), (2, 2));
        assert_eq!(suggested.weekly_hours, 0);
        assert_eq!((board.branch_hours, board.pool_hours), (0, 0));
    }

    /// Otomatik kayıtlı satır tahtada öğrenci sayısını izler; elle satır izlemez.
    #[tokio::test]
    async fn board_shows_automatic_rows_following_students_and_manual_rows_fixed() {
        let (_dir, pool) = test_pool().await;
        n_students(&pool, "12/C", "Dal A", 16).await;
        n_students(&pool, "12/D", "Dal B", 16).await;
        let mut manual = auto_input("12/D", "Dal B", 24);
        manual.is_group_manual = true;
        manual.group_count = 3;
        teaching_load::replace_for_term(&pool, TERM, &[auto_input("12/C", "Dal A", 24), manual])
            .await
            .unwrap();
        n_students(&pool, "12/C", "Dal A", 1).await;
        n_students(&pool, "12/D", "Dal B", 1).await;

        let board = build_teaching_load_board(&pool, TERM, ymd(2026, 10, 1)).await.unwrap();

        assert_eq!(board.rows[0].group_count, 2, "12/C otomatik: 17 öğrenci ⇒ 2");
        assert_eq!(board.rows[1].group_count, 3, "12/D elle: 3 korunur");
        assert_eq!(board.rows[1].auto_group_count, 2);
        assert_eq!(board.branch_hours, 24 * 2 + 24 * 3);
    }

    // --- Havuz aşımı: `save_company_hours` yolu (kullanıcı kuralı "havuz aşılamaz") ---

    async fn a_company(pool: &SqlitePool, name: &str) -> i64 {
        companies::create(
            pool,
            &crate::domain::models::NewCompany {
                name: name.into(),
                contact_first_name: String::new(),
                contact_last_name: String::new(),
                phone: String::new(),
                email: String::new(),
                address_text: "Test adres".into(),
                latitude: None,
                longitude: None,
                one_way_distance_km: Some(5.0),
                notes: String::new(),
            },
        )
        .await
        .unwrap()
        .id
    }

    /// Tek dal satırıyla havuzu istenen değere sabitler (grup 1, saat = havuz).
    async fn set_pool_hours(pool: &SqlitePool, hours: i64) {
        teaching_load::replace_for_term(
            pool,
            TERM,
            &[TermBranchHoursInput { grade: "12/C".into(), branch: "Dal".into(), weekly_hours: hours, group_count: 1, is_group_manual: true }],
        )
        .await
        .unwrap();
    }

    fn hours_input(company_id: i64, awarded: i64) -> HoursInput {
        HoursInput { company_id, max_hours_snapshot: 1000, awarded_hours: awarded, is_honorary: false, is_locked: false, notes: String::new() }
    }

    /// Havuz 100, İşletme A zaten 90 almış; B'ye +20 vermek toplamı 110'a
    /// çıkarır — reddedilir, HİÇBİR şey yazılmaz.
    #[tokio::test]
    async fn save_company_hours_rejects_when_pool_would_be_exceeded() {
        let (_dir, pool) = test_pool().await;
        set_pool_hours(&pool, 100).await;
        let a = a_company(&pool, "İşletme A").await;
        let b = a_company(&pool, "İşletme B").await;
        company_hours::save_many(&pool, TERM, &[hours_input(a, 90)]).await.unwrap();

        let err = save_hours_for_term(&pool, TERM, &[hours_input(b, 20)]).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
        assert_eq!(company_hours::total_awarded(&pool, TERM).await.unwrap(), 90, "reddedilen istekte hiçbir şey yazılmamalı");
    }

    /// Tam havuza eşitlemek (90 + 10 = 100) SINIR DAHİL kabul edilir.
    #[tokio::test]
    async fn save_company_hours_accepts_reaching_the_pool_exactly() {
        let (_dir, pool) = test_pool().await;
        set_pool_hours(&pool, 100).await;
        let a = a_company(&pool, "İşletme A").await;
        let b = a_company(&pool, "İşletme B").await;
        company_hours::save_many(&pool, TERM, &[hours_input(a, 90)]).await.unwrap();

        save_hours_for_term(&pool, TERM, &[hours_input(b, 10)]).await.unwrap();
        assert_eq!(company_hours::total_awarded(&pool, TERM).await.unwrap(), 100);
    }

    /// Havuz tanımlanmamışsa (`0`) aşım denetimi hiç yapılmaz.
    #[tokio::test]
    async fn save_company_hours_skips_the_pool_check_when_the_pool_is_undefined() {
        let (_dir, pool) = test_pool().await;
        let a = a_company(&pool, "İşletme A").await;
        let large = HoursInput { max_hours_snapshot: 10_000, ..hours_input(a, 10_000) };

        save_hours_for_term(&pool, TERM, &[large]).await.unwrap();
        assert_eq!(company_hours::total_awarded(&pool, TERM).await.unwrap(), 10_000);
    }

    /// Aşımı AZALTAN bir düzenleme (100 → 80, havuz 80) kabul edilir.
    #[tokio::test]
    async fn save_company_hours_accepts_an_edit_that_reduces_the_total() {
        let (_dir, pool) = test_pool().await;
        set_pool_hours(&pool, 80).await;
        let a = a_company(&pool, "İşletme A").await;
        company_hours::save_many(&pool, TERM, &[hours_input(a, 100)]).await.unwrap();

        save_hours_for_term(&pool, TERM, &[hours_input(a, 80)]).await.unwrap();
        assert_eq!(company_hours::total_awarded(&pool, TERM).await.unwrap(), 80);
    }

    /// Göçten kalma veri zaten havuzu aşmışsa (60 > 50), aşımı ARTIRMAYAN bir
    /// düzenleme kilitlenmez; aşımı BÜYÜTEN bir düzenleme yine reddedilir.
    #[tokio::test]
    async fn save_company_hours_does_not_lock_a_pre_existing_overrun_but_still_rejects_a_further_increase() {
        let (_dir, pool) = test_pool().await;
        set_pool_hours(&pool, 50).await;
        let a = a_company(&pool, "İşletme A").await;
        company_hours::save_many(&pool, TERM, &[hours_input(a, 60)]).await.unwrap();

        save_hours_for_term(&pool, TERM, &[hours_input(a, 60)])
            .await
            .expect("mevcut aşımı korumak kilitlenmemeli");

        let err = save_hours_for_term(&pool, TERM, &[hours_input(a, 65)]).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)), "aşımı büyüten değişiklik yine reddedilmeli");
    }
}
