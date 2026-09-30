use crate::db::read_at::ReadAt;
use crate::db::{settings, teachers, AppState};
use crate::domain::history::decide::{ChangeCommand, ChangeRequest, ImpactSummary, NewTeacherProfile};
use crate::domain::history::events::TeacherLoad;
use crate::domain::models::{NewTeacher, Teacher};
use crate::domain::terms::today_local;
use crate::domain::workload::{statutory_cap, MANAGEMENT_MAX_EXTRA_HOURS, teacher_capacity, InstitutionType};
use crate::error::{AppError, AppResult};
use crate::services::change_input::{chief_column, employment_column};
use crate::services::change_service::{execute_change, ChangeMode, ChangeOutcome};
use chrono::NaiveDate;
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::State;

/// `create_teacher` gerekçe vermezse yazılan varsayılan.
/// Gerekçe geldiğinde (arayüzden `reason`) onun yerine geçer.
const DEFAULT_CREATE_REASON: &str = "Öğretmen ekranından eklendi";

/// Öğretmen + mevzuattan türetilen kapasite bilgisi.
/// Kapasite hesabı yalnızca Rust tarafında yapılır; arayüz onu yeniden hesaplamaz.
///
/// `teacher`in kimlik alanları `teachers` tablosundan gelir; yük alanları
/// (`chiefType`, `baseHours`, `maxExtraHours`, `otherExtraHours`,
/// `employmentType`) ise okunan günün `teacher_load_periods` PROJEKSİYONUNDAN
/// doldurulur. Yük tarihçe ekranından (`commit_change` ile doğrudan
/// `SetTeacherLoad`) değiştiğinde eski sütunlar güncellenmez; arayüz unvanı
/// buradan okuduğu için eski sütuna bakmak unvanı donuk gösterirdi. JSON
/// alan adları değişmez (`flatten`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeacherWithCapacity {
    #[serde(flatten)]
    pub teacher: Teacher,
    /// MADDE 6/4: bölüm şefi 10, atölye/laboratuvar şefi 6, şef değilse 0.
    pub chief_hours: i64,
    /// Kişinin bağlı olduğu yasal tavan (MADDE 15/2; müdür/müdür yrd. için
    /// ayrıca MADDE 6/1-a).
    pub statutory_cap: i64,
    /// Koordinatörlük için kalan haftalık saat.
    pub capacity: i64,
}

/// Okul tavanı (MADDE 15/2) ile müdür/müdür yardımcısının ek ders sınırının
/// (MADDE 6/1-a, 6 saat) küçüğü; diğer öğretmenlerde okul tavanı aynen kalır.
fn personal_statutory_cap(school_cap: i64, load: &TeacherLoad) -> i64 {
    if load.chief_type.is_school_management() {
        school_cap.min(MANAGEMENT_MAX_EXTRA_HOURS)
    } else {
        school_cap
    }
}

#[tauri::command]
pub async fn list_teachers(state: State<'_, AppState>) -> AppResult<Vec<Teacher>> {
    teachers::list(&state.pool).await
}

/// Öğretmenleri kapasiteleriyle birlikte döner. Kapasite ayarlardaki okul
/// tipine, büyükşehir durumuna VE `teacher_load_periods` projeksiyonundaki
/// o günkü yüke bağlı olduğu için burada hesaplanır (spec R5c: kapasite
/// okuyan hiçbir yer artık eski `teachers` yük sütunlarına bakmaz).
async fn list_teachers_with_capacity_impl(
    pool: &SqlitePool,
    read_at: &ReadAt,
) -> AppResult<Vec<TeacherWithCapacity>> {
    let all = settings::get_all(pool).await?;
    let institution_type = InstitutionType::parse(
        all.get("institution_type").map(String::as_str).unwrap_or("other"),
    );
    let is_metropolitan = all
        .get("is_metropolitan_district")
        .map(|v| v == "true")
        .unwrap_or(false);
    let cap = statutory_cap(institution_type, is_metropolitan);

    let term = settings::get_active_term(pool).await?;
    let hours_as_of = read_at.hours_as_of(pool, &term).await?;
    let rows = teachers::list_with_load_as_of(pool, &term, hours_as_of).await?;

    Ok(rows
        .into_iter()
        .map(|entry| {
            let chief_hours = entry.load.chief_type.weekly_hours();
            let capacity = teacher_capacity(&entry.load, cap);
            let personal_cap = personal_statutory_cap(cap, &entry.load);
            let teacher = Teacher {
                chief_type: chief_column(entry.load.chief_type).to_string(),
                employment_type: employment_column(entry.load.employment_type).to_string(),
                base_hours: entry.load.base_hours,
                max_extra_hours: entry.load.max_extra_hours,
                other_extra_hours: entry.load.other_extra_hours,
                ..entry.teacher
            };
            TeacherWithCapacity {
                teacher,
                chief_hours,
                statutory_cap: personal_cap,
                capacity,
            }
        })
        .collect())
}

