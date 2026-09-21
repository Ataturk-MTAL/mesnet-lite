use crate::db::{teachers, terms};
use crate::domain::group_count;
use crate::domain::terms::today_local;
use crate::error::{AppError, AppResult};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::BTreeMap;

/// Bir dönemdeki sınıf+dal için haftalık ders saati ve grup sayısı.
///
/// Okulun ders yükü havuzu (MADDE 15/2) bu satırların toplamıdır:
/// Σ (haftalık ders saati × ETKİN grup sayısı). Saat ile grup sayısı TEK
/// satırda durur; eski iki-JSON tasarımındaki anahtar eşleşmezliği (bkz.
/// migration 0005) burada yapısal olarak imkânsızdır.
///
/// `group_count` her zaman etkin değer DEĞİLDİR: `is_group_manual` false ise
/// etkin değer öğrenci sayısından okuma anında hesaplanır ve `group_count`
/// yalnız önbellektir (bkz. migration 0007, `effective_group_count`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TermBranchHours {
    pub id: i64,
    pub term: String,
    pub grade: String,
    pub branch: String,
    pub weekly_hours: i64,
    pub group_count: i64,
    /// Grup sayısı kullanıcı tarafından elle mi girildi (migration 0007)?
    pub is_group_manual: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Ders yükü ekranından gelen tek satırlık girdi.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermBranchHoursInput {
    pub grade: String,
    pub branch: String,
    pub weekly_hours: i64,
    /// Yalnız `is_group_manual` true iken anlamlıdır; otomatik satırda
    /// istemcinin gönderdiği değer yok sayılır (kural tek yerde: sunucu).
    pub group_count: i64,
    /// Eksikse false: eski istemciler ve otomatik satırlar bu değeri yollamaz.
    #[serde(default)]
    pub is_group_manual: bool,
}

const SELECT_COLUMNS: &str =
    "id, term, grade, branch, weekly_hours, group_count, is_group_manual, created_at, updated_at";

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Bir dönemin ders yükü satırları, sınıf ve dala göre sıralı.
///
/// `E` üzerinden geneldir (bkz. `db/assignments.rs::awarded_hours_for`
/// deseni): aynı SQL, hem `&SqlitePool` hem de bir transaction'ın
/// `&mut SqliteConnection`'ı ile çalışır — havuz hesabının transaction
/// içinden (bkz. `db/history_context.rs`) TEK sorgudan geçmesini sağlar.
pub async fn list_for_term<'e, E>(executor: E, term: &str) -> AppResult<Vec<TermBranchHours>>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM term_branch_hours WHERE term = ?1
         ORDER BY grade COLLATE NOCASE, branch COLLATE NOCASE"
    );
    Ok(sqlx::query_as::<_, TermBranchHours>(&sql)
        .bind(term)
        .fetch_all(executor)
        .await?)
}

/// (sınıf, dal) çiftinin öğrenci sayısı. Anahtar, öğrenci kaydındaki metnin
/// aynısıdır (büyük/küçük harf ve boşluk dahil): satırlarla eşleştirme de
/// `merge_with_suggestions`'daki gibi birebir eşitliktir.
pub type BranchStudentCounts = BTreeMap<(String, String), i64>;

/// Öğrenci kayıtlarından türeyen (sınıf, dal, öğrenci sayısı) üçlüleri.
/// Filtre TEK yerde durur: dönem eşleşir, sınıf ve dal boş değildir. Hem
/// öneri satırları hem otomatik grup sayısı buradan beslenir.
pub async fn student_counts_by_branch<'e, E>(
    executor: E,
    term: &str,
) -> AppResult<Vec<(String, String, i64)>>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT grade, branch, COUNT(*) FROM students
         WHERE term = ?1 AND grade <> '' AND branch <> ''
         GROUP BY grade, branch
         ORDER BY grade COLLATE NOCASE, branch COLLATE NOCASE",
    )
    .bind(term)
    .fetch_all(executor)
    .await?;
    Ok(rows)
}

/// `student_counts_by_branch` sonucunu arama tablosuna çevirir.
pub async fn branch_student_counts<'e, E>(executor: E, term: &str) -> AppResult<BranchStudentCounts>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    Ok(student_counts_by_branch(executor, term)
        .await?
        .into_iter()
        .map(|(grade, branch, count)| ((grade, branch), count))
        .collect())
}

