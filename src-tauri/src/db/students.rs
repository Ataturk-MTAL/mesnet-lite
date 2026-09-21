use crate::domain::models::{NewStudent, Student};
use crate::error::{AppError, AppResult};
use sqlx::{SqliteConnection, SqlitePool};

const SELECT_COLUMNS: &str =
    "id, first_name, last_name, student_no, grade, branch, company_id, submitted_at, term";

fn not_found(id: i64) -> AppError {
    AppError::NotFound(format!("Öğrenci bulunamadı: {id}"))
}

/// TÜM dönemlerdeki öğrenciler. Ekranlar genelde `list_by_term` kullanır.
pub async fn list(pool: &SqlitePool) -> AppResult<Vec<Student>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM students
         ORDER BY term DESC, grade COLLATE NOCASE, last_name COLLATE NOCASE, \
                  first_name COLLATE NOCASE"
    );
    Ok(sqlx::query_as::<_, Student>(&sql).fetch_all(pool).await?)
}

/// Yalnızca verilen eğitim-öğretim yılındaki öğrenciler.
pub async fn list_by_term(pool: &SqlitePool, term: &str) -> AppResult<Vec<Student>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM students WHERE term = ?1
         ORDER BY grade COLLATE NOCASE, last_name COLLATE NOCASE, first_name COLLATE NOCASE"
    );
    Ok(sqlx::query_as::<_, Student>(&sql)
        .bind(term)
        .fetch_all(pool)
        .await?)
}

/// `list_by_term`in aynı transaction'daki bağlantı üzerinden çalışan hâli.
/// `services::student_list_apply` (e-Okul sınıf listesi içe aktarımı) TÜM
/// okuma ve yazmayı tek `BEGIN IMMEDIATE` transaction'ında yapar; transaction
/// açıkken havuzdan okumak sessizce eski veriyi döndürebileceği için
/// (`change_service.rs` üstteki uyarıyla aynı gerekçe) bu bağlantı-bazlı
/// kopya gerekir.
pub async fn list_by_term_in(conn: &mut SqliteConnection, term: &str) -> AppResult<Vec<Student>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM students WHERE term = ?1
         ORDER BY grade COLLATE NOCASE, last_name COLLATE NOCASE, first_name COLLATE NOCASE"
    );
    Ok(sqlx::query_as::<_, Student>(&sql)
        .bind(term)
        .fetch_all(&mut *conn)
        .await?)
}

/// Veritabanındaki tüm dönemler, en yeniden eskiye.
pub async fn list_terms(pool: &SqlitePool) -> AppResult<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT term FROM students WHERE term <> '' ORDER BY term DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(term,)| term).collect())
}

pub async fn get(pool: &SqlitePool, id: i64) -> AppResult<Student> {
    let sql = format!("SELECT {SELECT_COLUMNS} FROM students WHERE id = ?1");
    Ok(sqlx::query_as::<_, Student>(&sql)
        .bind(id)
        .fetch_one(pool)
        .await?)
}

/// Bir işletmedeki öğrenciler — dönem bazlı. İşletme kalıcıdır, öğrenciler değil.
pub async fn list_by_company(
    pool: &SqlitePool,
    company_id: i64,
    term: &str,
) -> AppResult<Vec<Student>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM students WHERE company_id = ?1 AND term = ?2
         ORDER BY last_name COLLATE NOCASE, first_name COLLATE NOCASE"
    );
    Ok(sqlx::query_as::<_, Student>(&sql)
        .bind(company_id)
        .bind(term)
        .fetch_all(pool)
        .await?)
}

/// İşletme başına öğrenci sayısı — dönem bazlı.
/// Saat tavanı kuralları bu sayıyı kullanır, bu yüzden dönem filtresi zorunludur:
/// geçen yılın öğrencileri bu yılın tavanını yükseltmemelidir.
pub async fn count_by_company(pool: &SqlitePool, term: &str) -> AppResult<Vec<(i64, i64)>> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT company_id, COUNT(*) FROM students
         WHERE company_id IS NOT NULL AND term = ?1 GROUP BY company_id",
    )
    .bind(term)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn create(pool: &SqlitePool, input: &NewStudent) -> AppResult<Student> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO students
            (first_name, last_name, student_no, grade, branch, company_id, submitted_at, term)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         RETURNING id",
    )
    .bind(&input.first_name)
    .bind(&input.last_name)
    .bind(&input.student_no)
    .bind(&input.grade)
    .bind(&input.branch)
    .bind(input.company_id)
    .bind(&input.submitted_at)
    .bind(&input.term)
    .fetch_one(pool)
    .await?;

    get(pool, id).await
}

