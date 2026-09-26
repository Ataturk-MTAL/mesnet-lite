use crate::error::{AppError, AppResult};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;

pub mod assignments;
pub mod availability;
pub mod change_log;
pub mod class_days;
pub mod companies;
pub mod company_hours;
pub mod history_context;
pub mod hour_rules;
#[cfg(test)]
pub(crate) mod legacy_seed_test_support;
#[cfg(test)]
mod migration_0009_tests;
pub mod projection;
pub mod settings;
pub mod students;
pub mod teachers;
pub mod teaching_load;
#[cfg(test)]
pub(crate) mod teaching_load_test_support;
pub mod terms;
pub mod users;

/// Veritabanı dosyasının sabit adı. `lib.rs`teki açılış yolu ve
/// `services::backup`teki geri yükleme (eski dosyanın yerine yenisini koyma)
/// AYNI adı kullanır; iki yerde ayrı ayrı yazılmasın diye tek burada tanımlıdır.
pub const DB_FILE_NAME: &str = "mesnet-lite.db";

/// Tauri yönetilen durumu. Komutlar veritabanı havuzuna buradan erişir.
pub struct AppState {
    pub pool: SqlitePool,
}

/// Veritabanı havuzunu kurar, dosya yoksa oluşturur ve migration'ları uygular.
pub async fn init_pool(db_path: &Path) -> AppResult<SqlitePool> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // "Dosya zaten var mıydı" burada, havuz açılmadan ÖNCE sorulur: aşağıdaki
    // `create_if_missing(true)` dosyayı sessizce oluşturabilir ve bu bilgiyi
    // sonradan geri getiremeyiz. İlk kurulumda (dosya yok) otomatik yedek
    // alınmaz — yedeklenecek bir şey henüz yok.
    let db_existed_before_open = db_path.exists();

    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(format!("Havuz açılamadı: {e}")))?;

    // Otomatik günlük yedek, migration'lardan ÖNCE alınır: bozuk bir
    // migration'dan bu yedekle geri dönülebilsin. Yedek BAŞARISIZ olsa bile
    // uygulama açılmaya devam eder (veriye erişimi engellememeli); hata
    // burada yutulmaz, en azından konsola yazılır.
    if db_existed_before_open {
        if let Some(parent) = db_path.parent() {
            let backup_dir = crate::services::backup::auto_backup_dir(parent);
            if let Err(e) =
                crate::services::backup::create_daily_backup_if_missing(&pool, &backup_dir, crate::domain::terms::today_local())
                    .await
            {
                eprintln!("Otomatik yedek alınamadı: {e}");
            }
        }
    }

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(format!("Migration başarısız: {e}")))?;

    backfill_company_districts(&pool).await?;
    backfill_missing_term_rows(&pool).await?;

    // Tek seferlik aktarım: eski Saat Ayarları/Dağıtım panosunun LIVE
    // değerlerini tarihçe projeksiyonlarına taşır (brief teşhisi — pano
    // yazımları tarihçeden hiç geçmiyordu). Göçlerden ve dönem tamamlamadan
    // SONRA, bayraklı ve idempotent çalışır; hata uygulamanın açılmasını
    // engellemez (bkz. `services::legacy_reconcile` başlığı).
    if let Some(parent) = db_path.parent() {
        crate::services::legacy_reconcile::reconcile_legacy_board(&pool, parent, crate::domain::terms::today_local()).await?;
    }

    Ok(pool)
}

/// Açılış tamamlaması: `settings::known_terms`in bildiği ama `terms`
/// tablosunda satırı olmayan her dönem için varsayılan tarihleri yazar.
///
/// 0006'dan SONRA `create_term`/`save_settings`'in `terms::ensure` çağırmadığı
/// eski sürümlerle açılmış kurulu veritabanlarında böyle dönemler kalmış
/// olabilir (bkz. brief teşhisi); bu, o veritabanlarını göç YAZMADAN düzeltir.
/// İDEMPOTENT: `terms::ensure` `INSERT OR IGNORE` kullandığı için var olan
/// bir satıra (özellikle onaylanmış tarihlere) asla dokunmaz.
async fn backfill_missing_term_rows(pool: &SqlitePool) -> AppResult<()> {
    for term in settings::known_terms(pool).await? {
        terms::ensure(pool, &term).await?;
    }
    Ok(())
}

