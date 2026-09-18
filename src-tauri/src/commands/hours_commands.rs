use crate::db::company_hours::HoursInput;
use crate::db::hour_rules::select_narrowest;
use crate::db::{companies, company_hours, hour_rules, settings, students, AppState};
use crate::domain::hour_distribution::{distribute, DistributionCandidate, DistributionOutcome};
use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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

/// Okulun ders yükü havuzu: Σ (sınıf+dal haftalık ders saati × grup sayısı).
///
/// Ayarlar JSON olarak `{"12/C|Elektronik Haberleşme": 24}` biçiminde tutulur.
/// Anahtarlar iki JSON arasında eşleşmelidir; eşleşmeyen anahtar yok sayılır —
/// eksik grup sayısını 1 varsaymak havuzu sessizce şişirirdi.
fn compute_pool_hours(branch_weekly_hours: &str, branch_group_counts: &str) -> i64 {
    let hours: BTreeMap<String, i64> =
        serde_json::from_str(branch_weekly_hours).unwrap_or_default();
    let groups: BTreeMap<String, i64> =
        serde_json::from_str(branch_group_counts).unwrap_or_default();

    hours
        .iter()
        .filter_map(|(key, weekly)| groups.get(key).map(|count| weekly * count))
        .sum()
}

fn pool_from_settings(all_settings: &BTreeMap<String, String>) -> i64 {
    compute_pool_hours(
        all_settings
            .get("branch_weekly_hours")
            .map(String::as_str)
            .unwrap_or("{}"),
        all_settings
            .get("branch_group_counts")
            .map(String::as_str)
            .unwrap_or("{}"),
    )
}

async fn load_board(state: &AppState) -> AppResult<HoursBoard> {
    let pool = &state.pool;
    let all_settings = settings::get_all(pool).await?;
    let term = all_settings.get("active_term").cloned().unwrap_or_default();
    let pool_hours = pool_from_settings(&all_settings);

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
            "Bu okul için ders yükü havuzu tanımlanmamış. Takdir edilen saatler bir üst sınırla \
             karşılaştırılamıyor. Önce Ayarlar ekranından dal ve grup saatlerini girin."
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
    let all_settings = settings::get_all(&state.pool).await?;
    let pool_hours = pool_from_settings(&all_settings);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_is_sum_of_weekly_hours_times_group_counts() {
        let hours = r#"{"12/C|Elektronik Haberleşme": 24, "12/D|Endüstriyel Bakım Onarım": 24}"#;
        let groups = r#"{"12/C|Elektronik Haberleşme": 2, "12/D|Endüstriyel Bakım Onarım": 1}"#;

        assert_eq!(compute_pool_hours(hours, groups), 24 * 2 + 24);
    }

    /// Bir tarafta olup diğerinde olmayan anahtar yok sayılır;
    /// eksik grup sayısı sessizce 1 varsayılmaz.
    #[test]
    fn keys_missing_from_either_side_are_ignored() {
        let hours = r#"{"12/C|Dal": 24, "12/D|Dal": 24}"#;
        let groups = r#"{"12/C|Dal": 2}"#;

        assert_eq!(compute_pool_hours(hours, groups), 48);
    }

    /// Ayar girilmemişse havuz 0'dır; bu "sınırsız" değil "bilinmiyor" demektir.
    #[test]
    fn empty_settings_produce_zero_pool() {
        assert_eq!(compute_pool_hours("{}", "{}"), 0);
    }

    /// Bozuk JSON okuma yolunu düşürmemeli.
    #[test]
    fn invalid_json_is_treated_as_empty() {
        assert_eq!(compute_pool_hours("bozuk", "{}"), 0);
        assert_eq!(compute_pool_hours(r#"{"a": 1}"#, "bozuk"), 0);
    }
}
