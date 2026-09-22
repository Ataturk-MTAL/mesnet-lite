use crate::domain::models::{NewStudent, Student};
use crate::error::{AppError, AppResult};
use sqlx::{SqliteConnection, SqlitePool};

/// `students.company_id` sütunu BİLİNÇLİ OLARAK burada YOK ve okunmaz: sütun
/// yalnız `0006_history.sql`in tek seferlik "opening" tohumundan sonra
/// donmuş kalır, kapıdan geçen hiçbir nakil/ayrılış/birleştirme onu
/// güncellemez. Yerleştirmenin tek doğruluk kaynağı `student_placements`
/// projeksiyonudur (spec §4.1); bu yüzden `company_id` öğrencinin KENDİ
/// dönemi için açık (`valid_to IS NULL`) yerleştirme satırından gelir.
const SELECT_COLUMNS: &str = "s.id, s.first_name, s.last_name, s.student_no, s.grade, s.branch, \
     p.company_id AS company_id, s.submitted_at, s.term";

/// `SELECT_COLUMNS`in beklediği JOIN; okuyan her sorgu bunu paylaşır (DRY) —
/// aksi hâlde bir kopya güncellenip diğeri unutulursa proje yeniden eski
/// donuk sütunu okumaya döner.
const PLACEMENT_JOIN: &str = "LEFT JOIN student_placements p \
     ON p.student_id = s.id AND p.term = s.term AND p.valid_to IS NULL";

fn not_found(id: i64) -> AppError {
    AppError::NotFound(format!("Öğrenci bulunamadı: {id}"))
}

/// `get`in aynı transaction'daki bağlantı üzerinden çalışan hâli. `INSERT`/
/// `UPDATE ... RETURNING` bir JOIN'i döndüremediği için (`SELECT_COLUMNS`
/// `p.company_id`ye ihtiyaç duyar), yerinde yazan fonksiyonlar satırı yazdıktan
/// SONRA bununla geri okur.
async fn get_in(conn: &mut SqliteConnection, id: i64) -> AppResult<Student> {
    let sql = format!("SELECT {SELECT_COLUMNS} FROM students s {PLACEMENT_JOIN} WHERE s.id = ?1");
    Ok(sqlx::query_as::<_, Student>(&sql)
        .bind(id)
        .fetch_one(&mut *conn)
        .await?)
}

/// TÜM dönemlerdeki öğrenciler. Ekranlar genelde `list_by_term` kullanır.
pub async fn list(pool: &SqlitePool) -> AppResult<Vec<Student>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM students s {PLACEMENT_JOIN}
         ORDER BY s.term DESC, s.grade COLLATE NOCASE, s.last_name COLLATE NOCASE, \
                  s.first_name COLLATE NOCASE"
    );
    Ok(sqlx::query_as::<_, Student>(&sql).fetch_all(pool).await?)
}

/// Yalnızca verilen eğitim-öğretim yılındaki öğrenciler.
pub async fn list_by_term(pool: &SqlitePool, term: &str) -> AppResult<Vec<Student>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM students s {PLACEMENT_JOIN} WHERE s.term = ?1
         ORDER BY s.grade COLLATE NOCASE, s.last_name COLLATE NOCASE, s.first_name COLLATE NOCASE"
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
        "SELECT {SELECT_COLUMNS} FROM students s {PLACEMENT_JOIN} WHERE s.term = ?1
         ORDER BY s.grade COLLATE NOCASE, s.last_name COLLATE NOCASE, s.first_name COLLATE NOCASE"
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
    let sql = format!("SELECT {SELECT_COLUMNS} FROM students s {PLACEMENT_JOIN} WHERE s.id = ?1");
    Ok(sqlx::query_as::<_, Student>(&sql)
        .bind(id)
        .fetch_one(pool)
        .await?)
}

