use crate::domain::models::{ChiefType, NewTeacher, Teacher};
use crate::error::{AppError, AppResult};
use sqlx::SqlitePool;

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
}
