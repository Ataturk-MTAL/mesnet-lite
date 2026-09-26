use crate::db::{settings, teaching_load, terms, AppState};
use crate::error::{AppError, AppResult};
use crate::services::commission_minutes;
use serde::Serialize;
use std::collections::BTreeMap;
use tauri::State;

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> AppResult<BTreeMap<String, String>> {
    settings::get_all(&state.pool).await
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    entries: BTreeMap<String, String>,
) -> AppResult<BTreeMap<String, String>> {
    save_settings_in_pool(&state.pool, &entries).await
}

/// `save_settings`'in gerçek mantığı (bkz. `create_term_in_pool` ile aynı
/// ayrım). Komisyon tutanağının müdür/alan adı ayarları başlığa basıldığından
/// yazmadan önce doğrulanır; tek geçersiz değer bütün kaydı reddeder.
///
/// Kenar çubuğundaki dönem seçici serbest metin kabul eder ve `create_term`'i
/// atlayarak doğrudan `active_term` ayarını yazabilir (bilinen kusur — bkz.
/// brief). Bu yüzden yazımdan sonra `terms::ensure` çağrılır: kaydedilen
/// dönem `terms` tablosunda yoksa varsayılan tarihlerle açılır, aksi hâlde
/// `teaching_load::current_as_of` gibi dönem tarihine bağımlı her sorgu
/// "Dönem bulunamadı" ile kırılır.
async fn save_settings_in_pool(
    pool: &sqlx::SqlitePool,
    entries: &BTreeMap<String, String>,
) -> AppResult<BTreeMap<String, String>> {
    commission_minutes::validate_minutes_settings(entries)?;
    settings::set_many(pool, entries).await?;
    if let Some(active_term) = entries.get("active_term") {
        terms::ensure(pool, active_term).await?;
    }
    settings::get_all(pool).await
}

/// Okul konumunu haritadan gelen değerle yazar.
#[tauri::command]
pub async fn set_school_location(
    state: State<'_, AppState>,
    latitude: f64,
    longitude: f64,
) -> AppResult<()> {
    validate_coordinates(latitude, longitude)?;
    settings::set_school_location(&state.pool, latitude, longitude).await
}

fn validate_coordinates(latitude: f64, longitude: f64) -> AppResult<()> {
    if !(-90.0..=90.0).contains(&latitude) {
        return Err(AppError::Validation(
            "Enlem -90 ile 90 arasında olmalı".into(),
        ));
    }
    if !(-180.0..=180.0).contains(&longitude) {
        return Err(AppError::Validation(
            "Boylam -180 ile 180 arasında olmalı".into(),
        ));
    }
    Ok(())
}

/// Bilinen tüm dönemler: döneme bağlı her tablonun birleşimi + aktif dönem.
/// Yeni bir eğitim-öğretim yılının henüz öğrencisi olmayabilir; bu yüzden
/// dönem yönetimi ekranı yalnızca öğrencisi olan dönemleri değil, bunu
/// kullanmalıdır.
#[tauri::command]
pub async fn get_known_terms(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    settings::known_terms(&state.pool).await
}

/// Dönem biçimini doğrular: `YYYY-YYYY/N`.
///
/// İkinci yıl birincinin bir fazlası olmalı (eğitim-öğretim yılı tek bir takvim
/// yılında bitmez) ve N güz/bahar dönemini ayırt eden 1 veya 2 olmalı. Serbest
/// metin kutusunun izin verdiği "2026" veya "2026-2028/1" gibi biçimler burada
/// reddedilir.
fn validate_term_format(term: &str) -> AppResult<()> {
    let invalid = || {
        AppError::Validation(format!(
            "Dönem biçimi \"{term}\" geçersiz. Beklenen biçim: YYYY-YYYY/N (ör. 2026-2027/1) \
             — ikinci yıl birincinin bir fazlası, N ise 1 veya 2 olmalı."
        ))
    };

    let (years, period) = term.split_once('/').ok_or_else(invalid)?;
    let (first_year, second_year) = years.split_once('-').ok_or_else(invalid)?;

    if first_year.len() != 4 || second_year.len() != 4 {
        return Err(invalid());
    }

    let first: i32 = first_year.parse().map_err(|_| invalid())?;
    let second: i32 = second_year.parse().map_err(|_| invalid())?;
    let number: i32 = period.parse().map_err(|_| invalid())?;

    if second != first + 1 || (number != 1 && number != 2) {
        return Err(invalid());
    }

    Ok(())
}