/// Bir işletmedeki öğrenciler — dönem bazlı. İşletme kalıcıdır, öğrenciler değil.
/// Filtre projeksiyondaki AÇIK yerleştirme satırına göredir (`INNER JOIN`
/// yeterli: eşleşmeyenler zaten o işletmede değildir).
pub async fn list_by_company(
    pool: &SqlitePool,
    company_id: i64,
    term: &str,
) -> AppResult<Vec<Student>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM students s
         JOIN student_placements p
             ON p.student_id = s.id AND p.term = s.term AND p.valid_to IS NULL
         WHERE p.company_id = ?1 AND s.term = ?2
         ORDER BY s.last_name COLLATE NOCASE, s.first_name COLLATE NOCASE"
    );
    Ok(sqlx::query_as::<_, Student>(&sql)
        .bind(company_id)
        .bind(term)
        .fetch_all(pool)
        .await?)
}

/// İşletme başına öğrenci sayısı — dönem bazlı, doğrudan projeksiyondan.
/// Saat tavanı kuralları bu sayıyı kullanır, bu yüzden dönem filtresi zorunludur:
/// geçen yılın öğrencileri bu yılın tavanını yükseltmemelidir.
pub async fn count_by_company(pool: &SqlitePool, term: &str) -> AppResult<Vec<(i64, i64)>> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT company_id, COUNT(*) FROM student_placements
         WHERE term = ?1 AND valid_to IS NULL GROUP BY company_id",
    )
    .bind(term)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn create(pool: &SqlitePool, input: &NewStudent) -> AppResult<Student> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO students
            (first_name, last_name, student_no, grade, branch, submitted_at, term)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         RETURNING id",
    )
    .bind(&input.first_name)
    .bind(&input.last_name)
    .bind(&input.student_no)
    .bind(&input.grade)
    .bind(&input.branch)
    .bind(&input.submitted_at)
    .bind(&input.term)
    .fetch_one(pool)
    .await?;

    get(pool, id).await
}

/// `change_service::execute_in` (R4) yerinde oluşturma adımı içindir
/// (spec §5 adım 2) — `createStudent` komutu, `decide`'dan ÖNCE, aynı
/// transaction'daki bağlantı üzerinden bu satırı açar. Yerleştirme burada
/// YAZILMAZ: satırın henüz hiçbir `student_placements` satırı yoktur
/// (`decide` bunu `StudentPlaced` olayıyla SONRA üretir); `PLACEMENT_JOIN`
/// bu yüzden eşleşmez ve `company_id` doğal olarak `NULL` döner — bu
/// doğrudur, yanlış değer değildir. `INSERT ... RETURNING` bir JOIN'i
/// döndüremediği için burada satır ayrıca `id` ile geri okunur.
pub async fn create_in(conn: &mut SqliteConnection, input: &NewStudent) -> AppResult<Student> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO students
            (first_name, last_name, student_no, grade, branch, submitted_at, term)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         RETURNING id",
    )
    .bind(&input.first_name)
    .bind(&input.last_name)
    .bind(&input.student_no)
    .bind(&input.grade)
    .bind(&input.branch)
    .bind(&input.submitted_at)
    .bind(&input.term)
    .fetch_one(&mut *conn)
    .await?;

    get_in(conn, id).await
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
    let affected = sqlx::query(
        "UPDATE students SET first_name = ?1, last_name = ?2, grade = ?3, branch = ?4 WHERE id = ?5",
    )
    .bind(first_name)
    .bind(last_name)
    .bind(grade)
    .bind(branch)
    .bind(id)
    .execute(&mut *conn)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(not_found(id));
    }
    get_in(conn, id).await
}

