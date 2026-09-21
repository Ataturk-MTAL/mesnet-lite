use crate::domain::history::events::TeacherLoad;
use crate::domain::models::{ChiefType, EmploymentType, NewTeacher, Teacher};
use crate::error::{AppError, AppResult};
use chrono::NaiveDate;
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::BTreeMap;

const SELECT_COLUMNS: &str = "id, first_name, last_name, registry_no, field, branches, \
     employment_type, base_hours, max_extra_hours, other_extra_hours, chief_type, is_active";

fn not_found(id: i64) -> AppError {
    AppError::NotFound(format!("Öğretmen bulunamadı: {id}"))
}

/// `branches` sütunu JSON dizi olarak saklanır.
fn encode_branches(branches: &[String]) -> AppResult<String> {
    serde_json::to_string(branches)
        .map_err(|e| AppError::Validation(format!("Dallar kaydedilemedi: {e}")))
}

/// Bozuk JSON kaydı okuma yolunu düşürmemeli; boş liste döner.
pub fn decode_branches(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

/// Metin değeri `ChiefType` enum'una çevirir. Tanınmayan değer şeflik yok sayılır;
/// şema zaten CHECK kısıtı ile üç değere sınırlıdır.
pub fn parse_chief_type(raw: &str) -> ChiefType {
    match raw {
        "department" => ChiefType::Department,
        "workshop_lab" => ChiefType::WorkshopLab,
        _ => ChiefType::None,
    }
}

/// Metin değeri `EmploymentType` enum'una çevirir. `parse_chief_type` ile aynı
/// desen: tanınmayan değer kadrolu sayılır; şema CHECK kısıtı zaten yalnız
/// `tenured`/`contracted` değerlerine izin verir.
pub fn parse_employment_type(raw: &str) -> EmploymentType {
    match raw {
        "contracted" => EmploymentType::Contracted,
        _ => EmploymentType::Tenured,
    }
}

/// Projeksiyonda satırı olmayan öğretmenin yükü (spec R5c). Yazma artık
/// tamamen kapıdan (`execute_change`) geçtiği için normalde HER öğretmenin
/// en az bir açılış `load_set` olayı vardır (bkz. migration 0008); bu değer
/// yalnız bir geçiş anomalisinde kullanılır ve şeflik saatini 0 sayar —
/// olmayan bir yükü var saymaktansa düşük hesaplamak tercih edilir.
fn zero_load() -> TeacherLoad {
    TeacherLoad {
        base_hours: 0,
        max_extra_hours: 0,
        other_extra_hours: 0,
        chief_type: ChiefType::None,
        employment_type: EmploymentType::Tenured,
    }
}

/// `as_of` gününde geçerli ders yükü satırları, öğretmen kimliğine göre.
/// `teaching_load::chief_planning_hours`ın kullandığı aynı yarı açık aralık
/// deseni: `valid_from <= as_of < valid_to`; `valid_to` boşsa aralık açıktır.
pub async fn load_as_of_by_teacher(
    pool: &SqlitePool,
    term: &str,
    as_of: NaiveDate,
) -> AppResult<BTreeMap<i64, TeacherLoad>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        teacher_id: i64,
        base_hours: i64,
        max_extra_hours: i64,
        other_extra_hours: i64,
        chief_type: String,
        employment_type: String,
    }

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT teacher_id, base_hours, max_extra_hours, other_extra_hours, chief_type, employment_type
         FROM teacher_load_periods
         WHERE term = ?1 AND valid_from <= ?2 AND (valid_to IS NULL OR ?2 < valid_to)",
    )
    .bind(term)
    .bind(as_of)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let load = TeacherLoad {
                base_hours: r.base_hours,
                max_extra_hours: r.max_extra_hours,
                other_extra_hours: r.other_extra_hours,
                chief_type: parse_chief_type(&r.chief_type),
                employment_type: parse_employment_type(&r.employment_type),
            };
            (r.teacher_id, load)
        })
        .collect())
}

/// Bir öğretmen ve `as_of` gününde projeksiyondan okunan yükü.
#[derive(Debug, Clone)]
pub struct TeacherWithLoadAsOf {
    pub teacher: Teacher,
    pub load: TeacherLoad,
}

