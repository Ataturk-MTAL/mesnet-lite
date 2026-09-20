use crate::db::{teachers, terms};
use crate::domain::group_count;
use crate::domain::terms::today_local;
use crate::error::{AppError, AppResult};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
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
pub async fn list_for_term(pool: &SqlitePool, term: &str) -> AppResult<Vec<TermBranchHours>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM term_branch_hours WHERE term = ?1
         ORDER BY grade COLLATE NOCASE, branch COLLATE NOCASE"
    );
    Ok(sqlx::query_as::<_, TermBranchHours>(&sql)
        .bind(term)
        .fetch_all(pool)
        .await?)
}

/// (sınıf, dal) çiftinin öğrenci sayısı. Anahtar, öğrenci kaydındaki metnin
/// aynısıdır (büyük/küçük harf ve boşluk dahil): satırlarla eşleştirme de
/// `merge_with_suggestions`'daki gibi birebir eşitliktir.
pub type BranchStudentCounts = BTreeMap<(String, String), i64>;

/// Öğrenci kayıtlarından türeyen (sınıf, dal, öğrenci sayısı) üçlüleri.
/// Filtre TEK yerde durur: dönem eşleşir, sınıf ve dal boş değildir. Hem
/// öneri satırları hem otomatik grup sayısı buradan beslenir.
pub async fn student_counts_by_branch(
    pool: &SqlitePool,
    term: &str,
) -> AppResult<Vec<(String, String, i64)>> {
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT grade, branch, COUNT(*) FROM students
         WHERE term = ?1 AND grade <> '' AND branch <> ''
         GROUP BY grade, branch
         ORDER BY grade COLLATE NOCASE, branch COLLATE NOCASE",
    )
    .bind(term)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// `student_counts_by_branch` sonucunu arama tablosuna çevirir.