/// Yerleştirmeye DOKUNMAZ: `company_id` artık `NewStudent`de yok, sütun
/// yazılmaz. Bir öğrencinin işletmesini değiştirmenin tek yolu tarihçe
/// kapısıdır (`PlaceStudent`/`TransferStudent`/`StudentLeaves`).
pub async fn update(pool: &SqlitePool, id: i64, input: &NewStudent) -> AppResult<Student> {
    let affected = sqlx::query(
        "UPDATE students SET
            first_name = ?1, last_name = ?2, student_no = ?3, grade = ?4,
            branch = ?5, submitted_at = ?6, term = ?7
         WHERE id = ?8",
    )
    .bind(&input.first_name)
    .bind(&input.last_name)
    .bind(&input.student_no)
    .bind(&input.grade)
    .bind(&input.branch)
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
/// (bkz. `find_duplicate_in` üstündeki yorum), o yüzden bu yardımcı paylaşılır.
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
///
/// Yalnızca bağlantı-bazlı (`_in`) hâli vardır: TEK gerçek çağıranı
/// `services::import_apply` (CSV içe aktarımı) TÜM okuma+yazmayı tek
/// `BEGIN IMMEDIATE` transaction'ında yapar (`list_by_term_in` üstündeki
/// yorumla aynı gerekçe: havuzdan okumak açık transaction sırasında
/// sessizce eski veriyi döndürebilir); ayrı bir havuz-bazlı sürüm bu yüzden
/// tutulmaz — kullanılmayan bir kopya olurdu.
pub async fn find_duplicate_in(
    conn: &mut SqliteConnection,
    candidate: &NewStudent,
) -> AppResult<Option<Student>> {
    let rows = list_by_term_in(conn, &candidate.term).await?;
    Ok(find_duplicate_among(&rows, candidate))
}

fn find_duplicate_among(rows: &[Student], candidate: &NewStudent) -> Option<Student> {
    if let Some(candidate_no) = non_empty(candidate.student_no.as_ref()) {
        return rows
            .iter()
            .find(|s| non_empty(s.student_no.as_ref()).as_deref() == Some(candidate_no.as_str()))
            .cloned();
    }

    let key = fallback_key(
        &candidate.first_name,
        &candidate.last_name,
        &candidate.grade,
        &candidate.branch,
    );

    rows.iter()
        .find(|s| {
            // Numarası olan bir kayıt, numarasız bir adayla ad üzerinden eşleşmez;
            // aksi hâlde numarası girilmemiş yeni bir öğrenci yanlışlıkla yutulur.
            non_empty(s.student_no.as_ref()).is_none()
                && fallback_key(&s.first_name, &s.last_name, &s.grade, &s.branch) == key
        })
        .cloned()
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
    use crate::db::{companies, init_pool, terms};
    use crate::domain::history::decide::{ChangeCommand, ChangeRequest, NewStudentInput, TransferTarget};
    use crate::domain::models::NewCompany;
    use crate::domain::terms::TermDates;
    use crate::services::change_service::{execute_change, ChangeMode, ChangeOutcome};
    use chrono::NaiveDate;

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

    /// Dönem 1 Eylül'de başlar (migration 0006 tohumu); bu gün planlama
    /// evresindedir — `effectiveDate` zorunlu değildir (MADDE 5/1-ç).
    fn planning_today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 15).unwrap()
    }

    /// Dönem başladıktan SONRAki bir "bugün": ilk yerleştirmeden (dönem
    /// başlangıcı, `planning_today` ile açılır) sonra gelen bir nakil/ayrılış
    /// için kullanılır — aynı GÜNE iki olay koyarsak `validate_placement_change`
    /// (`state_before` d'den KESİN ÖNCEyi bakar) ilk olayı hiç görmez.
    fn november_today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()
    }

    fn sample(first: &str, last: &str, grade: &str) -> NewStudent {
        NewStudent {
            first_name: first.into(),
            last_name: last.into(),
            student_no: None,
            grade: grade.into(),
            branch: "Elektronik Haberleşme".into(),
            submitted_at: Some("2026-09-11".into()),
            term: TERM.into(),
        }
    }

    fn create_student_request(first: &str, last: &str, grade: &str, company_id: Option<i64>) -> ChangeRequest {
        ChangeRequest {
            term: TERM.to_string(),
            effective_date: None,
            document_date: None,
            reason: "test".to_string(),
            command: ChangeCommand::CreateStudent {
                student: NewStudentInput {
                    first_name: first.into(),
                    last_name: last.into(),
                    student_no: None,
                    grade: grade.into(),
                    branch: "Elektronik Haberleşme".into(),
                    submitted_at: None,
                },
                company_id,
            },
        }
    }

    async fn commit(pool: &SqlitePool, req: ChangeRequest) -> ChangeOutcome {
        commit_on(pool, req, planning_today()).await
    }

    async fn commit_on(pool: &SqlitePool, req: ChangeRequest, today: NaiveDate) -> ChangeOutcome {
        execute_change(pool, req, ChangeMode::Commit { expected_high_water: None }, today)
            .await
            .unwrap()
    }

    fn expect_committed(outcome: ChangeOutcome) {
        assert!(matches!(outcome, ChangeOutcome::Committed { .. }), "Committed beklenirdi: {outcome:?}");
    }

    /// `db/students.rs`in okuma sorgularını GERÇEK yazma yolu (`change_service`)
    /// ÜZERİNDEN test eder: ham `create()` artık yerleştirme YAZMAZ (bkz.
    /// `NewStudent` başındaki yorum), bu yüzden bir öğrenciyi bir işletmeye
    /// "yerleştirmiş" olarak kurmanın tek yolu tarihçe kapısıdır.
    async fn create_placed_student(pool: &SqlitePool, first: &str, last: &str, grade: &str, company_id: Option<i64>) -> i64 {
        expect_committed(commit(pool, create_student_request(first, last, grade, company_id)).await);
        sqlx::query_scalar("SELECT id FROM students WHERE first_name = ?1 AND last_name = ?2")
            .bind(first)
            .bind(last)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// `count_by_company_is_scoped_to_term`in geçmiş dönem senaryosu için:
    /// gerçek tarihçe kapısı geçmiş bir dönemde kayıt açmaya izin vermez (ay
    /// penceresi, MADDE 5/1-ç). Bu yüzden geçmiş dönemin "açılış" durumu,
    /// migration 0006'nın gerçek veriyi tohumlarken yaptığının AYNISIYLA
    /// (elle bir `change_sets`/`change_events`/`student_placements` üçlüsü)
    /// taklit edilir.
    async fn seed_opening_placement(pool: &SqlitePool, term: &str, student_id: i64, company_id: i64) {
        let dates = TermDates::default_for(term);
        let exists: Option<String> = sqlx::query_scalar("SELECT term FROM terms WHERE term = ?1")
            .bind(term)
            .fetch_optional(pool)
            .await
            .unwrap();
        if exists.is_none() {
            let mut conn = pool.acquire().await.unwrap();
            terms::insert_in(&mut conn, &dates).await.unwrap();
        }

        let start = dates.start.to_string();
        let change_set_id: i64 = sqlx::query_scalar(
            "INSERT INTO change_sets
                (term, kind, effective_date, document_date, reason, actor, recorded_at, revokes_change_set_id, impact_json)
             VALUES (?1, 'opening', ?2, NULL, 'test', 'tester', datetime('now'), NULL, '{}')
             RETURNING id",
        )
        .bind(term)
        .bind(&start)
        .fetch_one(pool)
        .await
        .unwrap();

        let payload = format!(
            r#"{{"toCompanyId":{company_id},"fromCompanyId":null,"source":"opening","labels":{{}}}}"#
        );
        let event_id: i64 = sqlx::query_scalar(
            "INSERT INTO change_events
                (change_set_id, stream, subject_id, term, kind, kind_version, effective_date, payload, caused_by, revokes)
             VALUES (?1, 'placement', ?2, ?3, 'student_placed', 1, ?4, ?5, NULL, NULL)
             RETURNING id",
        )
        .bind(change_set_id)
        .bind(student_id)
        .bind(term)
        .bind(&start)
        .bind(&payload)
        .fetch_one(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO student_placements (student_id, term, company_id, valid_from, valid_to, source_event_id)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
        )
        .bind(student_id)
        .bind(term)
        .bind(company_id)
        .bind(&start)
        .bind(event_id)
        .execute(pool)
        .await
        .unwrap();
    }

    /// Üretimde `find_duplicate_in`in TEK gerçek çağıranı (`services::import_apply`)
    /// zaten açık bir transaction'ın bağlantısı üzerinden çağırır; havuz-bazlı
    /// bir sürüm bu yüzden production'da yok (kullanılmayan bir kopya olurdu).
    /// Bu yardımcı yalnız TESTLERİN havuzdan tek seferlik bir bağlantı almasını
    /// kolaylaştırır.
    async fn find_duplicate(pool: &SqlitePool, candidate: &NewStudent) -> AppResult<Option<Student>> {
        let mut conn = pool.acquire().await.unwrap();
        find_duplicate_in(&mut conn, candidate).await
    }

    #[tokio::test]
    async fn create_then_get_returns_same_record() {
        let (_dir, pool) = test_pool().await;

        let created = create(&pool, &sample("Ahmet", "Yılmaz", "12/C")).await.unwrap();
        let fetched = get(&pool, created.id).await.unwrap();

        assert_eq!(fetched.first_name, "Ahmet");
        // Ham `create()` yerleştirme YAZMAZ; işletme yalnız tarihçe kapısından
        // (`PlaceStudent`/`CreateStudent{ companyId }`) gelir.
        assert_eq!(fetched.company_id, None);
        // CSV'de öğrenci no boş olabilir; None olarak kalmalı.
        assert_eq!(fetched.student_no, None);
    }

    /// Teşhis: kapıdan (`CreateStudent{ companyId: Some(...) }`) oluşturulan
    /// bir öğrencinin işletmesi `get`in okuduğu projeksiyondan gelir; artık
    /// donuk `students.company_id` sütunundan DEĞİL.
    #[tokio::test]
    async fn company_id_comes_from_the_open_placement_not_the_legacy_column() {
        let (_dir, pool) = test_pool().await;
        let company_id = a_company(&pool, "Test İşletme A").await;

        let student_id = create_placed_student(&pool, "Ahmet", "Yılmaz", "12/C", Some(company_id)).await;

        assert_eq!(get(&pool, student_id).await.unwrap().company_id, Some(company_id));
    }

    /// Teşhis'in kanıtı: kapıdan geçen bir NAKİL, `students::list`in okuduğu
    /// projeksiyonu günceller. Bu test eski koda karşı KIRMIZI başlardı: eski
    /// `list` sütunu okuyordu ve nakilden sonra hâlâ eski işletmeyi gösterirdi.
    #[tokio::test]
    async fn list_reflects_a_transfer_through_the_gate() {
        let (_dir, pool) = test_pool().await;
        let from = a_company(&pool, "İşletme A").await;
        let to = a_company(&pool, "İşletme B").await;
        let student_id = create_placed_student(&pool, "Ahmet", "Yılmaz", "12/C", Some(from)).await;

        // İlk yerleştirme dönem başlangıcında (`planning_today`) açılır; nakil
        // AYNI güne değil, SONRAsına konur — aksi hâlde `validate_placement_change`
        // (`state_before` d'den KESİN ÖNCEyi bakar) ilk olayı hiç göremez.
        let transfer = ChangeRequest {
            term: TERM.to_string(),
            effective_date: Some(NaiveDate::from_ymd_opt(2026, 11, 3).unwrap()),
            document_date: None,
            reason: "test".to_string(),
            command: ChangeCommand::TransferStudent {
                student_id,
                from_company_id: from,
                to: TransferTarget::Existing { company_id: to },
            },
        };
        expect_committed(commit_on(&pool, transfer, november_today()).await);

        let listed = list(&pool).await.unwrap();
        let student = listed.iter().find(|s| s.id == student_id).unwrap();
        assert_eq!(student.company_id, Some(to), "nakilden sonra YENİ işletme görünmeli");
    }

    /// `count_by_company` da aynı projeksiyonu okur: nakil sonrası eski
    /// işletmede sayı düşer, yeni işletmede yükselir.
    #[tokio::test]
    async fn count_by_company_moves_with_a_transfer() {
        let (_dir, pool) = test_pool().await;
        let from = a_company(&pool, "İşletme A").await;
        let to = a_company(&pool, "İşletme B").await;
        let student_id = create_placed_student(&pool, "Ahmet", "Yılmaz", "12/C", Some(from)).await;

        let transfer = ChangeRequest {
            term: TERM.to_string(),
            effective_date: Some(NaiveDate::from_ymd_opt(2026, 11, 3).unwrap()),
            document_date: None,
            reason: "test".to_string(),
            command: ChangeCommand::TransferStudent {
                student_id,
                from_company_id: from,
                to: TransferTarget::Existing { company_id: to },
            },
        };
        expect_committed(commit_on(&pool, transfer, november_today()).await);

        let counts = count_by_company(&pool, TERM).await.unwrap();
        assert_eq!(counts.iter().find(|(id, _)| *id == from).map(|(_, n)| *n).unwrap_or(0), 0, "eski işletmede kalmamalı");
        assert_eq!(counts.iter().find(|(id, _)| *id == to).unwrap().1, 1, "yeni işletmede görünmeli");
    }

    /// Ayrılan (`StudentLeaves`) öğrenci `company_id: None` döner.
    #[tokio::test]
    async fn student_leaves_clears_company_id() {
        let (_dir, pool) = test_pool().await;
        let company_id = a_company(&pool, "İşletme A").await;
        let student_id = create_placed_student(&pool, "Ahmet", "Yılmaz", "12/C", Some(company_id)).await;

        let leaves = ChangeRequest {
            term: TERM.to_string(),
            effective_date: Some(NaiveDate::from_ymd_opt(2026, 11, 3).unwrap()),
            document_date: None,
            reason: "test".to_string(),
            command: ChangeCommand::StudentLeaves { student_id, from_company_id: company_id },
        };
        expect_committed(commit_on(&pool, leaves, november_today()).await);

        assert_eq!(get(&pool, student_id).await.unwrap().company_id, None);
    }

    #[tokio::test]
    async fn list_by_company_returns_only_that_companys_students() {
        let (_dir, pool) = test_pool().await;
        let a = a_company(&pool, "İşletme A").await;
        let b = a_company(&pool, "İşletme B").await;

        create_placed_student(&pool, "Ahmet", "Yılmaz", "12/C", Some(a)).await;
        create_placed_student(&pool, "Ayşe", "Demir", "12/C", Some(a)).await;
        create_placed_student(&pool, "Mehmet", "Kaya", "12/D", Some(b)).await;

        assert_eq!(list_by_company(&pool, a, TERM).await.unwrap().len(), 2);
        assert_eq!(list_by_company(&pool, b, TERM).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn count_by_company_groups_correctly() {
        let (_dir, pool) = test_pool().await;
        let a = a_company(&pool, "İşletme A").await;
        let b = a_company(&pool, "İşletme B").await;

        create_placed_student(&pool, "Ahmet", "Yılmaz", "12/C", Some(a)).await;
        create_placed_student(&pool, "Ayşe", "Demir", "12/C", Some(a)).await;
        create_placed_student(&pool, "Mehmet", "Kaya", "12/D", Some(b)).await;

        let counts = count_by_company(&pool, TERM).await.unwrap();
        assert_eq!(counts.iter().find(|(id, _)| *id == a).unwrap().1, 2);
        assert_eq!(counts.iter().find(|(id, _)| *id == b).unwrap().1, 1);
    }

    /// İşletme silinince öğrenci SATIRI silinmez.
    ///
    /// Eski test burada `students.company_id`nin `ON DELETE SET NULL` ile
    /// boşaldığını da doğruluyordu; o iddia projeksiyona geçişle ANLAMSIZLAŞTI
    /// (davranış değişmedi, `company_id`nin okuma KAYNAĞI değişti — ham
    /// `companies::remove` `student_placements`e hiç dokunmaz, o sütun zaten
    /// artık okunmuyor). Kalan gerçek garanti şu: öğrenci kaydı ayakta kalır.
    #[tokio::test]
    async fn deleting_company_does_not_delete_the_student() {
        let (_dir, pool) = test_pool().await;
        let company_id = a_company(&pool, "Test İşletme A").await;
        let student_id = create_placed_student(&pool, "Ahmet", "Yılmaz", "12/C", Some(company_id)).await;

        companies::remove(&pool, company_id).await.unwrap();

        assert!(get(&pool, student_id).await.is_ok(), "öğrenci kaydı silinmemeli");
    }

    /// Teşhis edilen asıl zarar: açık bir yerleştirmesi olan işletme
    /// silinince (artık pasifleştirilince, spec §5.4) `student_placements`
    /// satırı sarkan bir `company_id` bırakmamalı — hâlâ VAR OLAN bir
    /// işletmeye işaret etmeli. `companies::get` bu id ile başarıyla dönerse
    /// kanıtlanmış olur (sert silinseydi `NotFound` dönerdi).
    #[tokio::test]
    async fn deleting_a_company_with_a_placement_leaves_the_placement_pointing_at_a_real_company() {
        let (_dir, pool) = test_pool().await;
        let company_id = a_company(&pool, "Test İşletme A").await;
        create_placed_student(&pool, "Ahmet", "Yılmaz", "12/C", Some(company_id)).await;

        let result = companies::remove(&pool, company_id).await.unwrap();
        assert!(result.soft_deleted, "açık yerleştirmesi olan işletme pasife alınmalı, silinmemeli");

        let placement_company_id: i64 = sqlx::query_scalar(
            "SELECT company_id FROM student_placements WHERE student_id = (
                 SELECT id FROM students WHERE first_name = 'Ahmet' AND last_name = 'Yılmaz'
             ) AND valid_to IS NULL",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(placement_company_id, company_id);
        assert!(
            companies::get(&pool, placement_company_id).await.is_ok(),
            "yerleştirmenin işaret ettiği işletme hâlâ var olmalı (sarkan kimlik değil)"
        );
    }

    #[tokio::test]
    async fn create_in_writes_through_the_given_connection() {
        let (_dir, pool) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let created = create_in(&mut conn, &sample("Bağlantı", "Testi", "12/C")).await.unwrap();
        assert_eq!(get(&pool, created.id).await.unwrap().first_name, "Bağlantı");
    }

    #[tokio::test]
    async fn remove_in_deletes_the_row() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample("Ahmet", "Yılmaz", "12/C")).await.unwrap();

        let mut conn = pool.acquire().await.unwrap();
        remove_in(&mut conn, created.id).await.unwrap();

        assert!(matches!(get(&pool, created.id).await.unwrap_err(), AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn update_changes_fields() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample("Ahmet", "Yılmaz", "12/C")).await.unwrap();

        let mut input = sample("Ahmet", "Yılmaz", "12/D");
        input.student_no = Some("1234".into());
        let updated = update(&pool, created.id, &input).await.unwrap();

        assert_eq!(updated.grade, "12/D");
        assert_eq!(updated.student_no.as_deref(), Some("1234"));
    }

    /// `update` yalnız kimlik alanlarını değiştirir; projeksiyondaki
    /// yerleştirmeye DOKUNMAZ (`NewStudent`de artık `company_id` yok).
    #[tokio::test]
    async fn update_does_not_change_the_students_placement() {
        let (_dir, pool) = test_pool().await;
        let company_id = a_company(&pool, "İşletme A").await;
        let student_id = create_placed_student(&pool, "Ahmet", "Yılmaz", "12/C", Some(company_id)).await;

        let mut input = sample("Ahmet", "Yılmaz", "12/D");
        input.student_no = Some("1234".into());
        let updated = update(&pool, student_id, &input).await.unwrap();

        assert_eq!(updated.grade, "12/D");
        assert_eq!(updated.company_id, Some(company_id), "update yerleştirmeyi değiştirmemeli");
    }

    #[tokio::test]
    async fn remove_deletes_record() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample("Ahmet", "Yılmaz", "12/C")).await.unwrap();

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

        create(&pool, &sample("Ahmet", "Yilmaz", "12/C")).await.unwrap();

        let mut next_year = sample("Ayse", "Demir", "12/C");
        next_year.term = "2027-2028/1".into();
        create(&pool, &next_year).await.unwrap();

        assert_eq!(list(&pool).await.unwrap().len(), 2, "tüm dönemler");
        assert_eq!(list_by_term(&pool, TERM).await.unwrap().len(), 1);
        assert_eq!(list_by_term(&pool, "2027-2028/1").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn list_terms_returns_distinct_terms_newest_first() {
        let (_dir, pool) = test_pool().await;
        create(&pool, &sample("Ahmet", "Yilmaz", "12/C")).await.unwrap();

        let mut next_year = sample("Ayse", "Demir", "12/C");
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

        create_placed_student(&pool, "Ahmet", "Yilmaz", "12/C", Some(company_id)).await;

        let last_year_student = create(&pool, &{
            let mut s = sample("Eski", "Ogrenci", "12/C");
            s.term = "2025-2026/1".into();
            s
        })
        .await
        .unwrap();
        seed_opening_placement(&pool, "2025-2026/1", last_year_student.id, company_id).await;

        let counts = count_by_company(&pool, TERM).await.unwrap();
        assert_eq!(counts.iter().find(|(id, _)| *id == company_id).unwrap().1, 1);
    }

    /// Aynı öğrenci ertesi yıl tekrar kaydedilebilmeli.
    #[tokio::test]
    async fn same_student_in_a_later_term_is_not_a_duplicate() {
        let (_dir, pool) = test_pool().await;
        let mut this_year = sample("Ahmet", "Yilmaz", "12/C");
        this_year.student_no = Some("9101".into());
        create(&pool, &this_year).await.unwrap();

        let mut next_year = this_year.clone();
        next_year.term = "2027-2028/1".into();

        assert!(find_duplicate(&pool, &next_year).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn find_duplicate_matches_on_student_no_first() {
        let (_dir, pool) = test_pool().await;
        let mut existing = sample("Ahmet", "Yilmaz", "12/C");
        existing.student_no = Some("9101".into());
        create(&pool, &existing).await.unwrap();

        // Aynı numara, tamamen farklı ad: yine de aynı öğrencidir.
        let mut candidate = sample("Bambaska", "Isim", "12/D");
        candidate.student_no = Some("9101".into());
        assert!(find_duplicate(&pool, &candidate).await.unwrap().is_some());

        // Farklı numara: farklı öğrenci.
        let mut other = sample("Ahmet", "Yilmaz", "12/C");
        other.student_no = Some("9102".into());
        assert!(find_duplicate(&pool, &other).await.unwrap().is_none());
    }

    /// Gerçek veride aynı sınıfta aynı ad ve soyada sahip iki farklı öğrenci var
    /// (Kurgusal Kişi, 12/D — numaraları 9101 ve 9102, dalları farklı).
    /// Bunlar ayrı kayıt olarak durmalı.
    #[tokio::test]
    async fn two_students_with_same_name_and_grade_are_distinct() {
        let (_dir, pool) = test_pool().await;

        let mut first = sample("Mehmet", "Yildiz", "12/D");
        first.student_no = Some("9101".into());
        first.branch = "Elektrik Tesisatları ve Pano Montörlüğü".into();
        create(&pool, &first).await.unwrap();

        let mut second = sample("Mehmet", "Yildiz", "12/D");
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
        create(&pool, &sample("AHMET", "YILMAZ", "12/C")).await.unwrap();

        // Büyük/küçük harf ve boşluk farkı eşleşmeyi bozmamalı.
        let candidate = sample("  ahmet  ", "yilmaz", " 12/c ");
        assert!(find_duplicate(&pool, &candidate).await.unwrap().is_some());

        // Farklı sınıf farklı öğrencidir.
        assert!(find_duplicate(&pool, &sample("Ahmet", "Yilmaz", "12/D"))
            .await
            .unwrap()
            .is_none());

        // Farklı dal farklı öğrencidir.
        let mut other_branch = sample("Ahmet", "Yilmaz", "12/C");
        other_branch.branch = "Endüstriyel Bakım Onarım".into();
        assert!(find_duplicate(&pool, &other_branch).await.unwrap().is_none());
    }

    /// Numarası olan bir kayıt, numarasız bir adayı yutmamalı.
    #[tokio::test]
    async fn numbered_record_does_not_swallow_unnumbered_candidate() {
        let (_dir, pool) = test_pool().await;
        let mut existing = sample("Ahmet", "Yilmaz", "12/C");
        existing.student_no = Some("9101".into());
        create(&pool, &existing).await.unwrap();

        let candidate = sample("Ahmet", "Yilmaz", "12/C");
        assert!(find_duplicate(&pool, &candidate).await.unwrap().is_none());
    }

    /// `find_duplicate_in` doğrudan bir bağlantı üzerinden de aynı kuralı
    /// uygular; `find_duplicate` (yukarıdaki test yardımcısı) yalnız havuzdan
    /// bağlantı almanın kısayoludur, ayrı bir kural DEĞİLDİR.
    #[tokio::test]
    async fn find_duplicate_in_works_directly_on_a_connection() {
        let (_dir, pool) = test_pool().await;
        let mut existing = sample("Ahmet", "Yilmaz", "12/C");
        existing.student_no = Some("9101".into());
        create(&pool, &existing).await.unwrap();

        let mut conn = pool.acquire().await.unwrap();
        let mut candidate = sample("Ahmet", "Yilmaz", "12/C");
        candidate.student_no = Some("9101".into());
        assert!(find_duplicate_in(&mut conn, &candidate).await.unwrap().is_some());
    }
}