/// Öğrenci kayıtlarından türeyen, o dönemde en az bir öğrencisi olan tüm
/// (sınıf, dal) çiftleri. Ders yükü ekranı henüz satırı olmayan çiftler için
/// öneri satırı göstermek üzere bunu kullanır — kullanıcı boru işaretli
/// bileşik anahtarı elle yazmak zorunda kalmamalı.
pub async fn distinct_branches_from_students(
    pool: &SqlitePool,
    term: &str,
) -> AppResult<Vec<(String, String)>> {
    Ok(student_counts_by_branch(pool, term)
        .await?
        .into_iter()
        .map(|(grade, branch, _)| (grade, branch))
        .collect())
}

/// Tabloya (Norm Kadro Yön. MADDE 22/1-ç) göre hesaplanan otomatik grup
/// sayısı. Öğrencisi olmayan çift 0 grup verir.
pub fn auto_group_count(counts: &BranchStudentCounts, grade: &str, branch: &str) -> i64 {
    let students = counts
        .get(&(grade.to_string(), branch.to_string()))
        .copied()
        .unwrap_or(0);
    group_count::groups_for_grade(grade, students)
}

/// Havuzda ve ekranda kullanılan ETKİN grup sayısı: elle satırda saklanan
/// değer, otomatik satırda öğrenci sayısından hesaplanan değer. Bu kural
/// tek noktadadır; havuz da ekran da buradan geçer.
pub fn effective_group_count(saved: &TermBranchHours, auto: i64) -> i64 {
    if saved.is_group_manual {
        saved.group_count
    } else {
        auto
    }
}

/// Σ (haftalık ders saati × ETKİN grup sayısı). Satır yoksa 0 döner; bu
/// "sınırsız" değil "henüz tanımlanmamış" demektir. Otomatik satırın etkin
/// grup sayısı 0 olabilir (öğrencisiz şube havuza katkı vermez).
pub async fn effective_branch_hours(pool: &SqlitePool, term: &str) -> AppResult<i64> {
    let rows = list_for_term(pool, term).await?;
    let counts = branch_student_counts(pool, term).await?;
    Ok(sum_branch_hours(&rows, &counts))
}

/// `effective_branch_hours`'ın transaction-içi (bkz. `db/history_context.rs`)
/// karşılığı: aynı iki sorguyu (`list_for_term`, `branch_student_counts`) TEK
/// bağlantı üzerinden, `&mut *conn` ile ödünç alarak çalıştırır. `&SqlitePool`
/// `Copy` olduğu için pool sürümü aynı iki çağrıyı doğrudan yapabiliyor;
/// `&mut SqliteConnection` `Copy` değildir, bu yüzden bu ince sarmalayıcı
/// (yalnız kompozisyon, SQL'siz) ayrı tutulur — sorgunun kendisi yukarıdaki
/// generic fonksiyonlarda TEK yerde yaşamaya devam eder.
pub async fn effective_branch_hours_in(conn: &mut SqliteConnection, term: &str) -> AppResult<i64> {
    let rows = list_for_term(&mut *conn, term).await?;
    let counts = branch_student_counts(&mut *conn, term).await?;
    Ok(sum_branch_hours(&rows, &counts))
}

fn sum_branch_hours(rows: &[TermBranchHours], counts: &BranchStudentCounts) -> i64 {
    rows.iter()
        .map(|saved| {
            let auto = auto_group_count(counts, &saved.grade, &saved.branch);
            saved.weekly_hours * effective_group_count(saved, auto)
        })
        .sum()
}

/// Koordinatörlük toplam ders yükü (havuz) iki kalemden oluşur; ekran ikisini
/// ayrı göstermek için kırılımı da taşır.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolBreakdown {
    /// Σ (haftalık ders saati × ETKİN grup sayısı); bkz. `effective_branch_hours`.
    pub branch_hours: i64,
    /// Alanın tüm şeflerinin planlama-bakım-onarım ek ders saatleri toplamı.
    pub chief_planning_hours: i64,
}

impl PoolBreakdown {
    pub fn total(self) -> i64 {
        self.branch_hours + self.chief_planning_hours
    }
}