/// `asOf` eksikse `ReadAt::Latest`; verilirse aktif dönemin aralığında
/// olması zorunludur (spec §6).
#[tauri::command]
pub async fn list_teachers_with_capacity(
    state: State<'_, AppState>,
    as_of: Option<String>,
) -> AppResult<Vec<TeacherWithCapacity>> {
    let term = settings::get_active_term(&state.pool).await?;
    let read_at = ReadAt::resolve(&state.pool, &term, as_of).await?;
    list_teachers_with_capacity_impl(&state.pool, &read_at).await
}

/// `NewTeacher`in kimlik kısmını `createTeacher.teacher` gövdesine çevirir.
fn profile_from_input(input: &NewTeacher) -> NewTeacherProfile {
    NewTeacherProfile {
        first_name: input.first_name.clone(),
        last_name: input.last_name.clone(),
        registry_no: input.registry_no.clone(),
        field: input.field.clone(),
        branches: input.branches.clone(),
        is_active: input.is_active,
    }
}

/// `NewTeacher`in yük kısmını `TeacherLoad`a çevirir (`change_input.rs`daki
/// TERS dönüşümün karşılığı).
fn load_from_input(input: &NewTeacher) -> TeacherLoad {
    TeacherLoad {
        base_hours: input.base_hours,
        max_extra_hours: input.max_extra_hours,
        other_extra_hours: input.other_extra_hours,
        chief_type: teachers::parse_chief_type(&input.chief_type),
        employment_type: teachers::parse_employment_type(&input.employment_type),
    }
}

/// Tek değişiklik kapısından geçirir; `Rejected`/`Stale` kullanıcıya
/// `Validation` olarak iletilir (spec §5, `availability_commands::commit_schedule_change`
/// ile aynı desen). Başarılıysa etki özetini döner — `create_teacher_impl`
/// yeni öğretmenin kimliğini buradan okur.
async fn commit_teacher_change(
    pool: &SqlitePool,
    term: &str,
    command: ChangeCommand,
    effective_date: Option<NaiveDate>,
    reason: Option<String>,
    default_reason: &str,
    today: NaiveDate,
) -> AppResult<ImpactSummary> {
    let request = ChangeRequest {
        term: term.to_string(),
        effective_date,
        document_date: None,
        reason: reason.unwrap_or_else(|| default_reason.to_string()),
        command,
    };
    match execute_change(pool, request, ChangeMode::Commit { expected_high_water: None }, today).await? {
        ChangeOutcome::Committed { impact, .. } => Ok(impact),
        ChangeOutcome::Rejected { reason, .. } => Err(AppError::Validation(reason)),
        // Bu komutlar bayat denetimi istemez (`expected_high_water: None`);
        // yine de gelirse kullanıcıya olduğu gibi iletilir.
        ChangeOutcome::Stale { message } => Err(AppError::Validation(message)),
        ChangeOutcome::Preview { .. } => Err(AppError::Database(
            "Kayıt sırasında beklenmeyen önizleme sonucu döndü".into(),
        )),
    }
}