pub async fn branch_student_counts(pool: &SqlitePool, term: &str) -> AppResult<BranchStudentCounts> {
    Ok(student_counts_by_branch(pool, term)
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
    Ok(rows
        .iter()
        .map(|saved| {
            let auto = auto_group_count(&counts, &saved.grade, &saved.branch);
            saved.weekly_hours * effective_group_count(saved, auto)
        })
        .sum())
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
pub async fn chief_planning_hours(pool: &SqlitePool, term: &str, as_of: NaiveDate) -> AppResult<i64> {
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
    .fetch_all(pool)
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

/// Tam havuz: şeflik saatleri + Σ (haftalık ders saati × etkin grup sayısı).
/// İşletme takdiri, otomatik dağıtım ve aşım uyarısı bu değer üzerinden
/// çalışır; şeflik saati havuzdan çıkarılmaz.
pub async fn total_pool_hours(pool: &SqlitePool, term: &str, as_of: NaiveDate) -> AppResult<i64> {
    Ok(pool_breakdown(pool, term, as_of).await?.total())
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
    use crate::db::{init_pool, students};
    use crate::db::teaching_load_test_support::{
        change_chief_type, deactivate_teacher, seed_teacher, ymd,
    };
    use crate::domain::models::{ChiefType, NewStudent};
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

    pub(super) fn breakdown(branch_hours: i64, chief_planning_hours: i64) -> PoolBreakdown {
        PoolBreakdown { branch_hours, chief_planning_hours }
    }

    /// Alan şefi 10 + atölye/lab şefi 6 saat, Σ(saat × grup)'a EKLENİR.
    #[tokio::test]
    async fn pool_adds_department_and_workshop_lab_chief_hours_to_branch_hours() {
        let (_dir, pool) = test_pool().await;
        replace_for_term(
            &pool,
            TERM,
            &[row("12/C", "Dal A", 24, 2), row("12/D", "Dal B", 24, 1)],
        )
        .await
        .unwrap();
        seed_teacher(&pool, "Alan", ChiefType::Department).await;
        seed_teacher(&pool, "Atolye", ChiefType::WorkshopLab).await;
        seed_teacher(&pool, "Siradan", ChiefType::None).await;

        let as_of = ymd(2026, 10, 1);

        assert_eq!(chief_planning_hours(&pool, TERM, as_of).await.unwrap(), 10 + 6);
        assert_eq!(pool_breakdown(&pool, TERM, as_of).await.unwrap(), breakdown(24 * 2 + 24, 16));
        assert_eq!(total_pool_hours(&pool, TERM, as_of).await.unwrap(), 24 * 2 + 24 + 16);
    }

    #[tokio::test]
    async fn pool_is_only_the_branch_sum_when_nobody_is_chief() {
        let (_dir, pool) = test_pool().await;
        replace_for_term(&pool, TERM, &[row("12/C", "Dal A", 24, 2)]).await.unwrap();
        seed_teacher(&pool, "Siradan", ChiefType::None).await;

        let as_of = ymd(2026, 10, 1);
        assert_eq!(chief_planning_hours(&pool, TERM, as_of).await.unwrap(), 0);
        assert_eq!(total_pool_hours(&pool, TERM, as_of).await.unwrap(), 48);
    }

    #[tokio::test]
    async fn inactive_teachers_chief_hours_are_not_counted() {
        let (_dir, pool) = test_pool().await;
        seed_teacher(&pool, "Alan", ChiefType::Department).await;
        let atolye = seed_teacher(&pool, "Atolye", ChiefType::WorkshopLab).await;
        deactivate_teacher(&pool, atolye).await;

        assert_eq!(chief_planning_hours(&pool, TERM, ymd(2026, 10, 1)).await.unwrap(), 10);
    }

    /// `valid_from <= as_of < valid_to`: aralığın başlangıç günü dahil, bitiş
    /// günü hariçtir.
    #[tokio::test]
    async fn chief_hours_outside_the_validity_range_are_not_counted() {
        let (_dir, pool) = test_pool().await;
        let alan = seed_teacher(&pool, "Alan", ChiefType::Department).await; // 2026-09-01'den
        change_chief_type(&pool, alan, ChiefType::None, ymd(2026, 11, 5)).await;

        let hours = |d| chief_planning_hours(&pool, TERM, d);
        assert_eq!(hours(ymd(2026, 8, 31)).await.unwrap(), 0, "aralıktan önce");
        assert_eq!(hours(ymd(2026, 9, 1)).await.unwrap(), 10, "başlangıç günü dahil");
        assert_eq!(hours(ymd(2026, 11, 4)).await.unwrap(), 10, "bitişten önceki gün");
        assert_eq!(hours(ymd(2026, 11, 5)).await.unwrap(), 0, "bitiş günü hariç");
    }

    /// Şef değişince iki aralık ardışık olur; her gün doğru aralığı okumalı.
    #[tokio::test]
    async fn chief_change_gives_the_right_total_on_each_side_of_the_change_day() {
        let (_dir, pool) = test_pool().await;
        let alan = seed_teacher(&pool, "Alan", ChiefType::Department).await;
        change_chief_type(&pool, alan, ChiefType::WorkshopLab, ymd(2026, 11, 5)).await;

        let hours = |d| chief_planning_hours(&pool, TERM, d);
        assert_eq!(hours(ymd(2026, 10, 1)).await.unwrap(), 10);
        assert_eq!(hours(ymd(2026, 11, 5)).await.unwrap(), 6);
        assert_eq!(hours(ymd(2027, 1, 31)).await.unwrap(), 6, "açık aralık (valid_to boş) dönem sonuna dek");
    }

    #[tokio::test]
    async fn chief_hours_are_scoped_to_the_term() {
        let (_dir, pool) = test_pool().await;
        seed_teacher(&pool, "Alan", ChiefType::Department).await;

        assert_eq!(chief_planning_hours(&pool, "2027-2028/1", ymd(2027, 10, 1)).await.unwrap(), 0);
    }

    #[test]
    fn breakdown_total_is_the_sum_of_both_parts() {
        assert_eq!(breakdown(72, 16).total(), 88);
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
}

#[cfg(test)]
mod auto_group_tests;