/// Migration 0010'un SQL ile yapamadığı işi tamamlar: `district = ''` olan
/// (henüz ilçesi türetilmemiş) işletmeleri adreslerinden geriye dönük
/// doldurur. SQLite'ta regex yoktur; ayrıştırma yalnızca Rust tarafındaki
/// `domain::address::parse_district`te vardır.
///
/// İDEMPOTENT: yalnızca `district = ''` satırları okunur, bu yüzden bir
/// kez doldurulan satır ikinci çalıştırmada bir daha işlenmez — elle
/// girilmiş boş bir ilçe de (kullanıcı kasıtlı olarak boş bıraktıysa) her
/// açılışta yeniden adresten türetilmeye çalışılır, ki bu istenen davranıştır:
/// adres eşleşmiyorsa `parse_district` yine `None` döner ve satır boş kalır.
async fn backfill_company_districts(pool: &SqlitePool) -> AppResult<()> {
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, address_text FROM companies WHERE district = ''")
            .fetch_all(pool)
            .await
            .map_err(|e| AppError::Database(format!("İlçe geri doldurma okunamadı: {e}")))?;

    for (id, address_text) in rows {
        let Some(district) = crate::domain::address::parse_district(&address_text) else {
            continue;
        };
        sqlx::query("UPDATE companies SET district = ?1 WHERE id = ?2")
            .bind(district)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|e| AppError::Database(format!("İlçe geri doldurma yazılamadı: {e}")))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Havuz kurulumu veritabanı dosyasını oluşturmalı ve migration'ları uygulamalı.
    #[tokio::test]
    async fn init_pool_creates_schema_and_seeds_hour_rules() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");

        let pool = init_pool(&path).await.unwrap();

        // assignment_slots kaldırıldı; bir işletme tek hücreye yerleşir.
        let removed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'assignment_slots'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(removed, 0, "assignment_slots tablosu kalmamalı");

        // Seed migration 16 satır kural yazmalı
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM company_hour_rules")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 16);

        // Tüm tablolar oluşmuş olmalı
        for table in [
            "companies",
            "students",
            "teachers",
            "teacher_availability",
            "class_workplace_days",
            "company_hour_rules",
            // Saat takdiri atamadan ayrıldı (migration 0004):
            // saat burada, yerleşim assignments tablosunda.
            "company_term_hours",
            "assignments",
            "settings",
            "term_branch_hours",
            // 0006: olay günlüğü + beş tarih aralıklı projeksiyon (spec §4.1).
            "terms",
            "change_sets",
            "change_events",
            "student_placements",
            "company_hour_periods",
            "coordination_periods",
            "teacher_load_periods",
            "teacher_schedule_periods",
        ] {
            let found: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(found, 1, "tablo bulunamadı: {table}");
        }
    }

    /// 0-1 km / 1-2 öğrenci hücresi 2 saat vermeli (seed tablosunun ilk hücresi).
    #[tokio::test]
    async fn seed_first_cell_is_two_hours() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();

        let hours: i64 = sqlx::query_scalar(
            "SELECT max_hours FROM company_hour_rules
             WHERE min_distance_km = 0.0 AND min_students = 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(hours, 2);
    }

    /// 5+ km / 6+ öğrenci hücresi 11 saat vermeli (seed tablosunun son hücresi).
    #[tokio::test]
    async fn seed_last_cell_is_eleven_hours() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();

        let hours: i64 = sqlx::query_scalar(
            "SELECT max_hours FROM company_hour_rules
             WHERE min_distance_km = 5.0 AND max_distance_km IS NULL
               AND min_students = 6 AND max_students IS NULL",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(hours, 11);
    }

    /// Migration 0010: `companies.district` yeni satırlarda boş metinle başlar.
    #[tokio::test]
    async fn migration_0010_adds_district_column_with_empty_default() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();

        sqlx::query(
            "INSERT INTO companies (name, address_text, created_at, updated_at)
             VALUES ('Test', 'adres', 'şimdi', 'şimdi')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let district: String = sqlx::query_scalar("SELECT district FROM companies WHERE name = 'Test'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(district, "");
    }

    /// Geri doldurma yalnızca `district = ''` satırlarını işler: adresi
    /// eşleşen satır ilçesini alır, eşleşmeyen boş kalır, elle zaten bir
    /// ilçesi olan satıra HİÇ dokunulmaz (adresi eşleşebilecek olsa bile).
    #[tokio::test]
    async fn backfill_company_districts_fills_only_blank_rows() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();

        // Bu üç satır, migration 0010'dan ÖNCE var olan gerçek verinin
        // taklidi: doğrudan SQL ile eklenir (companies::create'i ATLAR),
        // çünkü create zaten kendi içinde türetiyor — burada asıl test
        // edilen, migration sonrası HAZIR BEKLEYEN eski satırların geri
        // doldurulmasıdır.
        sqlx::query(
            "INSERT INTO companies (name, address_text, district, created_at, updated_at) VALUES
                ('Eşleşen', '33130 Akdeniz/Mersin', '', 't', 't'),
                ('Eşleşmeyen', 'posta kodu yok', '', 't', 't'),
                ('Elle Girilmiş', '33130 Akdeniz/Mersin', 'Toroslar', 't', 't')",
        )
        .execute(&pool)
        .await
        .unwrap();

        backfill_company_districts(&pool).await.unwrap();

        let districts: Vec<(String, String)> =
            sqlx::query_as("SELECT name, district FROM companies ORDER BY name")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            districts,
            vec![
                ("Elle Girilmiş".to_string(), "Toroslar".to_string()),
                ("Eşleşen".to_string(), "Akdeniz".to_string()),
                ("Eşleşmeyen".to_string(), "".to_string()),
            ]
        );

        // İkinci çalıştırma: hiçbir satır DEĞİŞMEMELİ (idempotent).
        backfill_company_districts(&pool).await.unwrap();
        let after_second_run: Vec<(String, String)> =
            sqlx::query_as("SELECT name, district FROM companies ORDER BY name")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(after_second_run, districts);
    }

    /// İlk kurulumda (DB dosyası henüz yok) otomatik yedek ALINMAMALI:
    /// yedeklenecek bir şey henüz yok.
    #[tokio::test]
    async fn init_pool_does_not_backup_on_first_install() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        init_pool(&db_path).await.unwrap();

        let backup_dir = dir.path().join("backups");
        assert!(
            !backup_dir.exists() || std::fs::read_dir(&backup_dir).unwrap().next().is_none(),
            "ilk kurulumda yedek klasörü boş olmalı"
        );
    }

    /// DB dosyası zaten varsa (ikinci açılış), bugünün otomatik yedeği
    /// migration'lardan önce, `backups/` altında alınmış olmalı.
    #[tokio::test]
    async fn init_pool_backs_up_existing_db_before_migrations() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Birinci açılış: dosyayı yaratır, henüz yedek almaz (yukarıdaki test).
        init_pool(&db_path).await.unwrap();
        // İkinci açılış: dosya artık VAR, bu yüzden bugünün yedeği alınmalı.
        init_pool(&db_path).await.unwrap();

        let backup_dir = dir.path().join("backups");
        let today = crate::domain::terms::today_local();
        let expected = backup_dir.join(format!("mesnet-lite-{}.db", today.format("%Y-%m-%d")));
        assert!(expected.exists(), "bugünün otomatik yedeği alınmış olmalı: {expected:?}");
    }

    fn ymd(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// Kök neden testi: `create_term`/`save_settings`'in `terms::ensure`
    /// çağırmadığı eski sürümlerle açılmış bir veritabanında, `students.term`
    /// bilinir ama `terms` tablosunda satırı yoktur (bkz. brief teşhisi).
    /// Açılış tamamlaması bu satırı sessizce doldurmalı.
    #[tokio::test]
    async fn init_pool_backfills_a_terms_row_for_a_known_term_missing_one() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let pool = init_pool(&db_path).await.unwrap();
        sqlx::query(
            "INSERT INTO students (first_name, last_name, grade, branch, company_id, term)
             VALUES ('Test', 'Öğrenci', '12/C', 'Dal', NULL, ?1)",
        )
        .bind("2027-2028/1")
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let reopened = init_pool(&db_path).await.unwrap();
        let mut conn = reopened.acquire().await.unwrap();
        let fetched = terms::get_in(&mut conn, "2027-2028/1").await.unwrap();
        let expected = crate::domain::terms::TermDates::default_for("2027-2028/1");
        assert_eq!(fetched.start, expected.start);
        assert_eq!(fetched.end, expected.end);
        assert!(!fetched.dates_confirmed);
    }

    /// Açılış tamamlaması, kullanıcının onayladığı tarihlere ASLA dokunmaz —
    /// ikinci (ve üçüncü) açılış hiçbir şeyi değiştirmemeli.
    #[tokio::test]
    async fn init_pool_completion_never_touches_a_confirmed_terms_row() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let pool = init_pool(&db_path).await.unwrap();
        // Seed'deki aktif dönem "2026-2027/1"; tarihlerini onayla.
        terms::update_dates(&pool, "2026-2027/1", ymd(2026, 9, 15), ymd(2027, 1, 20), true, ymd(2026, 9, 1))
            .await
            .unwrap();
        pool.close().await;

        let reopened = init_pool(&db_path).await.unwrap();
        let mut conn = reopened.acquire().await.unwrap();
        let fetched = terms::get_in(&mut conn, "2026-2027/1").await.unwrap();
        assert_eq!(fetched.start, ymd(2026, 9, 15));
        assert_eq!(fetched.end, ymd(2027, 1, 20));
        assert!(fetched.dates_confirmed);
    }
}
