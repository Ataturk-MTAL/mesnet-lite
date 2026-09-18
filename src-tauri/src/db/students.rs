use crate::domain::models::{NewStudent, Student};
use crate::error::{AppError, AppResult};
use sqlx::SqlitePool;

const SELECT_COLUMNS: &str =
    "id, first_name, last_name, student_no, grade, branch, company_id, submitted_at";

fn not_found(id: i64) -> AppError {
    AppError::NotFound(format!("Öğrenci bulunamadı: {id}"))
}

pub async fn list(pool: &SqlitePool) -> AppResult<Vec<Student>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM students
         ORDER BY grade COLLATE NOCASE, last_name COLLATE NOCASE, first_name COLLATE NOCASE"
    );
    Ok(sqlx::query_as::<_, Student>(&sql).fetch_all(pool).await?)
}

pub async fn get(pool: &SqlitePool, id: i64) -> AppResult<Student> {
    let sql = format!("SELECT {SELECT_COLUMNS} FROM students WHERE id = ?1");
    Ok(sqlx::query_as::<_, Student>(&sql)
        .bind(id)
        .fetch_one(pool)
        .await?)
}

pub async fn list_by_company(pool: &SqlitePool, company_id: i64) -> AppResult<Vec<Student>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM students WHERE company_id = ?1
         ORDER BY last_name COLLATE NOCASE, first_name COLLATE NOCASE"
    );
    Ok(sqlx::query_as::<_, Student>(&sql)
        .bind(company_id)
        .fetch_all(pool)
        .await?)
}

/// İşletme başına öğrenci sayısı. Saat tavanı kuralları bu sayıyı kullanır.
pub async fn count_by_company(pool: &SqlitePool) -> AppResult<Vec<(i64, i64)>> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT company_id, COUNT(*) FROM students
         WHERE company_id IS NOT NULL GROUP BY company_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn create(pool: &SqlitePool, input: &NewStudent) -> AppResult<Student> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO students
            (first_name, last_name, student_no, grade, branch, company_id, submitted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         RETURNING id",
    )
    .bind(&input.first_name)
    .bind(&input.last_name)
    .bind(&input.student_no)
    .bind(&input.grade)
    .bind(&input.branch)
    .bind(input.company_id)
    .bind(&input.submitted_at)
    .fetch_one(pool)
    .await?;

    get(pool, id).await
}

pub async fn update(pool: &SqlitePool, id: i64, input: &NewStudent) -> AppResult<Student> {
    let affected = sqlx::query(
        "UPDATE students SET
            first_name = ?1, last_name = ?2, student_no = ?3, grade = ?4,
            branch = ?5, company_id = ?6, submitted_at = ?7
         WHERE id = ?8",
    )
    .bind(&input.first_name)
    .bind(&input.last_name)
    .bind(&input.student_no)
    .bind(&input.grade)
    .bind(&input.branch)
    .bind(input.company_id)
    .bind(&input.submitted_at)
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
    let affected = sqlx::query("DELETE FROM students WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(not_found(id));
    }
    Ok(())
}

/// Aynı öğrencinin iki kez içe aktarılmasını önlemek için kullanılır.
/// Ad, soyad ve sınıf üçlüsü Unicode-doğru normalize edilerek karşılaştırılır.
pub async fn exists_with_name_and_grade(
    pool: &SqlitePool,
    first_name: &str,
    last_name: &str,
    grade: &str,
) -> AppResult<bool> {
    let key = normalize_person_key(first_name, last_name, grade);
    let rows = list(pool).await?;
    Ok(rows
        .iter()
        .any(|s| normalize_person_key(&s.first_name, &s.last_name, &s.grade) == key))
}