/// Öğretmen listesi + her birinin `as_of` gününde geçerli yükü — kapasite
/// hesaplayan HER yerin (atama tahtası, Genel Bakış, öğretmen listesi) tek
/// okuyucusu. Eski `teachers.chief_type`/yük sütunları burada OKUNMAZ; tek
/// doğruluk kaynağı `teacher_load_periods` projeksiyonudur.
pub async fn list_with_load_as_of(
    pool: &SqlitePool,
    term: &str,
    as_of: NaiveDate,
) -> AppResult<Vec<TeacherWithLoadAsOf>> {
    let teachers = list(pool).await?;
    let loads = load_as_of_by_teacher(pool, term, as_of).await?;
    Ok(teachers
        .into_iter()
        .map(|teacher| {
            let load = loads.get(&teacher.id).cloned().unwrap_or_else(zero_load);
            TeacherWithLoadAsOf { teacher, load }
        })
        .collect())
}

pub async fn list(pool: &SqlitePool) -> AppResult<Vec<Teacher>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM teachers
         ORDER BY last_name COLLATE NOCASE, first_name COLLATE NOCASE"
    );
    Ok(sqlx::query_as::<_, Teacher>(&sql).fetch_all(pool).await?)
}

pub async fn list_active(pool: &SqlitePool) -> AppResult<Vec<Teacher>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM teachers WHERE is_active = 1
         ORDER BY last_name COLLATE NOCASE, first_name COLLATE NOCASE"
    );
    Ok(sqlx::query_as::<_, Teacher>(&sql).fetch_all(pool).await?)
}

pub async fn get(pool: &SqlitePool, id: i64) -> AppResult<Teacher> {
    let sql = format!("SELECT {SELECT_COLUMNS} FROM teachers WHERE id = ?1");
    Ok(sqlx::query_as::<_, Teacher>(&sql)
        .bind(id)
        .fetch_one(pool)
        .await?)
}

/// Öğretmeni doğrudan eski tabloya yazar; olay günlüğüne UĞRAMAZ. Üretimde
/// artık tek yazma kapısı `commands/teacher_commands.rs::create_teacher_impl`
/// (kapıdan, `execute_change`) olduğu için bu fonksiyonun üretim çağıranı
/// YOKTUR — yalnız test fikstürleri (ör. `teaching_load.rs`'in eski-yoldan
/// öğretmen senaryosu, `services/commission_minutes_test_support.rs`)
/// kapıyı BİLEREK atlayıp eski davranışı simüle etmek için kullanır. Bu
/// yüzden `#[cfg(test)]`: gerçek derlemede "kullanılmıyor" uyarısı vermeden,
/// test derlemesinde hâlâ erişilebilir kalır.
#[cfg(test)]
pub async fn create(pool: &SqlitePool, input: &NewTeacher) -> AppResult<Teacher> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO teachers
            (first_name, last_name, registry_no, field, branches, employment_type,
             base_hours, max_extra_hours, other_extra_hours, chief_type, is_active)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         RETURNING id",
    )
    .bind(&input.first_name)
    .bind(&input.last_name)
    .bind(&input.registry_no)
    .bind(&input.field)
    .bind(encode_branches(&input.branches)?)
    .bind(&input.employment_type)
    .bind(input.base_hours)
    .bind(input.max_extra_hours)
    .bind(input.other_extra_hours)
    .bind(&input.chief_type)
    .bind(i64::from(input.is_active))
    .fetch_one(pool)
    .await?;

    get(pool, id).await
}

/// `change_service::execute_in` (R4) yerinde oluşturma adımı içindir
/// (spec §5 adım 2) — `createTeacher` komutu bu satırı `decide`'dan ÖNCE,
/// aynı transaction'daki bağlantı üzerinden açar.
pub async fn create_in(conn: &mut SqliteConnection, input: &NewTeacher) -> AppResult<Teacher> {
    let sql = format!(
        "INSERT INTO teachers
            (first_name, last_name, registry_no, field, branches, employment_type,
             base_hours, max_extra_hours, other_extra_hours, chief_type, is_active)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         RETURNING {SELECT_COLUMNS}"
    );
    Ok(sqlx::query_as::<_, Teacher>(&sql)
        .bind(&input.first_name)
        .bind(&input.last_name)
        .bind(&input.registry_no)
        .bind(&input.field)
        .bind(encode_branches(&input.branches)?)
        .bind(&input.employment_type)
        .bind(input.base_hours)
        .bind(input.max_extra_hours)
        .bind(input.other_extra_hours)
        .bind(&input.chief_type)
        .bind(i64::from(input.is_active))
        .fetch_one(&mut *conn)
        .await?)
}