/// `change_service::execute_in` (R4) yerinde oluşturma adımı içindir
/// (spec §5 adım 2) — `createStudent` komutu, `decide`'dan ÖNCE, aynı
/// transaction'daki bağlantı üzerinden bu satırı açar.
pub async fn create_in(conn: &mut SqliteConnection, input: &NewStudent) -> AppResult<Student> {
    let sql = format!(
        "INSERT INTO students
            (first_name, last_name, student_no, grade, branch, company_id, submitted_at, term)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         RETURNING {SELECT_COLUMNS}"
    );
    Ok(sqlx::query_as::<_, Student>(&sql)
        .bind(&input.first_name)
        .bind(&input.last_name)
        .bind(&input.student_no)
        .bind(&input.grade)
        .bind(&input.branch)
        .bind(input.company_id)
        .bind(&input.submitted_at)
        .bind(&input.term)
        .fetch_one(&mut *conn)
        .await?)
}

/// `deleteStudent` — yalnız açılış dışında geçmişi olmayan bir öğrenci için
/// çağrılır (`decide::student::delete_student`in `HasHistory` denetiminden
/// SONRA); satırın kendisi burada silinir, olay günlüğüne dokunmaz.
pub async fn remove_in(conn: &mut SqliteConnection, id: i64) -> AppResult<()> {
    let affected = sqlx::query("DELETE FROM students WHERE id = ?1")
        .bind(id)
        .execute(&mut *conn)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(not_found(id));
    }
    Ok(())
}

/// e-Okul sınıf listesi içe aktarımı SIRASINDA mevcut bir öğrencinin
/// ad/soyad/sınıf/dal alanlarını günceller. `company_id`, `submitted_at` ve
/// `term` BİLİNÇLİ OLARAK dokunulmaz: bu dosyada işletme bilgisi yoktur,
/// öğrencinin var olan yerleştirmesi ya da dönemi bu içe aktarmayla
/// bozulmamalıdır (brief: "İşletme ataması YOK").
pub async fn update_identity_fields_in(
    conn: &mut SqliteConnection,
    id: i64,
    first_name: &str,
    last_name: &str,
    grade: &str,
    branch: &str,
) -> AppResult<Student> {
    let sql = format!(
        "UPDATE students SET first_name = ?1, last_name = ?2, grade = ?3, branch = ?4
         WHERE id = ?5
         RETURNING {SELECT_COLUMNS}"
    );
    Ok(sqlx::query_as::<_, Student>(&sql)
        .bind(first_name)
        .bind(last_name)
        .bind(grade)
        .bind(branch)
        .bind(id)
        .fetch_one(&mut *conn)
        .await?)
}