/// `as_of` gününde geçerli şeflik aralığı olan AKTİF öğretmenlerin
/// planlama-bakım-onarım ek ders saatleri toplamı.
///
/// OÖKY MADDE 88/2-ç ve Norm Kadro Yön. MADDE 6/4'e göre koordinatörlük
/// toplam ders yükü, şeflerin bu saatleri ile Σ (haftalık ders saati × grup
/// sayısı)'nın toplamıdır. Saat değerleri `ChiefType::weekly_hours`'tan gelir
/// (alan şefi 10, atölye/laboratuvar şefi 6); burada sayı tekrarlanmaz.
///
/// Şeflik kaynağı `teacher_load_periods` projeksiyonudur, eski
/// `teachers.chief_type` sütunu DEĞİL: `setTeacherLoad` artık yalnız
/// projeksiyona yazar ve eski sütun bayatlar. Aralık yarı açıktır
/// (`valid_from <= as_of < valid_to`); `valid_to` boşsa aralık açıktır.
/// Tek sorgu olduğu için `list_for_term` gibi doğrudan generic'tir: aynı SQL
/// hem `&SqlitePool` hem transaction'ın `&mut SqliteConnection`'ı ile çalışır,
/// `_in` sarmalayıcıya gerek kalmaz.
pub async fn chief_planning_hours<'e, E>(executor: E, term: &str, as_of: NaiveDate) -> AppResult<i64>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let chief_types: Vec<String> = sqlx::query_scalar(
        "SELECT p.chief_type
         FROM teacher_load_periods p
         JOIN teachers t ON t.id = p.teacher_id
         WHERE p.term = ?1
           AND t.is_active = 1
           AND p.valid_from <= ?2
           AND (p.valid_to IS NULL OR ?2 < p.valid_to)",
    )
    .bind(term)
    .bind(as_of)
    .fetch_all(executor)
    .await?;

    Ok(chief_types
        .iter()
        .map(|raw| teachers::parse_chief_type(raw).weekly_hours())
        .sum())
}

/// Havuzun kırılımı — havuz hesabının TEK noktası. Havuzu gösteren, dağıtan
/// ya da aşım uyarısı veren her yer buradan geçer; böylece şeflik saati
/// kuralı bir yerde unutulamaz.
pub async fn pool_breakdown(pool: &SqlitePool, term: &str, as_of: NaiveDate) -> AppResult<PoolBreakdown> {
    Ok(PoolBreakdown {
        branch_hours: effective_branch_hours(pool, term).await?,
        chief_planning_hours: chief_planning_hours(pool, term, as_of).await?,
    })
}

/// `pool_breakdown`'ın transaction-içi karşılığı (bkz. `effective_branch_hours_in`
/// açıklaması): tarihçe kapısı (`db/history_context.rs::load`) havuzu BURADAN
/// okur, kendi kopyasını hesaplamaz.
pub async fn pool_breakdown_in(conn: &mut SqliteConnection, term: &str, as_of: NaiveDate) -> AppResult<PoolBreakdown> {
    Ok(PoolBreakdown {
        branch_hours: effective_branch_hours_in(&mut *conn, term).await?,
        chief_planning_hours: chief_planning_hours(&mut *conn, term, as_of).await?,
    })
}

/// Tam havuz: şeflik saatleri + Σ (haftalık ders saati × etkin grup sayısı).
/// İşletme takdiri, otomatik dağıtım ve aşım uyarısı bu değer üzerinden
/// çalışır; şeflik saati havuzdan çıkarılmaz.
pub async fn total_pool_hours(pool: &SqlitePool, term: &str, as_of: NaiveDate) -> AppResult<i64> {
    Ok(pool_breakdown(pool, term, as_of).await?.total())
}

/// `total_pool_hours`'ın transaction-içi karşılığı.
pub async fn total_pool_hours_in(conn: &mut SqliteConnection, term: &str, as_of: NaiveDate) -> AppResult<i64> {
    Ok(pool_breakdown_in(conn, term, as_of).await?.total())
}

/// Havuzun hesaplandığı gün: bugün (Europe/Istanbul), dönem aralığına
/// sıkıştırılmış. Dönem başlamadıysa dönem başı, bittiyse dönem sonu
/// geçerlidir; şeflik aralıkları dönem içinde tanımlı olduğu için dönem
/// dışı bir gün asla sorgulanmaz. Dönem kaydı yoksa hata döner (havuzu
/// sessizce 0 saymak yerine kullanıcıya dönemi seçmesi söylenir).
pub async fn current_as_of(pool: &SqlitePool, term: &str) -> AppResult<NaiveDate> {
    let mut conn = pool.acquire().await?;
    let dates = terms::get_in(&mut conn, term).await?;
    Ok(dates.default_as_of(today_local()))
}