/// Yalnız kimlik alanlarını doğrular. Yük tutarlılığı (MADDE 6/1-c, azami ek
/// ders şeflik saatinden küçük olamaz) artık TEK yerde,
/// `services/change_input.rs::validate_load`dadır; `create_teacher_impl`
/// zaten kapıdan geçtiği için burada tekrarlanmaz (DRY).
fn validate_identity(input: &NewTeacherProfile) -> AppResult<()> {
    if input.first_name.trim().is_empty() || input.last_name.trim().is_empty() {
        return Err(AppError::Validation("Ad ve soyad boş olamaz".into()));
    }
    if input.field.trim().is_empty() {
        return Err(AppError::Validation("Alan boş olamaz".into()));
    }
    Ok(())
}

/// Yeni öğretmeni tek değişiklik kapısından oluşturur (spec R5c): olay
/// günlüğüne yazar, projeksiyonda satır açar, eski `teachers` tablosuna
/// `change_input::materialize` üzerinden yalnız BİR kez yazar.
async fn create_teacher_impl(
    pool: &SqlitePool,
    input: NewTeacher,
    effective_date: Option<NaiveDate>,
    reason: Option<String>,
    today: NaiveDate,
) -> AppResult<Teacher> {
    let term = settings::get_active_term(pool).await?;
    let command = ChangeCommand::CreateTeacher {
        teacher: profile_from_input(&input),
        load: load_from_input(&input),
    };
    let impact =
        commit_teacher_change(pool, &term, command, effective_date, reason, DEFAULT_CREATE_REASON, today).await?;
    let teacher_id = impact
        .primary
        .first()
        .map(|line| line.subject_id)
        .ok_or_else(|| AppError::Database("Öğretmen oluşturuldu ama etki özetinde kimlik bulunamadı".into()))?;
    teachers::get(pool, teacher_id).await
}

/// Öğretmenin yalnız KİMLİK alanlarını (ad, soyad, sicil, alan, branşlar,
/// aktiflik) günceller. Yük/şeflik alanlarına ve tarihçeye (olay günlüğü,
/// projeksiyon) dokunmaz: yük yalnız tarihçe (`SetTeacherLoad`) yolundan
/// değişir. Düzenleme penceresi yükü göstermediği için burada yük yazmak,
/// kullanıcının görmediği değerlerle ileri tarihli planlı değişikliği
/// sessizce ezerdi (issue #29). `is_active` bugün olduğu gibi tarihçe olayı
/// üretmeden doğrudan yazılır.
async fn update_teacher_impl(pool: &SqlitePool, id: i64, input: NewTeacherProfile) -> AppResult<Teacher> {
    validate_identity(&input)?;
    teachers::update_profile(pool, id, &input).await
}

#[tauri::command]
pub async fn create_teacher(
    state: State<'_, AppState>,
    input: NewTeacher,
    effective_date: Option<NaiveDate>,
    reason: Option<String>,
) -> AppResult<Teacher> {
    create_teacher_impl(&state.pool, input, effective_date, reason, today_local()).await
}

#[tauri::command]
pub async fn update_teacher(state: State<'_, AppState>, id: i64, input: NewTeacherProfile) -> AppResult<Teacher> {
    update_teacher_impl(&state.pool, id, input).await
}