pub async fn update(pool: &SqlitePool, id: i64, input: &NewStudent) -> AppResult<Student> {
    let affected = sqlx::query(
        "UPDATE students SET
            first_name = ?1, last_name = ?2, student_no = ?3, grade = ?4,
            branch = ?5, company_id = ?6, submitted_at = ?7, term = ?8
         WHERE id = ?9",
    )
    .bind(&input.first_name)
    .bind(&input.last_name)
    .bind(&input.student_no)
    .bind(&input.grade)
    .bind(&input.branch)
    .bind(input.company_id)
    .bind(&input.submitted_at)
    .bind(&input.term)
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

/// `services::student_list_apply` da (e-Okul içe aktarımı) aynı normalize
/// kuralını kullanır; farklı olan yalnız yedek anahtarın alan SAYISIDIR
/// (bkz. `find_duplicate` üstündeki yorum), o yüzden bu yardımcı paylaşılır.
pub(crate) fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn non_empty(value: Option<&String>) -> Option<String> {
    value
        .map(|v| normalize(v))
        .filter(|v| !v.is_empty())
}

/// Aynı öğrencinin iki kez içe aktarılmasını önlemek için kullanılır.
///
/// Kimlik önce ÖĞRENCİ NUMARASINDAN gelir; numara gerçek kimliktir ve tekildir.
/// Numara yoksa ad + soyad + sınıf + DAL dörtlüsüne düşülür.
///
/// Dal'ın anahtara dahil olması zorunludur: gerçek veride aynı sınıfta aynı ad
/// ve soyada sahip iki farklı öğrenci bulunmaktadır (farklı numara, farklı dal,
/// farklı işletme). Dal olmadan biri sessizce kaybolur.
///
/// Numarası olmayan ve her şeyi aynı olan iki farklı öğrenci hâlâ ayırt edilemez;
/// bu durumda kullanıcının numara girmesi gerekir.
pub async fn find_duplicate(pool: &SqlitePool, candidate: &NewStudent) -> AppResult<Option<Student>> {
    // Mükerrer arama AYNI DÖNEM içinde yapılır: geçen yılın aynı öğrencisi
    // bu yılın kaydını engellememelidir.
    let rows = list_by_term(pool, &candidate.term).await?;

    if let Some(candidate_no) = non_empty(candidate.student_no.as_ref()) {
        return Ok(rows
            .into_iter()
            .find(|s| non_empty(s.student_no.as_ref()).as_deref() == Some(candidate_no.as_str())));
    }

    let key = fallback_key(
        &candidate.first_name,
        &candidate.last_name,
        &candidate.grade,
        &candidate.branch,
    );

    Ok(rows.into_iter().find(|s| {
        // Numarası olan bir kayıt, numarasız bir adayla ad üzerinden eşleşmez;
        // aksi hâlde numarası girilmemiş yeni bir öğrenci yanlışlıkla yutulur.
        non_empty(s.student_no.as_ref()).is_none()
            && fallback_key(&s.first_name, &s.last_name, &s.grade, &s.branch) == key
    }))
}

fn fallback_key(first_name: &str, last_name: &str, grade: &str, branch: &str) -> String {
    format!(
        "{}|{}|{}|{}",
        normalize(first_name),
        normalize(last_name),
        normalize(grade),
        normalize(branch)
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
                district: String::new(),
                notes: String::new(),
            },
        )
        .await
        .unwrap()
        .id
    }

    const TERM: &str = "2026-2027/1";

    fn sample(first: &str, last: &str, grade: &str, company_id: Option<i64>) -> NewStudent {
        NewStudent {
            first_name: first.into(),
            last_name: last.into(),
            student_no: None,
            grade: grade.into(),
            branch: "Elektronik Haberleşme".into(),
            company_id,
            submitted_at: Some("2026-09-11".into()),
            term: TERM.into(),
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

        assert_eq!(list_by_company(&pool, a, TERM).await.unwrap().len(), 2);
        assert_eq!(list_by_company(&pool, b, TERM).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn count_by_company_groups_correctly() {
        let (_dir, pool) = test_pool().await;
        let a = a_company(&pool, "İşletme A").await;
        let b = a_company(&pool, "İşletme B").await;

        create(&pool, &sample("Ahmet", "Yılmaz", "12/C", Some(a))).await.unwrap();
        create(&pool, &sample("Ayşe", "Demir", "12/C", Some(a))).await.unwrap();
        create(&pool, &sample("Mehmet", "Kaya", "12/D", Some(b))).await.unwrap();

        let counts = count_by_company(&pool, TERM).await.unwrap();
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
    async fn create_in_writes_through_the_given_connection() {
        let (_dir, pool) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let created = create_in(&mut conn, &sample("Bağlantı", "Testi", "12/C", None)).await.unwrap();
        assert_eq!(get(&pool, created.id).await.unwrap().first_name, "Bağlantı");
    }

    #[tokio::test]
    async fn remove_in_deletes_the_row() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample("Ahmet", "Yılmaz", "12/C", None)).await.unwrap();

        let mut conn = pool.acquire().await.unwrap();
        remove_in(&mut conn, created.id).await.unwrap();

        assert!(matches!(get(&pool, created.id).await.unwrap_err(), AppError::NotFound(_)));
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

    /// Öğrenci listesi döneme bağlıdır; işletmeler kalıcıdır.
    #[tokio::test]
    async fn list_by_term_separates_academic_years() {
        let (_dir, pool) = test_pool().await;

        create(&pool, &sample("Ahmet", "Yilmaz", "12/C", None)).await.unwrap();

        let mut next_year = sample("Ayse", "Demir", "12/C", None);
        next_year.term = "2027-2028/1".into();
        create(&pool, &next_year).await.unwrap();

        assert_eq!(list(&pool).await.unwrap().len(), 2, "tüm dönemler");
        assert_eq!(list_by_term(&pool, TERM).await.unwrap().len(), 1);
        assert_eq!(list_by_term(&pool, "2027-2028/1").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn list_terms_returns_distinct_terms_newest_first() {
        let (_dir, pool) = test_pool().await;
        create(&pool, &sample("Ahmet", "Yilmaz", "12/C", None)).await.unwrap();

        let mut next_year = sample("Ayse", "Demir", "12/C", None);
        next_year.term = "2027-2028/1".into();
        create(&pool, &next_year).await.unwrap();

        assert_eq!(
            list_terms(&pool).await.unwrap(),
            vec!["2027-2028/1".to_string(), TERM.to_string()]
        );
    }

    /// Saat tavanı öğrenci sayısına bakar; geçen yılın öğrencileri bu yılın
    /// sayısını şişirmemelidir.
    #[tokio::test]
    async fn count_by_company_is_scoped_to_term() {
        let (_dir, pool) = test_pool().await;
        let company_id = a_company(&pool, "Test İşletme A").await;

        create(&pool, &sample("Ahmet", "Yilmaz", "12/C", Some(company_id))).await.unwrap();

        let mut last_year = sample("Eski", "Ogrenci", "12/C", Some(company_id));
        last_year.term = "2025-2026/1".into();
        create(&pool, &last_year).await.unwrap();

        let counts = count_by_company(&pool, TERM).await.unwrap();
        assert_eq!(counts.iter().find(|(id, _)| *id == company_id).unwrap().1, 1);
    }

    /// Aynı öğrenci ertesi yıl tekrar kaydedilebilmeli.
    #[tokio::test]
    async fn same_student_in_a_later_term_is_not_a_duplicate() {
        let (_dir, pool) = test_pool().await;
        let mut this_year = sample("Ahmet", "Yilmaz", "12/C", None);
        this_year.student_no = Some("9101".into());
        create(&pool, &this_year).await.unwrap();

        let mut next_year = this_year.clone();
        next_year.term = "2027-2028/1".into();

        assert!(find_duplicate(&pool, &next_year).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn find_duplicate_matches_on_student_no_first() {
        let (_dir, pool) = test_pool().await;
        let mut existing = sample("Ahmet", "Yilmaz", "12/C", None);
        existing.student_no = Some("9101".into());
        create(&pool, &existing).await.unwrap();

        // Aynı numara, tamamen farklı ad: yine de aynı öğrencidir.
        let mut candidate = sample("Bambaska", "Isim", "12/D", None);
        candidate.student_no = Some("9101".into());
        assert!(find_duplicate(&pool, &candidate).await.unwrap().is_some());

        // Farklı numara: farklı öğrenci.
        let mut other = sample("Ahmet", "Yilmaz", "12/C", None);
        other.student_no = Some("9102".into());
        assert!(find_duplicate(&pool, &other).await.unwrap().is_none());
    }

    /// Gerçek veride aynı sınıfta aynı ad ve soyada sahip iki farklı öğrenci var
    /// (Kurgusal Kişi, 12/D — numaraları 9101 ve 9102, dalları farklı).
    /// Bunlar ayrı kayıt olarak durmalı.
    #[tokio::test]
    async fn two_students_with_same_name_and_grade_are_distinct() {
        let (_dir, pool) = test_pool().await;

        let mut first = sample("Mehmet", "Yildiz", "12/D", None);
        first.student_no = Some("9101".into());
        first.branch = "Elektrik Tesisatları ve Pano Montörlüğü".into();
        create(&pool, &first).await.unwrap();

        let mut second = sample("Mehmet", "Yildiz", "12/D", None);
        second.student_no = Some("9102".into());
        second.branch = "Endüstriyel Bakım Onarım".into();

        assert!(
            find_duplicate(&pool, &second).await.unwrap().is_none(),
            "farklı numaralı iki öğrenci aynı sayılmamalı"
        );
    }

    /// Numara yoksa ad + soyad + sınıf + DAL dörtlüsüne düşülür.
    #[tokio::test]
    async fn find_duplicate_falls_back_to_name_grade_and_branch() {
        let (_dir, pool) = test_pool().await;
        create(&pool, &sample("AHMET", "YILMAZ", "12/C", None)).await.unwrap();

        // Büyük/küçük harf ve boşluk farkı eşleşmeyi bozmamalı.
        let candidate = sample("  ahmet  ", "yilmaz", " 12/c ", None);
        assert!(find_duplicate(&pool, &candidate).await.unwrap().is_some());

        // Farklı sınıf farklı öğrencidir.
        assert!(find_duplicate(&pool, &sample("Ahmet", "Yilmaz", "12/D", None))
            .await
            .unwrap()
            .is_none());

        // Farklı dal farklı öğrencidir.
        let mut other_branch = sample("Ahmet", "Yilmaz", "12/C", None);
        other_branch.branch = "Endüstriyel Bakım Onarım".into();
        assert!(find_duplicate(&pool, &other_branch).await.unwrap().is_none());
    }

    /// Numarası olan bir kayıt, numarasız bir adayı yutmamalı.
    #[tokio::test]
    async fn numbered_record_does_not_swallow_unnumbered_candidate() {
        let (_dir, pool) = test_pool().await;
        let mut existing = sample("Ahmet", "Yilmaz", "12/C", None);
        existing.student_no = Some("9101".into());
        create(&pool, &existing).await.unwrap();

        let candidate = sample("Ahmet", "Yilmaz", "12/C", None);
        assert!(find_duplicate(&pool, &candidate).await.unwrap().is_none());
    }
}