fn normalize_person_key(first_name: &str, last_name: &str, grade: &str) -> String {
    let normalize = |value: &str| {
        value
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    format!(
        "{}|{}|{}",
        normalize(first_name),
        normalize(last_name),
        normalize(grade)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{companies, init_pool};
    use crate::domain::models::NewCompany;

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    async fn a_company(pool: &SqlitePool, name: &str) -> i64 {
        companies::create(
            pool,
            &NewCompany {
                name: name.into(),
                contact_first_name: String::new(),
                contact_last_name: String::new(),
                phone: String::new(),
                email: String::new(),
                address_text: "Test adres".into(),
                latitude: None,
                longitude: None,
                one_way_distance_km: Some(3.0),
                notes: String::new(),
            },
        )
        .await
        .unwrap()
        .id
    }

    fn sample(first: &str, last: &str, grade: &str, company_id: Option<i64>) -> NewStudent {
        NewStudent {
            first_name: first.into(),
            last_name: last.into(),
            student_no: None,
            grade: grade.into(),
            branch: "Elektronik Haberleşme".into(),
            company_id,
            submitted_at: Some("2026-09-11".into()),
        }
    }

    #[tokio::test]
    async fn create_then_get_returns_same_record() {
        let (_dir, pool) = test_pool().await;
        let company_id = a_company(&pool, "Test İşletme A").await;

        let created = create(&pool, &sample("Ahmet", "Yılmaz", "12/C", Some(company_id)))
            .await
            .unwrap();
        let fetched = get(&pool, created.id).await.unwrap();

        assert_eq!(fetched.first_name, "Ahmet");
        assert_eq!(fetched.company_id, Some(company_id));
        // CSV'de öğrenci no boş olabilir; None olarak kalmalı.
        assert_eq!(fetched.student_no, None);
    }

    #[tokio::test]
    async fn list_by_company_returns_only_that_companys_students() {
        let (_dir, pool) = test_pool().await;
        let a = a_company(&pool, "İşletme A").await;
        let b = a_company(&pool, "İşletme B").await;

        create(&pool, &sample("Ahmet", "Yılmaz", "12/C", Some(a))).await.unwrap();
        create(&pool, &sample("Ayşe", "Demir", "12/C", Some(a))).await.unwrap();
        create(&pool, &sample("Mehmet", "Kaya", "12/D", Some(b))).await.unwrap();

        assert_eq!(list_by_company(&pool, a).await.unwrap().len(), 2);
        assert_eq!(list_by_company(&pool, b).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn count_by_company_groups_correctly() {
        let (_dir, pool) = test_pool().await;
        let a = a_company(&pool, "İşletme A").await;
        let b = a_company(&pool, "İşletme B").await;

        create(&pool, &sample("Ahmet", "Yılmaz", "12/C", Some(a))).await.unwrap();
        create(&pool, &sample("Ayşe", "Demir", "12/C", Some(a))).await.unwrap();
        create(&pool, &sample("Mehmet", "Kaya", "12/D", Some(b))).await.unwrap();

        let counts = count_by_company(&pool).await.unwrap();
        assert_eq!(counts.iter().find(|(id, _)| *id == a).unwrap().1, 2);
        assert_eq!(counts.iter().find(|(id, _)| *id == b).unwrap().1, 1);
    }

    /// İşletme silinince öğrenci silinmez; bağlantısı kopar (ON DELETE SET NULL).
    #[tokio::test]
    async fn deleting_company_detaches_students_without_deleting_them() {
        let (_dir, pool) = test_pool().await;
        let company_id = a_company(&pool, "Test İşletme A").await;
        let student = create(&pool, &sample("Ahmet", "Yılmaz", "12/C", Some(company_id)))
            .await
            .unwrap();

        companies::remove(&pool, company_id).await.unwrap();

        assert_eq!(get(&pool, student.id).await.unwrap().company_id, None);
    }

    #[tokio::test]
    async fn update_changes_fields() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample("Ahmet", "Yılmaz", "12/C", None)).await.unwrap();

        let mut input = sample("Ahmet", "Yılmaz", "12/D", None);
        input.student_no = Some("1234".into());
        let updated = update(&pool, created.id, &input).await.unwrap();

        assert_eq!(updated.grade, "12/D");
        assert_eq!(updated.student_no.as_deref(), Some("1234"));
    }

    #[tokio::test]
    async fn remove_deletes_record() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample("Ahmet", "Yılmaz", "12/C", None)).await.unwrap();

        remove(&pool, created.id).await.unwrap();

        assert!(matches!(
            get(&pool, created.id).await.unwrap_err(),
            AppError::NotFound(_)
        ));
    }

    #[tokio::test]
    async fn exists_with_name_and_grade_is_case_and_space_insensitive() {
        let (_dir, pool) = test_pool().await;
        create(&pool, &sample("AHMET", "YILMAZ", "12/C", None)).await.unwrap();

        // Büyük/küçük harf ve baştaki/sondaki boşluklar eşleşmeyi bozmamalı.
        assert!(exists_with_name_and_grade(&pool, "  ahmet  ", "YILMAZ", "12/C")
            .await
            .unwrap());
        assert!(exists_with_name_and_grade(&pool, "Ahmet", "Yilmaz", " 12/c ")
            .await
            .unwrap());

        // Farklı sınıf farklı öğrencidir; aynı ad soyad eşleşme saymaz.
        assert!(!exists_with_name_and_grade(&pool, "Ahmet", "Yilmaz", "12/D")
            .await
            .unwrap());
        assert!(!exists_with_name_and_grade(&pool, "Mehmet", "Yilmaz", "12/C")
            .await
            .unwrap());
    }
}