#[tauri::command]
pub async fn delete_teacher(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    teachers::remove(&state.pool, id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;
    use crate::db::teaching_load;

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    const TEST_REASON: &str = "Test kurulumu";

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// Dönem (2026-09-01) başlamadan önce: tarih sorulmaz.
    fn planning_today() -> NaiveDate {
        ymd(2026, 8, 15)
    }

    /// Dönem başladı: yürürlük tarihi zorunlu.
    fn november_today() -> NaiveDate {
        ymd(2026, 11, 10)
    }

    fn valid_input(last_name: &str, chief_type: &str) -> NewTeacher {
        NewTeacher {
            first_name: "Test".into(),
            last_name: last_name.into(),
            registry_no: String::new(),
            field: "Elektrik-Elektronik Teknolojisi".into(),
            branches: vec!["Elektronik Haberleşme".into()],
            employment_type: "tenured".into(),
            base_hours: 20,
            max_extra_hours: 24,
            other_extra_hours: 0,
            chief_type: chief_type.into(),
            is_active: true,
        }
    }

    async fn count(pool: &SqlitePool, table: &str, kind: Option<&str>) -> i64 {
        let sql = match kind {
            Some(_) => format!("SELECT COUNT(*) FROM {table} WHERE kind = ?1"),
            None => format!("SELECT COUNT(*) FROM {table}"),
        };
        let mut query = sqlx::query_scalar(&sql);
        if let Some(kind) = kind {
            query = query.bind(kind);
        }
        query.fetch_one(pool).await.unwrap()
    }

    #[test]
    fn rejects_blank_names_and_field() {
        let mut input = profile_from_input(&valid_input("Ogretmen", "none"));
        input.first_name = " ".into();
        assert!(validate_identity(&input).is_err());

        let mut input = profile_from_input(&valid_input("Ogretmen", "none"));
        input.field = String::new();
        assert!(validate_identity(&input).is_err());
    }

    #[test]
    fn accepts_valid_identity() {
        assert!(validate_identity(&profile_from_input(&valid_input("Ogretmen", "none"))).is_ok());
    }

    /// Asıl regresyon: `create_teacher`, olay günlüğünden ve projeksiyondan
    /// GEÇMEDEN yalnız eski tabloya yazıyordu; bu yüzden şeflik saati havuzu
    /// (`chief_planning_hours`) yeni öğretmenleri hiç görmüyordu.
    #[tokio::test]
    async fn create_teacher_writes_through_the_change_log_and_the_pool_sees_it_immediately() {
        let (_dir, pool) = test_pool().await;

        let created = create_teacher_impl(&pool, valid_input("Sef", "department"), None, None, planning_today())
            .await
            .unwrap();

        assert_eq!(count(&pool, "change_sets", Some("create_teacher")).await, 1, "olay yoluyla yazılmalı");
        let as_of = ymd(2026, 10, 1);
        let hours = teaching_load::chief_planning_hours(&pool, crate::db::teaching_load_test_support::TERM, as_of)
            .await
            .unwrap();
        assert_eq!(hours, 10, "MADDE 6/4: bölüm şefi 10 saat, havuz hemen görmeli");
        assert_eq!(created.chief_type, "department");
    }

    /// Öğretmen oluşturma sırasında yük tutarsızsa (MADDE 6/1-c) kapı
    /// reddeder; kural burada TEKRAR yazılmaz, gate'in `validate_load`ı
    /// devreye girer.
    #[tokio::test]
    async fn create_teacher_rejects_inconsistent_load_via_the_gate() {
        let (_dir, pool) = test_pool().await;
        // Taze havuzda bile `change_sets` boş DEĞİLDİR: migration 0006,
        // aktif dönem için bir 'opening' kümesi tohumlar. Bu yüzden mutlak
        // sıfır yerine ÖNCESİ/SONRASI karşılaştırılır.
        let sets_before = count(&pool, "change_sets", None).await;
        let mut input = valid_input("Tutarsiz", "department");
        input.max_extra_hours = 8; // 10'dan (bölüm şefi) küçük

        let result = create_teacher_impl(&pool, input, None, None, planning_today()).await;

        assert!(matches!(result, Err(AppError::Validation(_))), "Validation beklenirdi: {result:?}");
        assert_eq!(count(&pool, "change_sets", None).await, sets_before, "reddedilince hiçbir şey yazılmamalı");
    }

    /// Kimlik güncellemesi olay günlüğüne hiç uğramaz; yük yalnız tarihçe
    /// (`SetTeacherLoad`) yolundan değişir.
    #[tokio::test]
    async fn updating_identity_fields_does_not_touch_the_change_log() {
        let (_dir, pool) = test_pool().await;
        let created = create_teacher_impl(&pool, valid_input("Isim", "none"), None, None, planning_today())
            .await
            .unwrap();
        let sets_before = count(&pool, "change_sets", None).await;

        let mut profile = profile_from_input(&valid_input("YeniIsim", "none"));
        profile.first_name = "Yeni".into();
        let updated = update_teacher_impl(&pool, created.id, profile).await.unwrap();

        assert_eq!(updated.first_name, "Yeni");
        assert_eq!(count(&pool, "change_sets", None).await, sets_before, "olay yazılmamalı");
    }

    /// Issue #29: düzenleme penceresi yükü göstermez; ad değiştirmek ileri
    /// tarihli planlı yük değişikliğini (burada 2026-11-01'de müdür yardımcısı,
    /// MADDE 6/1-a: 6 saat) ezmemeli.
    #[tokio::test]
    async fn renaming_keeps_a_future_dated_planned_load_change() {
        let (_dir, pool) = test_pool().await;
        let created = create_teacher_impl(&pool, valid_input("Plan", "none"), None, None, planning_today())
            .await
            .unwrap();
        let mut deputy = valid_input("Plan", "deputy_principal");
        deputy.max_extra_hours = 6;
        commit_teacher_change(
            &pool,
            TERM,
            ChangeCommand::SetTeacherLoad { teacher_id: created.id, load: load_from_input(&deputy) },
            Some(ymd(2026, 11, 1)),
            None,
            TEST_REASON,
            planning_today(),
        )
        .await
        .unwrap();
        let events_before = count(&pool, "change_sets", Some("set_teacher_load")).await;

        let mut profile = profile_from_input(&valid_input("YeniAd", "none"));
        profile.first_name = "Yeni".into();
        let updated = update_teacher_impl(&pool, created.id, profile).await.unwrap();

        assert_eq!(updated.last_name, "YeniAd");
        assert_eq!(count(&pool, "change_sets", Some("set_teacher_load")).await, events_before);
        let future: (String, String, i64) = sqlx::query_as(
            "SELECT valid_from, chief_type, max_extra_hours FROM teacher_load_periods
             WHERE teacher_id = ?1 AND valid_to IS NULL",
        )
        .bind(created.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(future, ("2026-11-01".to_string(), "deputy_principal".to_string(), 6));
    }

    /// Kilit test: `list_teachers_with_capacity`in kapasitesi projeksiyondan
    /// gelir, eski `teachers.chief_type` sütunu DEĞİŞMESE bile. Yük yalnız
    /// kapıdan (`commit_teacher_change`, planlama evresinde tarih vermeden)
    /// değiştirilir; okuma `current_as_of`in kullandığı GERÇEK bugünü
    /// kullandığından, yürürlük tarihi dönem başında (2026-09-01) kalır ki
    /// test hangi gün koşulursa koşulsun görünür olsun.
    #[tokio::test]
    async fn list_teachers_with_capacity_follows_the_projection_after_a_load_only_change() {
        let (_dir, pool) = test_pool().await;
        settings::set(&pool, "institution_type", "other").await.unwrap();
        settings::set(&pool, "is_metropolitan_district", "true").await.unwrap();
        let created = create_teacher_impl(&pool, valid_input("Sef", "none"), None, None, planning_today())
            .await
            .unwrap();

        let before = list_teachers_with_capacity_impl(&pool, &ReadAt::Latest).await.unwrap();
        assert_eq!(before[0].chief_hours, 0);

        let term = crate::db::teaching_load_test_support::TERM.to_string();
        commit_teacher_change(
            &pool,
            &term,
            ChangeCommand::SetTeacherLoad { teacher_id: created.id, load: load_from_input(&valid_input("Sef", "department")) },
            None,
            None,
            TEST_REASON,
            planning_today(),
        )
        .await
        .unwrap();

        let after = list_teachers_with_capacity_impl(&pool, &ReadAt::Latest).await.unwrap();
        assert_eq!(after[0].chief_hours, 10, "kapasite projeksiyondaki yeni şeflikten okunmalı");
        assert_eq!(
            teachers::get(&pool, created.id).await.unwrap().chief_type,
            "none",
            "eski sütun donuk kalmalı"
        );
    }

    /// Unvan hücresi `teacher.chiefType`ten okunur; yük yalnız kapıdan
    /// (eski sütunu yazmayan yol) değiştiğinde de projeksiyonu yansıtmalıdır.
    /// Müdür yardımcısı: MADDE 6/1-a, azami ek ders 6 saat.
    #[tokio::test]
    async fn list_teachers_with_capacity_reports_load_fields_from_the_projection_not_the_legacy_columns() {
        let (_dir, pool) = test_pool().await;
        let created = create_teacher_impl(&pool, valid_input("Yrd", "none"), None, None, planning_today())
            .await
            .unwrap();
        let mut deputy = valid_input("Yrd", "deputy_principal");
        deputy.max_extra_hours = 6;
        deputy.base_hours = 18;
        deputy.other_extra_hours = 2;
        deputy.employment_type = "contracted".into();
        let term = TERM.to_string();
        commit_teacher_change(
            &pool,
            &term,
            ChangeCommand::SetTeacherLoad { teacher_id: created.id, load: load_from_input(&deputy) },
            None,
            None,
            TEST_REASON,
            planning_today(),
        )
        .await
        .unwrap();

        let rows = list_teachers_with_capacity_impl(&pool, &ReadAt::Latest).await.unwrap();

        assert_eq!(rows[0].teacher.chief_type, "deputy_principal");
        assert_eq!(rows[0].teacher.max_extra_hours, 6);
        assert_eq!(rows[0].teacher.base_hours, 18);
        assert_eq!(rows[0].teacher.other_extra_hours, 2);
        assert_eq!(rows[0].teacher.employment_type, "contracted");
        assert_eq!(teachers::get(&pool, created.id).await.unwrap().chief_type, "none", "eski sütun donuk");
    }

    /// As-of: değişiklikten ÖNCEKİ gün eski unvan (`none`), sonrası yeni unvan.
    #[tokio::test]
    async fn list_teachers_with_capacity_as_of_reports_the_title_in_force_on_that_day() {
        let (_dir, pool) = test_pool().await;
        let created = create_teacher_impl(&pool, valid_input("Yrd", "none"), None, None, planning_today())
            .await
            .unwrap();
        let mut deputy = valid_input("Yrd", "deputy_principal");
        deputy.max_extra_hours = 6;
        commit_teacher_change(
            &pool,
            TERM,
            ChangeCommand::SetTeacherLoad { teacher_id: created.id, load: load_from_input(&deputy) },
            Some(ymd(2026, 9, 14)),
            None,
            TEST_REASON,
            planning_today(),
        )
        .await
        .unwrap();

        let before = ReadAt::resolve(&pool, TERM, Some("2026-09-10".into())).await.unwrap();
        let rows_before = list_teachers_with_capacity_impl(&pool, &before).await.unwrap();
        assert_eq!(rows_before[0].teacher.chief_type, "none");
        assert_eq!(rows_before[0].teacher.max_extra_hours, 24);

        let after = ReadAt::resolve(&pool, TERM, Some("2026-09-15".into())).await.unwrap();
        let rows_after = list_teachers_with_capacity_impl(&pool, &after).await.unwrap();
        assert_eq!(rows_after[0].teacher.chief_type, "deputy_principal");
        assert_eq!(rows_after[0].teacher.max_extra_hours, 6);
    }

    /// MADDE 6/1-a: müdür yardımcısının kişisel tavanı 6'dır (okul tavanı değil);
    /// düz öğretmende okul tavanı (MADDE 15/2) aynen kalır.
    #[tokio::test]
    async fn statutory_cap_is_six_for_school_management_and_the_school_cap_for_others() {
        let (_dir, pool) = test_pool().await;
        settings::set(&pool, "institution_type", "other").await.unwrap();
        settings::set(&pool, "is_metropolitan_district", "true").await.unwrap();
        let school_cap = {
            let rows = list_teachers_with_capacity_impl(&pool, &ReadAt::Latest).await.unwrap();
            assert!(rows.is_empty());
            statutory_cap(InstitutionType::Other, true)
        };
        assert!(school_cap > MANAGEMENT_MAX_EXTRA_HOURS, "test ayırt edici olmalı");
        create_teacher_impl(&pool, valid_input("Duz", "none"), None, None, planning_today()).await.unwrap();
        let mut deputy = valid_input("Yrd", "deputy_principal");
        deputy.max_extra_hours = 6;
        create_teacher_impl(&pool, deputy, None, None, planning_today()).await.unwrap();

        let rows = list_teachers_with_capacity_impl(&pool, &ReadAt::Latest).await.unwrap();

        let by_name = |n: &str| rows.iter().find(|r| r.teacher.last_name == n).unwrap();
        assert_eq!(by_name("Duz").statutory_cap, school_cap);
        assert_eq!(by_name("Yrd").statutory_cap, MANAGEMENT_MAX_EXTRA_HOURS);
    }

    /// Kurulum: `none`/24 öğretmen, kapıdan `deputy_principal`/6 (eski sütun donuk).
    async fn frozen_legacy_deputy(pool: &SqlitePool) -> i64 {
        let created = create_teacher_impl(pool, valid_input("Eski", "none"), None, None, planning_today())
            .await
            .unwrap();
        let mut deputy = valid_input("Eski", "deputy_principal");
        deputy.max_extra_hours = 6;
        commit_teacher_change(
            pool,
            TERM,
            ChangeCommand::SetTeacherLoad { teacher_id: created.id, load: load_from_input(&deputy) },
            None,
            None,
            TEST_REASON,
            planning_today(),
        )
        .await
        .unwrap();
        created.id
    }

    /// Eski sütun donuk (`none`/24), projeksiyon müdür yardımcısı/6: ad
    /// güncellemesi ikisine de dokunmaz, yeni yük olayı üretilmez.
    #[tokio::test]
    async fn update_leaves_the_projection_and_the_legacy_load_columns_untouched() {
        let (_dir, pool) = test_pool().await;
        let id = frozen_legacy_deputy(&pool).await;
        let sets_before = count(&pool, "change_sets", Some("set_teacher_load")).await;

        let profile = profile_from_input(&valid_input("YeniAd", "none"));
        let updated = update_teacher_impl(&pool, id, profile).await.unwrap();

        assert_eq!(updated.last_name, "YeniAd");
        assert_eq!(updated.chief_type, "none", "eski sütun yazılmamalı");
        assert_eq!(updated.max_extra_hours, 24, "eski sütun yazılmamalı");
        assert_eq!(count(&pool, "change_sets", Some("set_teacher_load")).await, sets_before);
        let rows = list_teachers_with_capacity_impl(&pool, &ReadAt::Latest).await.unwrap();
        assert_eq!(rows[0].teacher.chief_type, "deputy_principal");
        assert_eq!(rows[0].teacher.max_extra_hours, 6);
    }

    /// `is_active` kimlik alanıdır: tarihçe olayı üretmeden doğrudan yazılır.
    #[tokio::test]
    async fn update_writes_is_active_without_a_history_event() {
        let (_dir, pool) = test_pool().await;
        let created = create_teacher_impl(&pool, valid_input("Aktif", "none"), None, None, planning_today())
            .await
            .unwrap();
        let sets_before = count(&pool, "change_sets", None).await;

        let mut profile = profile_from_input(&valid_input("Aktif", "none"));
        profile.is_active = false;
        let updated = update_teacher_impl(&pool, created.id, profile).await.unwrap();

        assert_eq!(updated.is_active, 0);
        assert_eq!(count(&pool, "change_sets", None).await, sets_before);
    }

    /// Kimlik doğrulaması korunur: boş ad reddedilir, hiçbir şey yazılmaz.
    #[tokio::test]
    async fn update_rejects_a_blank_name() {
        let (_dir, pool) = test_pool().await;
        let created = create_teacher_impl(&pool, valid_input("Ada", "none"), None, None, planning_today())
            .await
            .unwrap();

        let mut profile = profile_from_input(&valid_input("Ada", "none"));
        profile.first_name = " ".into();
        let result = update_teacher_impl(&pool, created.id, profile).await;

        assert!(matches!(result, Err(AppError::Validation(_))), "{result:?}");
        assert_eq!(teachers::get(&pool, created.id).await.unwrap().first_name, "Test");
    }

    // --- R5 spec §6: `list_teachers_with_capacity` tarih itibarıyla okur ---

    use crate::db::teaching_load_test_support::{change_chief_type_in_planning, seed_teacher, TERM};
    use crate::domain::models::ChiefType;
    use crate::domain::workload::MANAGEMENT_MAX_EXTRA_HOURS;

    /// Şeflik dönem başında yok, 2026-09-10'dan itibaren bölüm şefliğine
    /// (MADDE 6/4, 10 saat) dönüşür. Bu tarih oturumun GERÇEK bugününden
    /// (2026-09-26) önce olduğu için `Latest` (`current_as_of`, gerçek
    /// bugünü kullanır) de yeni değeri görür — `list_teachers_with_capacity_follows_the_projection_after_a_load_only_change`
    /// testindeki AYNI desen.
    #[tokio::test]
    async fn list_teachers_with_capacity_as_of_reads_the_chief_type_at_the_given_date() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = seed_teacher(&pool, "Ada", ChiefType::None).await;
        change_chief_type_in_planning(&pool, teacher_id, ChiefType::Department, ymd(2026, 9, 10)).await;

        let before = ReadAt::resolve(&pool, TERM, Some("2026-09-05".into())).await.unwrap();
        let rows_before = list_teachers_with_capacity_impl(&pool, &before).await.unwrap();
        assert_eq!(rows_before[0].chief_hours, 0, "9-05'te henüz şeflik yoktu");

        let after = ReadAt::resolve(&pool, TERM, Some("2026-09-15".into())).await.unwrap();
        let rows_after = list_teachers_with_capacity_impl(&pool, &after).await.unwrap();
        assert_eq!(rows_after[0].chief_hours, 10, "9-15'te bölüm şefliği başlamıştı");

        let rows_latest = list_teachers_with_capacity_impl(&pool, &ReadAt::Latest).await.unwrap();
        assert_eq!(rows_latest[0].chief_hours, 10, "Latest gerçek bugünü kullanır, değişiklik zaten geçmişte kaldı");
    }

    /// Dönem dışı bir tarih `Validation` ile reddedilir.
    #[tokio::test]
    async fn list_teachers_with_capacity_rejects_a_date_outside_the_term() {
        let (_dir, pool) = test_pool().await;
        let err = ReadAt::resolve(&pool, TERM, Some("2025-12-31".into())).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    /// MADDE 6/1-a: müdür ve müdür yardımcısı 24 saat ek dersle OLUŞTURULAMAZ;
    /// kapı reddeder ve hiçbir şey yazılmaz.
    #[tokio::test]
    async fn create_teacher_rejects_school_management_above_six_hours() {
        let (_dir, pool) = test_pool().await;
        let sets_before = count(&pool, "change_sets", None).await;

        for title in ["principal", "deputy_principal"] {
            let mut input = valid_input(title, title);
            input.max_extra_hours = 24;
            let result = create_teacher_impl(&pool, input, None, None, planning_today()).await;
            assert!(matches!(&result, Err(AppError::Validation(m)) if m.contains("MADDE 6/1-a")), "{title}: {result:?}");
        }
        assert_eq!(count(&pool, "change_sets", None).await, sets_before, "reddedilince hiçbir şey yazılmamalı");
    }

    /// 6 saat sınırın kendisidir: kabul edilir, unvan eski tabloya ve projeksiyona
    /// yazılır (yeni CHECK değerleri) ve şeflik saati 0 kalır (MADDE 6/4).
    #[tokio::test]
    async fn create_teacher_accepts_school_management_at_six_hours_and_they_are_not_chiefs() {
        let (_dir, pool) = test_pool().await;
        for title in ["principal", "deputy_principal"] {
            let mut input = valid_input(title, title);
            input.max_extra_hours = 6;
            let created = create_teacher_impl(&pool, input, None, None, planning_today()).await.unwrap();
            assert_eq!(created.chief_type, title);
        }
        let hours = teaching_load::chief_planning_hours(&pool, crate::db::teaching_load_test_support::TERM, ymd(2026, 10, 1))
            .await
            .unwrap();
        assert_eq!(hours, 0, "müdür/müdür yardımcısı havuza şeflik saati katmaz");
    }
}
