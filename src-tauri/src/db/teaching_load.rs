use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Bir dönemdeki sınıf+dal için haftalık ders saati ve grup sayısı.
///
/// Okulun ders yükü havuzu (MADDE 15/2) bu satırların toplamıdır:
/// Σ (haftalık ders saati × grup sayısı). Saat ile grup sayısı TEK satırda
/// durur; eski iki-JSON tasarımındaki anahtar eşleşmezliği (bkz. migration
/// 0005) burada yapısal olarak imkânsızdır.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TermBranchHours {
    pub id: i64,
    pub term: String,
    pub grade: String,
    pub branch: String,
    pub weekly_hours: i64,
    pub group_count: i64,
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
    pub group_count: i64,
}

const SELECT_COLUMNS: &str =
    "id, term, grade, branch, weekly_hours, group_count, created_at, updated_at";

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

/// Öğrenci kayıtlarından türeyen, o dönemde en az bir öğrencisi olan tüm
/// (sınıf, dal) çiftleri. Ders yükü ekranı henüz satırı olmayan çiftler için
/// öneri satırı göstermek üzere bunu kullanır — kullanıcı boru işaretli
/// bileşik anahtarı elle yazmak zorunda kalmamalı.
pub async fn distinct_branches_from_students(
    pool: &SqlitePool,
    term: &str,
) -> AppResult<Vec<(String, String)>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT grade, branch FROM students
         WHERE term = ?1 AND grade <> '' AND branch <> ''
         ORDER BY grade COLLATE NOCASE, branch COLLATE NOCASE",
    )
    .bind(term)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Dönemin ders yükü havuzu: Σ (haftalık ders saati × grup sayısı).
/// Satır yoksa 0 döner; bu "sınırsız" değil "henüz tanımlanmamış" demektir.
pub async fn pool_hours_for_term(pool: &SqlitePool, term: &str) -> AppResult<i64> {
    let total: Option<i64> = sqlx::query_scalar(
        "SELECT SUM(weekly_hours * group_count) FROM term_branch_hours WHERE term = ?1",
    )
    .bind(term)
    .fetch_one(pool)
    .await?;
    Ok(total.unwrap_or(0))
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
    if input.group_count <= 0 {
        return Err(AppError::Validation("Grup sayısı en az 1 olmalı".into()));
    }
    Ok(())
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

    let mut tx = pool.begin().await?;
    let now = now_iso();

    sqlx::query("DELETE FROM term_branch_hours WHERE term = ?1")
        .bind(term)
        .execute(&mut *tx)
        .await?;

    for row in rows {
        sqlx::query(
            "INSERT INTO term_branch_hours
                (term, grade, branch, weekly_hours, group_count, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        )
        .bind(term)
        .bind(&row.grade)
        .bind(&row.branch)
        .bind(row.weekly_hours)
        .bind(row.group_count)
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
            (term, grade, branch, weekly_hours, group_count, created_at, updated_at)
         SELECT ?1, grade, branch, weekly_hours, group_count, ?2, ?2
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
    use crate::domain::models::NewStudent;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    const TERM: &str = "2026-2027/1";

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    fn row(grade: &str, branch: &str, weekly: i64, groups: i64) -> TermBranchHoursInput {
        TermBranchHoursInput {
            grade: grade.into(),
            branch: branch.into(),
            weekly_hours: weekly,
            group_count: groups,
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
    async fn pool_hours_is_the_sum_of_weekly_hours_times_group_counts() {
        let (_dir, pool) = test_pool().await;

        replace_for_term(
            &pool,
            TERM,
            &[row("12/C", "Dal A", 24, 2), row("12/D", "Dal B", 24, 1)],
        )
        .await
        .unwrap();

        assert_eq!(pool_hours_for_term(&pool, TERM).await.unwrap(), 24 * 2 + 24);
    }

    #[tokio::test]
    async fn pool_hours_is_zero_when_term_has_no_rows() {
        let (_dir, pool) = test_pool().await;
        assert_eq!(pool_hours_for_term(&pool, TERM).await.unwrap(), 0);
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

    async fn a_student(pool: &SqlitePool, grade: &str, branch: &str, term: &str) {
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

        let rows = sqlx::query_as::<_, TermBranchHours>(&format!(
            "SELECT {SELECT_COLUMNS} FROM term_branch_hours ORDER BY grade"
        ))
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
