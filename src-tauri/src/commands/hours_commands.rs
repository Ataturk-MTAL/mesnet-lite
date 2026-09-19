use crate::db::company_hours::HoursInput;
use crate::db::hour_rules::select_narrowest;
use crate::db::teaching_load::{self, TermBranchHoursInput};
use crate::db::{companies, company_hours, hour_rules, settings, students, AppState};
use crate::domain::hour_distribution::{distribute, DistributionCandidate, DistributionOutcome};
use crate::error::AppResult;
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
    /// Okulun ders yükü havuzu (MADDE 15/2). Ayar girilmemişse 0.
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
    // Havuz artık `settings` ayarlarından değil, döneme bağlı
    // `term_branch_hours` tablosundan hesaplanır (bkz. migration 0005).
    let pool_hours = teaching_load::pool_hours_for_term(pool, &term).await?;

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
    company_hours::save_many(&state.pool, &term, &rows).await?;
    load_board(&state).await
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
    let pool_hours = teaching_load::pool_hours_for_term(&state.pool, &term).await?;

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
    pub group_count: i64,
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
    /// Σ (haftalık ders saati × grup sayısı) — yalnızca KAYITLI satırlardan;
    /// öneri satırları henüz kaydedilmediği için havuza katkı vermez.
    pub pool_hours: i64,
}

/// Kayıtlı satırlarla öğrenci kayıtlarından türeyen öneri satırlarını
/// birleştirir. Zaten kayıtlı bir (sınıf, dal) çifti için öneri EKLENMEZ.
fn merge_with_suggestions(
    existing: Vec<teaching_load::TermBranchHours>,
    student_pairs: Vec<(String, String)>,
) -> Vec<TeachingLoadRow> {
    let mut known: BTreeSet<(String, String)> = BTreeSet::new();
    let mut rows: Vec<TeachingLoadRow> = existing
        .into_iter()
        .map(|saved| {
            known.insert((saved.grade.clone(), saved.branch.clone()));
            TeachingLoadRow {
                id: Some(saved.id),
                grade: saved.grade,
                branch: saved.branch,
                weekly_hours: saved.weekly_hours,
                group_count: saved.group_count,
                is_suggested: false,
            }
        })
        .collect();

    for (grade, branch) in student_pairs {
        if known.contains(&(grade.clone(), branch.clone())) {
            continue;
        }
        rows.push(TeachingLoadRow {
            id: None,
            grade,
            branch,
            weekly_hours: 0,
            group_count: 0,
            is_suggested: true,
        });
    }

    rows.sort_by(|a, b| a.grade.cmp(&b.grade).then_with(|| a.branch.cmp(&b.branch)));
    rows
}

async fn build_teaching_load_board(
    pool: &sqlx::SqlitePool,
    term: &str,
) -> AppResult<TeachingLoadBoard> {
    let existing = teaching_load::list_for_term(pool, term).await?;
    let student_pairs = teaching_load::distinct_branches_from_students(pool, term).await?;
    let pool_hours = teaching_load::pool_hours_for_term(pool, term).await?;

    Ok(TeachingLoadBoard {
        term: term.to_string(),
        rows: merge_with_suggestions(existing, student_pairs),
        pool_hours,
    })
}

/// Aktif dönemin ders yükü satırlarını getirir; henüz satırı olmayan ama
/// öğrencisi olan (sınıf, dal) çiftleri öneri satırı olarak eklenir.
#[tauri::command]
pub async fn get_teaching_load_board(state: State<'_, AppState>) -> AppResult<TeachingLoadBoard> {
    let term = settings::get_active_term(&state.pool).await?;
    build_teaching_load_board(&state.pool, &term).await
}

/// Aktif dönemin ders yükü satırlarını TAMAMEN değiştirir (kısmi güncelleme yok).
#[tauri::command]
pub async fn save_teaching_load(
    state: State<'_, AppState>,
    rows: Vec<TermBranchHoursInput>,
) -> AppResult<TeachingLoadBoard> {
    let term = settings::get_active_term(&state.pool).await?;
    teaching_load::replace_for_term(&state.pool, &term, &rows).await?;
    build_teaching_load_board(&state.pool, &term).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use teaching_load::TermBranchHours;

    fn saved(id: i64, grade: &str, branch: &str, weekly: i64, groups: i64) -> TermBranchHours {
        TermBranchHours {
            id,
            term: "2026-2027/1".into(),
            grade: grade.into(),
            branch: branch.into(),
            weekly_hours: weekly,
            group_count: groups,
            created_at: "2026-09-01T00:00:00Z".into(),
            updated_at: "2026-09-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn merge_marks_saved_rows_as_not_suggested() {
        let existing = vec![saved(1, "12/C", "Elektronik Haberleşme", 24, 2)];
        let rows = merge_with_suggestions(existing, vec![]);

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
        );

        assert_eq!(rows[0].grade, "12/C");
        assert_eq!(rows[1].grade, "12/D");
    }
}