/// `deleteTeacher` — `decide::teacher::delete_teacher`in `HasHistory`
/// denetiminden SONRA çağrılır; olay günlüğüne dokunmaz.
pub async fn remove_in(conn: &mut SqliteConnection, id: i64) -> AppResult<()> {
    let affected = sqlx::query("DELETE FROM teachers WHERE id = ?1")
        .bind(id)
        .execute(&mut *conn)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(not_found(id));
    }
    Ok(())
}

pub async fn update(pool: &SqlitePool, id: i64, input: &NewTeacher) -> AppResult<Teacher> {
    let affected = sqlx::query(
        "UPDATE teachers SET
            first_name = ?1, last_name = ?2, registry_no = ?3, field = ?4, branches = ?5,
            employment_type = ?6, base_hours = ?7, max_extra_hours = ?8,
            other_extra_hours = ?9, chief_type = ?10, is_active = ?11
         WHERE id = ?12",
    )
    .bind(&input.first_name)
    .bind(&input.last_name)
    .bind(&input.registry_no)
    .bind(&input.field)
    .bind(encode_branches(&input.branches)?)
    .bind(&input.employment_type)
    .bind(input.base_hours)
    .bind(input.max_extra_hours)
    .bind(input.other_extra_hours)
    .bind(&input.chief_type)
    .bind(i64::from(input.is_active))
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(not_found(id));
    }
    get(pool, id).await
}