fn validate(input: &TermBranchHoursInput) -> AppResult<()> {
    if input.grade.trim().is_empty() {
        return Err(AppError::Validation("Sınıf boş olamaz".into()));
    }
    if input.branch.trim().is_empty() {
        return Err(AppError::Validation("Dal adı boş olamaz".into()));
    }
    if input.weekly_hours <= 0 {
        return Err(AppError::Validation(
            "Haftalık ders saati sıfırdan büyük olmalı".into(),
        ));
    }
    // Otomatik satırda istemcinin grup sayısı zaten yok sayılır; 0 öğrenci de
    // geçerlidir (öğrencisiz şube havuza 0 katar). Yalnız elle girilen değer
    // doğrulanır.
    if input.is_group_manual && input.group_count <= 0 {
        return Err(AppError::Validation("Grup sayısı en az 1 olmalı".into()));
    }
    Ok(())
}

/// Satıra yazılacak `group_count`. Elle satırda istemcinin değeridir.
/// Otomatik satırda istemcinin değeri YOK SAYILIR: sunucu hesaplananı
/// önbellek olarak yazar. Öğrencisiz satırda önbellek en az 1'dir (satır
/// elle moda çevrilirse başlangıç değeri geçerli olsun); ETKİN değer yine
/// okuma anında hesaplanır ve 0 olabilir.
fn stored_group_count(row: &TermBranchHoursInput, counts: &BranchStudentCounts) -> i64 {
    if row.is_group_manual {
        return row.group_count;
    }
    auto_group_count(counts, &row.grade, &row.branch).max(1)
}

