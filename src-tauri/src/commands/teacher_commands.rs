use crate::db::read_at::ReadAt;
use crate::db::{settings, teachers, AppState};
use crate::domain::history::decide::{ChangeCommand, ChangeRequest, ImpactSummary, NewTeacherProfile};
use crate::domain::history::events::TeacherLoad;
use crate::domain::models::{NewTeacher, Teacher};
use crate::domain::terms::today_local;
use crate::domain::workload::{statutory_cap, teacher_capacity, InstitutionType};
use crate::error::{AppError, AppResult};
use crate::services::change_service::{execute_change, ChangeMode, ChangeOutcome};
use chrono::NaiveDate;
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::State;

/// `create_teacher`/`update_teacher` gerekçe vermezse yazılan varsayılan.
/// Gerekçe geldiğinde (arayüzden `reason`) onun yerine geçer.
const DEFAULT_CREATE_REASON: &str = "Öğretmen ekranından eklendi";
const DEFAULT_UPDATE_REASON: &str = "Öğretmen ekranından güncellendi";

/// Öğretmen + mevzuattan türetilen kapasite bilgisi.
/// Kapasite hesabı yalnızca Rust tarafında yapılır; arayüz onu yeniden hesaplamaz.
///
/// `teacher` eski `teachers` tablosundan gelir; `chiefHours`/`capacity` ise
/// `teacher_load_periods` PROJEKSİYONUNDAN türetilir. `update_teacher_impl`
/// yük değiştiğinde eski sütunu da AYNI çağrıda güncellediği için normal
/// düzenleme akışında ikisi senkron kalır. Yalnız yükü tarihçe ekranından
/// (`commit_change` ile doğrudan `SetTeacherLoad`) değiştiren bir düzeltme
/// bu komutu ATLAR — o durumda `teacher.baseHours` vb. ham alanlar donuk
/// kalır, `chiefHours`/`capacity` yine doğrudur (bkz. brief R5c, bilinen sınır).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeacherWithCapacity {
    #[serde(flatten)]
    pub teacher: Teacher,
    /// MADDE 6/4: bölüm şefi 10, atölye/laboratuvar şefi 6, şef değilse 0.
    pub chief_hours: i64,
    /// MADDE 15/2 tavanı (okul tipi ve büyükşehir durumuna göre).
    pub statutory_cap: i64,
    /// Koordinatörlük için kalan haftalık saat.
    pub capacity: i64,
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
            TeacherWithCapacity {
                teacher: entry.teacher,
                chief_hours,
                statutory_cap: cap,
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

/// Girilen yük, öğretmenin şu an kayıtlı yükünden (eski `teachers` sütunu —
/// `update_teacher_impl` yük DEĞİŞMEDİĞİNDE bu sütunu da değiştirmediği için
/// projeksiyonla aynı durumdadır) FARKLI mı? Yalnız farklıysa MADDE 6/4
/// olayı üretilir; aksi hâlde günlüğe anlamsız bir "değişiklik" yazılmaz.
fn load_changed(current: &Teacher, input: &NewTeacher) -> bool {
    current.base_hours != input.base_hours
        || current.max_extra_hours != input.max_extra_hours
        || current.other_extra_hours != input.other_extra_hours
        || current.chief_type != input.chief_type
        || current.employment_type != input.employment_type
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
fn validate_identity(input: &NewTeacher) -> AppResult<()> {
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

/// Öğretmeni günceller: yük/şeflik alanları değiştiyse `SetTeacherLoad`
/// olayı kapıdan geçer (spec R5c madde 1); kimlik alanları (ad, soyad,
/// sicil, alan, branşlar, aktiflik) eski yoldan (`teachers::update`)
/// yazılmaya devam eder. Kapı reddederse (ör. dönem başlamış, tarih
/// verilmemiş) HİÇBİR şey yazılmaz — kimlik güncellemesi de dahil.
async fn update_teacher_impl(
    pool: &SqlitePool,
    id: i64,
    input: NewTeacher,
    effective_date: Option<NaiveDate>,
    reason: Option<String>,
    today: NaiveDate,
) -> AppResult<Teacher> {
    validate_identity(&input)?;
    let current = teachers::get(pool, id).await?;

    if load_changed(&current, &input) {
        let term = settings::get_active_term(pool).await?;
        let command = ChangeCommand::SetTeacherLoad { teacher_id: id, load: load_from_input(&input) };
        commit_teacher_change(pool, &term, command, effective_date, reason, DEFAULT_UPDATE_REASON, today).await?;
    }

    teachers::update(pool, id, &input).await
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
pub async fn update_teacher(
    state: State<'_, AppState>,
    id: i64,
    input: NewTeacher,
    effective_date: Option<NaiveDate>,
    reason: Option<String>,
) -> AppResult<Teacher> {
    update_teacher_impl(&state.pool, id, input, effective_date, reason, today_local()).await
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
        let mut input = valid_input("Ogretmen", "none");
        input.first_name = " ".into();
        assert!(validate_identity(&input).is_err());

        let mut input = valid_input("Ogretmen", "none");
        input.field = String::new();
        assert!(validate_identity(&input).is_err());
    }

    #[test]
    fn accepts_valid_identity() {
        assert!(validate_identity(&valid_input("Ogretmen", "none")).is_ok());
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

    /// Şefliği `none` → `department` yapan güncelleme olay üretir; havuz
    /// değişir. Eski `teachers.chief_type` de (bu komutun kimlik-dışı yolu
    /// üzerinden) aynı değere gelir çünkü ikisi birden yazılır.
    #[tokio::test]
    async fn updating_the_chief_type_emits_a_load_event_and_the_pool_changes() {
        let (_dir, pool) = test_pool().await;
        let created = create_teacher_impl(&pool, valid_input("Once", "none"), None, None, planning_today())
            .await
            .unwrap();

        let mut input = valid_input("Once", "department");
        let updated = update_teacher_impl(&pool, created.id, input.clone(), Some(ymd(2026, 11, 5)), None, november_today())
            .await
            .unwrap();
        input.chief_type = "department".into();

        assert_eq!(count(&pool, "change_sets", Some("set_teacher_load")).await, 1);
        assert_eq!(updated.chief_type, "department");

        let hours = teaching_load::chief_planning_hours(&pool, crate::db::teaching_load_test_support::TERM, ymd(2026, 11, 10))
            .await
            .unwrap();
        assert_eq!(hours, 10);
    }

    /// Kimlik alanı değişip yük DEĞİŞMEDİYSE gate'e hiç uğranmaz — günlüğe
    /// anlamsız bir "yük değişti" olayı yazılmaz.
    #[tokio::test]
    async fn updating_only_identity_fields_does_not_touch_the_change_log() {
        let (_dir, pool) = test_pool().await;
        let created = create_teacher_impl(&pool, valid_input("Isim", "none"), None, None, planning_today())
            .await
            .unwrap();
        let sets_before = count(&pool, "change_sets", None).await;

        let mut input = valid_input("YeniIsim", "none");
        input.first_name = "Yeni".into();
        let updated = update_teacher_impl(&pool, created.id, input, None, None, planning_today())
            .await
            .unwrap();

        assert_eq!(updated.first_name, "Yeni");
        assert_eq!(count(&pool, "change_sets", None).await, sets_before, "yük değişmediyse olay yazılmamalı");
    }

    /// Dönem başladıysa yük değişikliği tarih ister; tarih verilmezse
    /// `Validation` ile reddedilir ve HİÇBİR şey yazılmaz (ne olay ne eski
    /// tablo).
    #[tokio::test]
    async fn changing_load_after_the_term_started_without_a_date_writes_nothing() {
        let (_dir, pool) = test_pool().await;
        let created = create_teacher_impl(&pool, valid_input("Ada", "none"), None, None, planning_today())
            .await
            .unwrap();
        let sets_before = count(&pool, "change_sets", None).await;

        let input = valid_input("Ada", "department");
        let result = update_teacher_impl(&pool, created.id, input, None, None, november_today()).await;

        assert!(matches!(result, Err(AppError::Validation(_))), "Validation beklenirdi: {result:?}");
        assert_eq!(count(&pool, "change_sets", None).await, sets_before);
        let unchanged = teachers::get(&pool, created.id).await.unwrap();
        assert_eq!(unchanged.chief_type, "none", "eski tablo da değişmemeli");
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
            DEFAULT_UPDATE_REASON,
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

    // --- R5 spec §6: `list_teachers_with_capacity` tarih itibarıyla okur ---

    use crate::db::teaching_load_test_support::{change_chief_type_in_planning, seed_teacher, TERM};
    use crate::domain::models::ChiefType;

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
}