/// `create_term` komutunun sonucu: dönem zaten var mıydı ve ders yükü
/// devrinden ne kadarı gerçekten kopyalandı.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTermResult {
    pub term: String,
    /// Dönem çağrıdan önce de bilinen dönemler arasındaysa true.
    /// Bu bir hata değildir; mevcut dönem sessizce kullanılır.
    pub already_existed: bool,
    /// `copy_teaching_load_from_term` verildiyse ve hedefte satır yoksa
    /// kopyalanan satır sayısı; aksi hâlde 0.
    pub teaching_load_rows_copied: i64,
    /// Kopyalama istendiği hâlde hedef dönemde zaten satır olduğu için
    /// atlandıysa true — mevcut veri asla üzerine yazılmaz.
    pub teaching_load_copy_skipped: bool,
}

/// Yeni bir dönem kaydeder. `SettingsView.vue`'daki serbest metin kutusunun
/// yerini alır: biçim burada doğrulanır, dönem hiçbir tabloya veri
/// eklenmeden de "oluşturulabilir" (kullanıcı önce dönemi açar, verileri
/// sonra girer).
///
/// `copy_teaching_load_from_term` verilirse, kaynak dönemin `term_branch_hours`
/// satırları hedefe kopyalanır — ama yalnızca hedefte zaten satır YOKSA;
/// `teaching_load::copy_term` ile aynı "üzerine yazma" kuralı burada da
/// geçerlidir (bkz. `availability.rs::copy_term`).
#[tauri::command]
pub async fn create_term(
    state: State<'_, AppState>,
    term: String,
    copy_teaching_load_from_term: Option<String>,
) -> AppResult<CreateTermResult> {
    create_term_in_pool(&state.pool, term, copy_teaching_load_from_term).await
}