/// Bir dönemin ders yükü satırlarını TAMAMEN değiştirir.
///
/// Ekran her kaydetmede dönemin tüm satırlarını gönderir; `availability.rs`
/// `replace_for_teacher` ile aynı düzeni izler: kısmi güncelleme yoktur,
/// arayüzle veritabanının ayrışması imkânsız kılınır. İşlem atomiktir: yeni
/// liste yazılamazsa eski liste de silinmez.
pub async fn replace_for_term(
    pool: &SqlitePool,
    term: &str,
    rows: &[TermBranchHoursInput],
) -> AppResult<()> {
    for row in rows {
        validate(row)?;
    }

    let mut seen = std::collections::BTreeSet::new();
    for row in rows {
        let key = (row.grade.trim().to_lowercase(), row.branch.trim().to_lowercase());
        if !seen.insert(key) {
            return Err(AppError::Validation(format!(
                "\"{}\" sınıfı için \"{}\" dalı birden fazla satırda geçiyor",
                row.grade, row.branch
            )));
        }
    }

    let counts = branch_student_counts(pool, term).await?;

    let mut tx = pool.begin().await?;
    let now = now_iso();

    sqlx::query("DELETE FROM term_branch_hours WHERE term = ?1")
        .bind(term)
        .execute(&mut *tx)
        .await?;

    for row in rows {
        sqlx::query(
            "INSERT INTO term_branch_hours
                (term, grade, branch, weekly_hours, group_count, is_group_manual,
                 created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        )
        .bind(term)
        .bind(&row.grade)
        .bind(&row.branch)
        .bind(row.weekly_hours)
        .bind(stored_group_count(row, &counts))
        .bind(row.is_group_manual)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Bir dönemin ders yükü satırlarını başka bir döneme kopyalar.
/// Yeni eğitim-öğretim yılına başlarken geçen yılın dal/grup ayarını sıfırdan
/// girmeyi önler. Hedef dönemde zaten satır varsa hiçbir şey yapılmaz ve
/// `false` döner — `availability.rs::copy_term` ile aynı kural.
pub async fn copy_term(pool: &SqlitePool, from_term: &str, to_term: &str) -> AppResult<bool> {
    let existing: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM term_branch_hours WHERE term = ?1")
            .bind(to_term)
            .fetch_one(pool)
            .await?;

    if existing > 0 {
        return Ok(false);
    }

    let now = now_iso();
    sqlx::query(
        "INSERT INTO term_branch_hours
            (term, grade, branch, weekly_hours, group_count, is_group_manual,
             created_at, updated_at)
         SELECT ?1, grade, branch, weekly_hours, group_count, is_group_manual, ?2, ?2
         FROM term_branch_hours WHERE term = ?3",
    )
    .bind(to_term)
    .bind(&now)
    .bind(from_term)
    .execute(pool)
    .await?;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_pool, students, teachers};
    use crate::db::teaching_load_test_support::{seed_teacher, ymd};
    use crate::domain::models::{ChiefType, NewStudent, NewTeacher};
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    pub(super) const TERM: &str = "2026-2027/1";

    pub(super) async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    pub(super) fn row(grade: &str, branch: &str, weekly: i64, groups: i64) -> TermBranchHoursInput {
        TermBranchHoursInput {
            grade: grade.into(),
            branch: branch.into(),
            weekly_hours: weekly,
            group_count: groups,
            is_group_manual: true,
        }
    }

    #[tokio::test]
    async fn replace_writes_the_whole_term() {
        let (_dir, pool) = test_pool().await;

        replace_for_term(
            &pool,
            TERM,
            &[row("12/C", "Elektronik Haberleşme", 24, 2)],
        )
        .await
        .unwrap();

        let rows = list_for_term(&pool, TERM).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].weekly_hours, 24);
        assert_eq!(rows[0].group_count, 2);
    }

    /// İkinci kayıt öncekinin tamamen yerine geçer; birikmez.
    #[tokio::test]
    async fn replace_overwrites_previous_rows() {
        let (_dir, pool) = test_pool().await;

        replace_for_term(&pool, TERM, &[row("12/C", "Dal A", 24, 2)]).await.unwrap();
        replace_for_term(&pool, TERM, &[row("12/D", "Dal B", 20, 1)]).await.unwrap();

        let rows = list_for_term(&pool, TERM).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].grade, "12/D");
    }

    #[tokio::test]
    async fn replace_with_empty_list_clears_term() {
        let (_dir, pool) = test_pool().await;

        replace_for_term(&pool, TERM, &[row("12/C", "Dal A", 24, 2)]).await.unwrap();
        replace_for_term(&pool, TERM, &[]).await.unwrap();

        assert!(list_for_term(&pool, TERM).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn rows_are_scoped_to_term() {
        let (_dir, pool) = test_pool().await;

        replace_for_term(&pool, TERM, &[row("12/C", "Dal A", 24, 2)]).await.unwrap();
        replace_for_term(&pool, "2027-2028/1", &[row("12/D", "Dal B", 20, 1)])
            .await
            .unwrap();

        assert_eq!(list_for_term(&pool, TERM).await.unwrap().len(), 1);
        assert_eq!(list_for_term(&pool, "2027-2028/1").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn zero_or_negative_weekly_hours_is_rejected() {
        let (_dir, pool) = test_pool().await;
        let err = replace_for_term(&pool, TERM, &[row("12/C", "Dal A", 0, 2)])
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn zero_group_count_is_rejected() {
        let (_dir, pool) = test_pool().await;
        let err = replace_for_term(&pool, TERM, &[row("12/C", "Dal A", 24, 0)])
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    /// Aynı sınıf+dal aynı batch içinde iki kez gönderilirse UNIQUE kısıtına
    /// çarpıp anlaşılmaz bir veritabanı hatası vermek yerine açık bir
    /// doğrulama hatası dönmeli.
    #[tokio::test]
    async fn duplicate_grade_branch_in_same_batch_is_rejected() {
        let (_dir, pool) = test_pool().await;
        let err = replace_for_term(
            &pool,
            TERM,
            &[row("12/C", "Dal A", 24, 2), row("12/C", "Dal A", 20, 1)],
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
        assert!(list_for_term(&pool, TERM).await.unwrap().is_empty(), "hiçbiri yazılmamalı");
    }

    #[tokio::test]
    async fn copy_term_duplicates_rows_into_empty_term() {
        let (_dir, pool) = test_pool().await;
        replace_for_term(&pool, TERM, &[row("12/C", "Dal A", 24, 2)]).await.unwrap();

        assert!(copy_term(&pool, TERM, "2027-2028/1").await.unwrap());
        let copied = list_for_term(&pool, "2027-2028/1").await.unwrap();
        assert_eq!(copied.len(), 1);
        assert_eq!(copied[0].weekly_hours, 24);
        assert_eq!(copied[0].group_count, 2);
    }

    /// Hedef dönemde veri varsa kopyalama üzerine yazmaz.
    #[tokio::test]
    async fn copy_term_refuses_when_target_already_has_data() {
        let (_dir, pool) = test_pool().await;
        replace_for_term(&pool, TERM, &[row("12/C", "Dal A", 24, 2)]).await.unwrap();
        replace_for_term(&pool, "2027-2028/1", &[row("12/D", "Dal B", 10, 1)])
            .await
            .unwrap();

        assert!(!copy_term(&pool, TERM, "2027-2028/1").await.unwrap());
        let target = list_for_term(&pool, "2027-2028/1").await.unwrap();
        assert_eq!(target.len(), 1);
        assert_eq!(target[0].grade, "12/D", "mevcut kayıt korunmalı");
    }

    pub(super) async fn a_student(pool: &SqlitePool, grade: &str, branch: &str, term: &str) {
        students::create(
            pool,
            &NewStudent {
                first_name: "Test".into(),
                last_name: "Ogrenci".into(),
                student_no: None,
                grade: grade.into(),
                branch: branch.into(),
                company_id: None,
                submitted_at: None,
                term: term.into(),
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn distinct_branches_from_students_ignores_other_terms() {
        let (_dir, pool) = test_pool().await;
        a_student(&pool, "12/C", "Elektronik Haberleşme", TERM).await;
        a_student(&pool, "12/C", "Elektronik Haberleşme", TERM).await;
        a_student(&pool, "12/D", "Endüstriyel Bakım Onarım", TERM).await;
        a_student(&pool, "12/E", "Diğer Dal", "2027-2028/1").await;

        let pairs = distinct_branches_from_students(&pool, TERM).await.unwrap();
        assert_eq!(
            pairs,
            vec![
                ("12/C".to_string(), "Elektronik Haberleşme".to_string()),
                ("12/D".to_string(), "Endüstriyel Bakım Onarım".to_string()),
            ]
        );
    }

    // --- Şeflik saatleri (MADDE 6/4, OÖKY MADDE 88/2-ç) ---
    // Testler `pool_tests.rs`'te durur (dosya 800 satırı aşıyordu); bu
    // yardımcı, hem oradan hem `auto_group_tests.rs`'ten paylaşılır.

    pub(super) fn breakdown(branch_hours: i64, chief_planning_hours: i64) -> PoolBreakdown {
        PoolBreakdown { branch_hours, chief_planning_hours }
    }

    /// Migration 0005, önceki dört göçün oluşturduğu eski iki-JSON ayarını
    /// kalıcı veri kaybetmeden `term_branch_hours`'a taşımalı: eşleşen
    /// anahtar satıra dönüşür, eşleşmeyen anahtar (eski `filter_map`
    /// davranışıyla aynı biçimde) atlanır, ve eski ayar anahtarları silinir.
    #[tokio::test]
    async fn migration_moves_legacy_branch_settings_into_term_branch_hours() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("legacy.db");

        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();

        // 0005 öncesi durumu kur: yalnızca ilk dört göçü uygula.
        for file in [
            "0001_initial.sql",
            "0002_seed_hour_rules.sql",
            "0003_students_term.sql",
            "0004_company_term_hours.sql",
        ] {
            let sql = std::fs::read_to_string(format!("migrations/{file}")).unwrap();
            sqlx::raw_sql(&sql).execute(&pool).await.unwrap();
        }

        // Eski iki-JSON ayarını, biri eşleşen biri eşleşmeyen anahtarla kur.
        sqlx::query("UPDATE settings SET value = ?1 WHERE key = 'branch_weekly_hours'")
            .bind(r#"{"12/C|Elektronik Haberleşme": 24, "12/D|Eslesmeyen": 20}"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE settings SET value = ?1 WHERE key = 'branch_group_counts'")
            .bind(r#"{"12/C|Elektronik Haberleşme": 2}"#)
            .execute(&pool)
            .await
            .unwrap();

        let sql = std::fs::read_to_string("migrations/0005_term_branch_hours.sql").unwrap();
        sqlx::raw_sql(&sql).execute(&pool).await.unwrap();

        let rows = sqlx::query_as::<_, TermBranchHours>(
            "SELECT id, term, grade, branch, weekly_hours, group_count, 0 AS is_group_manual,
                    created_at, updated_at FROM term_branch_hours ORDER BY grade",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(rows.len(), 1, "yalnızca eşleşen anahtar taşınmalı");
        assert_eq!(rows[0].term, "2026-2027/1", "hedef, taşıma anındaki aktif dönem olmalı");
        assert_eq!(rows[0].grade, "12/C");
        assert_eq!(rows[0].branch, "Elektronik Haberleşme");
        assert_eq!(rows[0].weekly_hours, 24);
        assert_eq!(rows[0].group_count, 2);

        let remaining: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM settings WHERE key IN ('branch_weekly_hours', 'branch_group_counts')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(remaining, 0, "eski ayar anahtarları silinmeli");
    }

    // --- Migration 0008: kapıdan hiç geçmemiş öğretmenlerin geriye dönük tohumu ---

    /// Eski `teachers::create` ile (kapıdan/`execute_change`den GEÇMEDEN)
    /// eklenen bir öğretmen — `commands/teacher_commands.rs::create_teacher`
    /// bu düzeltmeden ÖNCE tam olarak böyle yazıyordu.
    fn legacy_teacher(last_name: &str, chief_type: &str) -> NewTeacher {
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

    async fn apply_0008(pool: &SqlitePool) {
        let sql = std::fs::read_to_string("migrations/0008_backfill_teacher_load.sql").unwrap();
        sqlx::raw_sql(&sql).execute(pool).await.unwrap();
    }

    /// Gerçek senaryonun kopyası (brief): 12 öğretmen — 9 atölye/lab şefi,
    /// 1 bölüm şefi, 2 şefsiz — yalnız BİRİ (kapıdan oluşturulan) projeksiyonda.
    /// Göç 0008'den ÖNCE şeflik saatleri 6 (yalnız o bir öğretmen), SONRA 64
    /// (9×6 + 1×10) olmalı.
    #[tokio::test]
    async fn migration_0008_backfills_chief_hours_for_teachers_that_never_went_through_the_gate() {
        let (_dir, pool) = test_pool().await;
        let as_of = ymd(2026, 10, 1);

        // Kapıdan geçen tek öğretmen — 9 atölye/lab şefinden biri.
        seed_teacher(&pool, "Kapidan", ChiefType::WorkshopLab).await;
        assert_eq!(chief_planning_hours(&pool, TERM, as_of).await.unwrap(), 6, "göçten önce yalnız bu öğretmen görünür");

        // Kalan 11 öğretmen eski yoldan (kapıyı hiç görmeden) eklendi.
        for i in 0..8 {
            teachers::create(&pool, &legacy_teacher(&format!("Atolye{i}"), "workshop_lab")).await.unwrap();
        }
        teachers::create(&pool, &legacy_teacher("Bolum", "department")).await.unwrap();
        teachers::create(&pool, &legacy_teacher("Duz1", "none")).await.unwrap();
        teachers::create(&pool, &legacy_teacher("Duz2", "none")).await.unwrap();

        apply_0008(&pool).await;

        // MADDE 6/4: 9 × 6 (atölye/lab şefi) + 1 × 10 (bölüm şefi) = 64.
        assert_eq!(chief_planning_hours(&pool, TERM, as_of).await.unwrap(), 9 * 6 + 10);
    }

    /// Göç 0008 birden fazla kez uygulanırsa (ör. tekrar dağıtım) projeksiyonu
    /// zaten olan öğretmene ikinci bir satır YAZMAMALI — `UNIQUE(teacher_id,
    /// term, valid_from)` kısıtına çarpıp göçü kırmamalı.
    #[tokio::test]
    async fn migration_0008_is_idempotent() {
        let (_dir, pool) = test_pool().await;
        seed_teacher(&pool, "Kapidan", ChiefType::WorkshopLab).await;
        teachers::create(&pool, &legacy_teacher("Eski", "department")).await.unwrap();

        apply_0008(&pool).await;
        let after_first: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM teacher_load_periods")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(after_first, 2, "biri kapıdan, biri göçten: iki satır");

        apply_0008(&pool).await;
        let after_second: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM teacher_load_periods")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(after_second, after_first, "ikinci uygulama yeni satır eklememeli");

        let events_after_second: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM change_events WHERE kind = 'load_set'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(events_after_second, 2, "ikinci uygulama yeni olay da eklememeli");
    }
}

#[cfg(test)]
mod auto_group_tests;

#[cfg(test)]
mod pool_tests;