pub async fn remove(pool: &SqlitePool, id: i64) -> AppResult<()> {
    let affected = sqlx::query("DELETE FROM teachers WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(not_found(id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    fn sample(last_name: &str, chief_type: &str) -> NewTeacher {
        NewTeacher {
            first_name: "Test".into(),
            last_name: last_name.into(),
            registry_no: "123456".into(),
            field: "Elektrik-Elektronik Teknolojisi".into(),
            branches: vec![
                "Elektronik Haberleşme".into(),
                "Endüstriyel Bakım Onarım".into(),
            ],
            employment_type: "tenured".into(),
            base_hours: 20,
            max_extra_hours: 24,
            other_extra_hours: 0,
            chief_type: chief_type.into(),
            is_active: true,
        }
    }

    #[tokio::test]
    async fn create_then_get_roundtrips_branches_as_json() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample("Yılmaz", "none")).await.unwrap();
        let fetched = get(&pool, created.id).await.unwrap();

        let branches = decode_branches(&fetched.branches);
        assert_eq!(branches.len(), 2);
        assert_eq!(branches[0], "Elektronik Haberleşme");
    }

    #[tokio::test]
    async fn defaults_follow_madde_5_and_6() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample("Yılmaz", "none")).await.unwrap();

        // MADDE 5/1-ç: atölye ve laboratuvar öğretmenleri haftada 20 saat
        assert_eq!(created.base_hours, 20);
        // MADDE 6/1-c: haftada 24 saate kadar ek ders
        assert_eq!(created.max_extra_hours, 24);
    }

    #[tokio::test]
    async fn chief_type_is_stored_and_maps_to_statutory_hours() {
        let (_dir, pool) = test_pool().await;

        let department = create(&pool, &sample("Bölüm", "department")).await.unwrap();
        let workshop = create(&pool, &sample("Atölye", "workshop_lab"))
            .await
            .unwrap();
        let plain = create(&pool, &sample("Düz", "none")).await.unwrap();

        // MADDE 6/4: bölüm şefi 10, atölye ve laboratuvar şefi 6 saat
        assert_eq!(parse_chief_type(&department.chief_type).weekly_hours(), 10);
        assert_eq!(parse_chief_type(&workshop.chief_type).weekly_hours(), 6);
        assert_eq!(parse_chief_type(&plain.chief_type).weekly_hours(), 0);
    }

    #[tokio::test]
    async fn list_active_excludes_inactive_teachers() {
        let (_dir, pool) = test_pool().await;
        create(&pool, &sample("Aktif", "none")).await.unwrap();

        let mut passive = sample("Pasif", "none");
        passive.is_active = false;
        create(&pool, &passive).await.unwrap();

        assert_eq!(list(&pool).await.unwrap().len(), 2);
        let active = list_active(&pool).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].last_name, "Aktif");
    }

    #[tokio::test]
    async fn update_changes_chief_type_and_branches() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample("Yılmaz", "none")).await.unwrap();

        let mut input = sample("Yılmaz", "department");
        input.branches = vec!["Elektrik Tesisatları ve Pano Montörlüğü".into()];
        let updated = update(&pool, created.id, &input).await.unwrap();

        assert_eq!(updated.chief_type, "department");
        assert_eq!(decode_branches(&updated.branches).len(), 1);
    }

    #[tokio::test]
    async fn remove_deletes_record() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample("Yılmaz", "none")).await.unwrap();

        remove(&pool, created.id).await.unwrap();

        assert!(matches!(
            get(&pool, created.id).await.unwrap_err(),
            AppError::NotFound(_)
        ));
    }

    #[tokio::test]
    async fn create_in_writes_through_the_given_connection() {
        let (_dir, pool) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let created = create_in(&mut conn, &sample("Bağlantı", "none")).await.unwrap();
        assert_eq!(get(&pool, created.id).await.unwrap().last_name, "Bağlantı");
    }

    #[tokio::test]
    async fn remove_in_deletes_the_row() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample("Yılmaz", "none")).await.unwrap();

        let mut conn = pool.acquire().await.unwrap();
        remove_in(&mut conn, created.id).await.unwrap();

        assert!(matches!(get(&pool, created.id).await.unwrap_err(), AppError::NotFound(_)));
    }

    /// Bozuk JSON okuma yolunu düşürmemeli.
    #[test]
    fn decode_branches_returns_empty_on_invalid_json() {
        assert!(decode_branches("bozuk json").is_empty());
        assert!(decode_branches("").is_empty());
    }

    #[test]
    fn parse_chief_type_falls_back_to_none_for_unknown_value() {
        assert_eq!(parse_chief_type("bilinmeyen"), ChiefType::None);
    }

    #[test]
    fn parse_employment_type_falls_back_to_tenured_for_unknown_value() {
        assert_eq!(parse_employment_type("bilinmeyen"), EmploymentType::Tenured);
        assert_eq!(parse_employment_type("contracted"), EmploymentType::Contracted);
    }

    // --- `list_with_load_as_of` — R5c'nin tek okuyucusu ---

    use crate::db::teaching_load_test_support::{change_chief_type_in_planning, seed_teacher, ymd, TERM};

    /// Projeksiyonda satırı olmayan öğretmen için yük SIFIR sayılır (rapor
    /// edilen karar): yazma artık kapıdan geçtiği için bu durum yalnız bir
    /// geçiş anomalisinde görülür.
    #[tokio::test]
    async fn list_with_load_as_of_zeroes_out_a_teacher_without_a_projection_row() {
        let (_dir, pool) = test_pool().await;
        // Doğrudan eski yoldan yazılır: `execute_change`e hiç uğramaz, bu
        // yüzden projeksiyonda satırı yoktur (tarihte teacher_commands.rs'in
        // eski `create_teacher`'ının ürettiği kayıtla aynı durum).
        create(&pool, &sample("Kapıdan Geçmemiş", "department")).await.unwrap();

        let rows = list_with_load_as_of(&pool, TERM, ymd(2026, 10, 1)).await.unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].load.chief_type, ChiefType::None, "satır yoksa şeflik sıfır sayılır");
        assert_eq!(rows[0].load.max_extra_hours, 0);
    }

    /// Kilit test: okuma projeksiyondan gelir, eski `teachers.chief_type`
    /// sütunu DEĞİŞMESE bile. `change_chief_type` yalnız kapıdan
    /// (`execute_change`) yazar; eski sütuna hiç dokunmaz.
    #[tokio::test]
    async fn list_with_load_as_of_follows_the_projection_even_when_the_legacy_column_stays_stale() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = seed_teacher(&pool, "Ada", ChiefType::None).await;
        change_chief_type_in_planning(&pool, teacher_id, ChiefType::Department, ymd(2026, 9, 5)).await;

        // Eski sütun hâlâ "none" — hiçbir yazma yolu ona dokunmadı.
        let legacy = get(&pool, teacher_id).await.unwrap();
        assert_eq!(legacy.chief_type, "none", "eski sütun donuk kalmalı");

        let rows = list_with_load_as_of(&pool, TERM, ymd(2026, 10, 1)).await.unwrap();
        let entry = rows.iter().find(|r| r.teacher.id == teacher_id).unwrap();
        assert_eq!(entry.load.chief_type, ChiefType::Department, "okuma projeksiyondan gelmeli");
    }
}