/// `create_term`'ün gerçek mantığı. `tauri::State` bir Tauri çalışma zamanı
/// olmadan test edilemediği için, komut ince bir sarmalayıcıdır ve mantık
/// düz bir `SqlitePool` üzerinde çalışır (bkz. `hours_commands.rs::load_board`
/// ile aynı ayrım).
async fn create_term_in_pool(
    pool: &sqlx::SqlitePool,
    term: String,
    copy_teaching_load_from_term: Option<String>,
) -> AppResult<CreateTermResult> {
    validate_term_format(&term)?;

    let known = settings::known_terms(pool).await?;
    let already_existed = known.iter().any(|known_term| known_term == &term);

    // `terms` satırı olmadan dönem yalnızca "bilinir" görünür, tarihe bağımlı
    // her sorgu (`teaching_load::current_as_of` vb.) "Dönem bulunamadı" ile
    // kırılır. İdempotent: dönem zaten bilinse bile `ensure` mevcut satıra
    // dokunmaz (bkz. `terms::ensure_in`).
    terms::ensure(pool, &term).await?;

    let (teaching_load_rows_copied, teaching_load_copy_skipped) =
        match copy_teaching_load_from_term {
            Some(from_term) if from_term == term => {
                return Err(AppError::Validation(
                    "Kaynak ve hedef dönem aynı olamaz".into(),
                ));
            }
            Some(from_term) => {
                let source_rows = teaching_load::list_for_term(pool, &from_term).await?;
                if source_rows.is_empty() {
                    (0, false)
                } else {
                    let copied = teaching_load::copy_term(pool, &from_term, &term).await?;
                    if copied {
                        (source_rows.len() as i64, false)
                    } else {
                        (0, true)
                    }
                }
            }
            None => (0, false),
        };

    Ok(CreateTermResult {
        term,
        already_existed,
        teaching_load_rows_copied,
        teaching_load_copy_skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;
    use sqlx::SqlitePool;

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    #[test]
    fn rejects_out_of_range_coordinates() {
        assert!(validate_coordinates(91.0, 34.0).is_err());
        assert!(validate_coordinates(36.0, 181.0).is_err());
        assert!(validate_coordinates(-91.0, 34.0).is_err());
    }

    #[test]
    fn accepts_mersin_coordinates() {
        assert!(validate_coordinates(36.8121, 34.6415).is_ok());
    }

    #[test]
    fn accepts_a_well_formed_term() {
        assert!(validate_term_format("2026-2027/1").is_ok());
        assert!(validate_term_format("2027-2028/2").is_ok());
    }

    #[test]
    fn rejects_a_second_year_that_is_not_one_more_than_the_first() {
        assert!(validate_term_format("2026-2028/1").is_err());
        assert!(validate_term_format("2026-2026/1").is_err());
    }

    #[test]
    fn rejects_a_period_number_outside_one_or_two() {
        assert!(validate_term_format("2026-2027/0").is_err());
        assert!(validate_term_format("2026-2027/3").is_err());
    }

    #[test]
    fn rejects_free_text_and_malformed_terms() {
        assert!(validate_term_format("2026").is_err());
        assert!(validate_term_format("2026-2027").is_err());
        assert!(validate_term_format("26-27/1").is_err());
        assert!(validate_term_format("2026-2027/1/2").is_err());
        assert!(validate_term_format("bozuk").is_err());
    }

    /// Tutanak başlığına basılan iki isteğe bağlı ayar kaydetme sınırında
    /// doğrulanır; reddedilen kayıtta hiçbir anahtar yazılmaz.
    #[tokio::test]
    async fn save_settings_rejects_a_multiline_principal_name_and_writes_nothing() {
        let (_dir, pool) = test_pool().await;
        let mut entries = BTreeMap::new();
        entries.insert("school_name".to_string(), "Yeni Okul".to_string());
        entries.insert("principal_name".to_string(), "Ömer\nYiğit".to_string());

        let err = save_settings_in_pool(&pool, &entries).await.unwrap_err();

        assert!(matches!(err, AppError::Validation(_)));
        let all = settings::get_all(&pool).await.unwrap();
        assert_ne!(all.get("school_name").map(String::as_str), Some("Yeni Okul"));
        assert!(!all.contains_key("principal_name"));
    }

    #[tokio::test]
    async fn save_settings_stores_and_returns_the_minutes_settings() {
        let (_dir, pool) = test_pool().await;
        let mut entries = BTreeMap::new();
        entries.insert("principal_name".to_string(), "Ömer Yiğit".to_string());
        entries.insert("field_name".to_string(), "Elektrik-Elektronik Teknolojisi".to_string());

        let all = save_settings_in_pool(&pool, &entries).await.unwrap();

        assert_eq!(all.get("principal_name").map(String::as_str), Some("Ömer Yiğit"));
        assert_eq!(
            all.get("field_name").map(String::as_str),
            Some("Elektrik-Elektronik Teknolojisi")
        );
    }

    #[tokio::test]
    async fn create_term_rejects_bad_format_without_touching_the_database() {
        let (_dir, pool) = test_pool().await;
        let err = create_term_in_pool(&pool, "bozuk".into(), None).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn create_term_reports_when_the_term_is_new() {
        let (_dir, pool) = test_pool().await;
        let result = create_term_in_pool(&pool, "2027-2028/1".into(), None).await.unwrap();

        assert!(!result.already_existed);
        assert_eq!(result.teaching_load_rows_copied, 0);
        assert!(!result.teaching_load_copy_skipped);
    }

    /// Zaten bilinen bir dönemi yeniden oluşturmak hata değildir; mevcut
    /// dönem sessizce kullanılır ama çağırana bildirilir.
    #[tokio::test]
    async fn create_term_silently_reuses_an_already_known_term() {
        let (_dir, pool) = test_pool().await;
        // Seed'deki aktif dönem zaten "2026-2027/1".
        let result = create_term_in_pool(&pool, "2026-2027/1".into(), None).await.unwrap();
        assert!(result.already_existed);
    }

    #[tokio::test]
    async fn create_term_copies_teaching_load_rows_from_the_source_term() {
        let (_dir, pool) = test_pool().await;
        teaching_load::replace_for_term(
            &pool,
            "2026-2027/1",
            &[teaching_load::TermBranchHoursInput {
                grade: "12/C".into(),
                branch: "Elektronik Haberleşme".into(),
                weekly_hours: 24,
                group_count: 2,
                is_group_manual: true,
            }],
        )
        .await
        .unwrap();

        let result = create_term_in_pool(
            &pool,
            "2027-2028/1".into(),
            Some("2026-2027/1".into()),
        )
        .await
        .unwrap();

        assert_eq!(result.teaching_load_rows_copied, 1);
        assert!(!result.teaching_load_copy_skipped);
        assert_eq!(
            teaching_load::list_for_term(&pool, "2027-2028/1").await.unwrap().len(),
            1
        );
    }

    /// Hedef dönemde zaten satır varsa devir ÜZERİNE YAZMAZ; bu, mevcut
    /// verinin kaybolmayacağının garantisidir.
    #[tokio::test]
    async fn create_term_skips_copy_when_target_already_has_rows() {
        let (_dir, pool) = test_pool().await;
        teaching_load::replace_for_term(
            &pool,
            "2026-2027/1",
            &[teaching_load::TermBranchHoursInput {
                grade: "12/C".into(),
                branch: "Kaynak Dal".into(),
                weekly_hours: 24,
                group_count: 2,
                is_group_manual: true,
            }],
        )
        .await
        .unwrap();
        teaching_load::replace_for_term(
            &pool,
            "2027-2028/1",
            &[teaching_load::TermBranchHoursInput {
                grade: "12/D".into(),
                branch: "Zaten Var Olan Dal".into(),
                weekly_hours: 10,
                group_count: 1,
                is_group_manual: true,
            }],
        )
        .await
        .unwrap();

        let result = create_term_in_pool(
            &pool,
            "2027-2028/1".into(),
            Some("2026-2027/1".into()),
        )
        .await
        .unwrap();

        assert_eq!(result.teaching_load_rows_copied, 0);
        assert!(result.teaching_load_copy_skipped);
        let target = teaching_load::list_for_term(&pool, "2027-2028/1").await.unwrap();
        assert_eq!(target.len(), 1);
        assert_eq!(target[0].branch, "Zaten Var Olan Dal", "mevcut satır korunmalı");
    }

    /// Kök neden testi: `create_term` yalnızca dönemi "bilindik" yapmakla
    /// kalmamalı, `terms` tablosuna da bir satır yazmalı — aksi hâlde
    /// `teaching_load::current_as_of` gibi tarihe bağımlı her sorgu
    /// "Dönem bulunamadı" ile kırılır (bkz. brief teşhisi).
    #[tokio::test]
    async fn create_term_creates_a_terms_row_with_default_dates() {
        let (_dir, pool) = test_pool().await;

        create_term_in_pool(&pool, "2027-2028/1".into(), None).await.unwrap();

        let mut conn = pool.acquire().await.unwrap();
        let fetched = terms::get_in(&mut conn, "2027-2028/1").await.unwrap();
        let expected = crate::domain::terms::TermDates::default_for("2027-2028/1");
        assert_eq!(fetched.start, expected.start);
        assert_eq!(fetched.end, expected.end);
        assert!(!fetched.dates_confirmed);
    }

    /// Regresyon: `create_term` ile açılan bir dönem, tarihe bağımlı
    /// sorgular için hemen kullanılabilir olmalı — 0006'dan sonra açılan
    /// hiçbir dönemde bu böyle DEĞİLDİ.
    #[tokio::test]
    async fn create_term_lets_teaching_load_current_as_of_resolve_the_new_term() {
        let (_dir, pool) = test_pool().await;

        create_term_in_pool(&pool, "2027-2028/1".into(), None).await.unwrap();

        let as_of = teaching_load::current_as_of(&pool, "2027-2028/1").await.unwrap();
        assert!(as_of >= crate::domain::terms::TermDates::default_for("2027-2028/1").start);
    }

    /// `create_term` zaten bilinen bir dönemi yeniden çağırdığında,
    /// kullanıcının önceden onayladığı tarihlere ASLA dokunmamalı.
    #[tokio::test]
    async fn create_term_does_not_touch_confirmed_dates_of_an_existing_term() {
        let (_dir, pool) = test_pool().await;
        let confirmed = crate::domain::terms::TermDates {
            term: "2026-2027/1".into(),
            start: ymd(2026, 9, 15),
            end: ymd(2027, 1, 20),
            dates_confirmed: true,
        };
        {
            let mut conn = pool.acquire().await.unwrap();
            sqlx::query("UPDATE terms SET start_date = ?1, end_date = ?2, dates_confirmed = 1 WHERE term = ?3")
                .bind(confirmed.start)
                .bind(confirmed.end)
                .bind(&confirmed.term)
                .execute(&mut *conn)
                .await
                .unwrap();
        }

        create_term_in_pool(&pool, "2026-2027/1".into(), None).await.unwrap();

        let mut conn = pool.acquire().await.unwrap();
        let fetched = terms::get_in(&mut conn, "2026-2027/1").await.unwrap();
        assert_eq!(fetched, confirmed);
    }

    /// Yeni bir dönemi görünür kılan üçüncü yol: kenar çubuğundaki dönem
    /// seçicisi `save_settings` ile doğrudan `active_term` yazar,
    /// `create_term`'i hiç ÇAĞIRMAZ (bkz. brief teşhisi).
    #[tokio::test]
    async fn save_settings_writing_a_new_active_term_creates_a_terms_row() {
        let (_dir, pool) = test_pool().await;
        let mut entries = BTreeMap::new();
        entries.insert("active_term".to_string(), "2027-2028/2".to_string());

        save_settings_in_pool(&pool, &entries).await.unwrap();

        let mut conn = pool.acquire().await.unwrap();
        let fetched = terms::get_in(&mut conn, "2027-2028/2").await.unwrap();
        assert!(!fetched.dates_confirmed);
    }

    fn ymd(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// `list_terms_with_dates`'in temeli `terms::list`tir; yeni açılan
    /// dönem satırı oluşmadan orada hiç görünemezdi (bkz. brief teşhisi).
    #[tokio::test]
    async fn create_term_makes_the_new_term_appear_in_terms_list() {
        let (_dir, pool) = test_pool().await;

        create_term_in_pool(&pool, "2027-2028/1".into(), None).await.unwrap();

        let listed = terms::list(&pool).await.unwrap();
        assert!(listed.iter().any(|t| t.term == "2027-2028/1"));
    }

    #[tokio::test]
    async fn create_term_rejects_copying_from_itself() {
        let (_dir, pool) = test_pool().await;
        let err = create_term_in_pool(
            &pool,
            "2027-2028/1".into(),
            Some("2027-2028/1".into()),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }
}
