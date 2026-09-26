//! `versions` testleri: oluşturma (dosya + satır, otomatik dedup, elle her
//! zaman yeni), liste sırası ve kullanılabilirlik, silme, açmanın orijinal
//! dosyaya dokunmadığı, uçtan uca sürümden eski değerin okunabildiği ve
//! sürüm alınamadığında çıktının hiç dönmediği hata yolu.

use chrono::{NaiveDate, NaiveDateTime};
use sqlx::SqlitePool;

use super::versions::{
    create_version, delete_version, list_versions, open_version, open_version_for_export, record_auto_version, VersionKind,
};
use crate::commands::hours_commands::save_hours_for_term;
use crate::db::company_hours::{self, HoursInput};
use crate::db::read_at::ReadAt;
use crate::db::{companies, init_pool};
use crate::domain::history::decide::{ChangeCommand, ChangeRequest, NewStudentInput};
use crate::domain::models::NewCompany;
use crate::services::change_service::{execute_change, ChangeMode, ChangeOutcome};
use crate::services::pdf_report;

/// Testlerin tümü aynı, seed'de zaten var olan aktif dönemi kullanır —
/// gerçek yoldaki (`save_hours_for_term`/`execute_change`) dönem doğrulaması
/// bilinmeyen bir dönem adına takılmasın diye.
const TERM: &str = "2026-2027/1";

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
    (dir, pool)
}

fn now(second: u32) -> NaiveDateTime {
    chrono::NaiveDate::from_ymd_opt(2026, 9, 26)
        .unwrap()
        .and_hms_opt(10, 0, second)
        .unwrap()
}

async fn insert_company(pool: &SqlitePool, name: &str) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO companies (name, address_text, created_at, updated_at)
         VALUES (?1, 'adres', 't', 't') RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// `services::change_service`in GERÇEK yolundan bir işletme yaratır —
/// `db/legacy_seed_test_support.rs`in donmuş tablolara elle yazan
/// yardımcılarının (bkz. o dosyanın başlığı) AKSİNE, uçtan uca testte
/// (`version_taken_before_a_change_still_reads_the_old_value`) sürümün
/// gerçekten `company_hour_periods` projeksiyonunu yakaladığını kanıtlamak
/// için kullanılır.
async fn create_test_company(pool: &SqlitePool, name: &str) -> i64 {
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
            // Mesafe BİLİNMİYOR: `cap_for` yalnızca öğrenci sayısı > 0
            // olduğu sürece hiç tavan koymaz (bkz. `place_one_student`); bu
            // test tavanı değil otomatik sürümü sınıyor, tavana takılmamalı.
            one_way_distance_km: None,
            district: String::new(),
            notes: String::new(),
        },
    )
    .await
    .unwrap()
    .id
}

/// İşletmeye GERÇEK yoldan (`execute_change`) tek bir öğrenci yerleştirir.
/// `cap_for`, 0 öğrencide mesafeden BAĞIMSIZ `Some(0)` döner (bkz.
/// `domain::history::policy::cap_for`) — öğrencisiz bir işletmeye saat
/// takdir edilemez; bu yardımcı yalnızca o ön koşulu karşılar.
async fn place_one_student(pool: &SqlitePool, company_id: i64, name: &str) {
    let request = ChangeRequest {
        term: TERM.to_string(),
        effective_date: None,
        document_date: None,
        reason: "test".into(),
        command: ChangeCommand::CreateStudent {
            student: NewStudentInput {
                first_name: name.to_string(),
                last_name: "Öğrenci".into(),
                student_no: None,
                grade: "12/C".into(),
                branch: "Elektronik Haberleşme".into(),
                submitted_at: None,
            },
            company_id: Some(company_id),
        },
    };
    let outcome = execute_change(pool, request, ChangeMode::Commit { expected_high_water: None }, ymd(2026, 8, 15))
        .await
        .unwrap();
    assert!(matches!(outcome, ChangeOutcome::Committed { .. }));
}

/// `save_hours_for_term`: panonun eski yazıcılarının kullandığı sarmalayıcı,
/// GERÇEK değişiklik kapısından (`commit_legacy_change` → `execute_change` →
/// `SetCompanyHours`) geçer — `company_hour_periods` projeksiyonuna yazan
/// TEK yol budur (#17'den beri donmuş `company_term_hours`e kimse yazmaz).
async fn set_company_hours_via_real_path(
    pool: &SqlitePool,
    company_id: i64,
    awarded_hours: i64,
    effective_date: Option<&str>,
    today: NaiveDate,
) {
    let row = HoursInput { company_id, max_hours_snapshot: 1000, awarded_hours, is_honorary: false, is_locked: false, notes: String::new() };
    save_hours_for_term(pool, TERM, &[row], effective_date.map(str::to_string), None, today).await.unwrap();
}

// ---------------------------------------------------------------------
// Oluşturma
// ---------------------------------------------------------------------

/// Elle kayıt: dosyayı `versions_dir` altına yazmalı ve `versions` satırını
/// eklemeli.
#[tokio::test]
async fn create_version_writes_a_file_and_a_row() {
    let (dir, pool) = test_pool().await;
    let versions_dir = dir.path().join("versions");
    insert_company(&pool, "Test A.Ş.").await;

    let version = create_version(&pool, &versions_dir, "İlk Kayıt", VersionKind::Manual, None, now(0))
        .await
        .unwrap();

    assert_eq!(version.name, "İlk Kayıt");
    assert!(matches!(version.kind, VersionKind::Manual));
    assert!(version.is_available);

    let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM versions").fetch_one(&pool).await.unwrap();
    assert_eq!(row_count, 1, "versions tablosuna tek satır eklenmeli");
}

/// Otomatik sürüm: veri DEĞİŞMEDEN ikinci çağrı yeni dosya açmamalı, aynı
/// (ilk) sürümü döndürmeli — kullanıcı kararı: ayırt etmeyen kopya birikmesin.
#[tokio::test]
async fn automatic_repeat_without_data_change_does_not_open_a_new_file() {
    let (dir, pool) = test_pool().await;
    let versions_dir = dir.path().join("versions");
    insert_company(&pool, "Değişmeyen İşletme").await;

    let first = create_version(&pool, &versions_dir, "Çıktı", VersionKind::Auto, Some("workbook"), now(0))
        .await
        .unwrap();
    let second = create_version(&pool, &versions_dir, "Çıktı", VersionKind::Auto, Some("workbook"), now(5))
        .await
        .unwrap();

    assert_eq!(first.id, second.id, "veri değişmediyse aynı sürüm dönmeli");

    let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM versions").fetch_one(&pool).await.unwrap();
    assert_eq!(row_count, 1, "ikinci otomatik çağrı yeni satır açmamalı");
}

/// Veri değişince otomatik sürüm YENİ bir dosya açmalı: parmak izi artık
/// farklı.
#[tokio::test]
async fn automatic_version_opens_a_new_file_when_data_changed() {
    let (dir, pool) = test_pool().await;
    let versions_dir = dir.path().join("versions");
    insert_company(&pool, "Birinci").await;

    let first = create_version(&pool, &versions_dir, "Çıktı", VersionKind::Auto, Some("workbook"), now(0))
        .await
        .unwrap();

    insert_company(&pool, "İkinci").await;
    let second = create_version(&pool, &versions_dir, "Çıktı", VersionKind::Auto, Some("workbook"), now(5))
        .await
        .unwrap();

    assert_ne!(first.id, second.id, "veri değiştiyse yeni bir sürüm açılmalı");

    let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM versions").fetch_one(&pool).await.unwrap();
    assert_eq!(row_count, 2);
}

/// Elle kayıt, veri değişmese bile HER ZAMAN yeni bir sürüm açar — otomatik
/// sürümün aksine, kullanıcının bilinçli isteği kısayola uğramaz.
#[tokio::test]
async fn manual_version_always_opens_a_new_file_even_without_data_change() {
    let (dir, pool) = test_pool().await;
    let versions_dir = dir.path().join("versions");
    insert_company(&pool, "Sabit").await;

    let first = create_version(&pool, &versions_dir, "Birinci Elle Kayıt", VersionKind::Manual, None, now(0))
        .await
        .unwrap();
    let second = create_version(&pool, &versions_dir, "İkinci Elle Kayıt", VersionKind::Manual, None, now(5))
        .await
        .unwrap();

    assert_ne!(first.id, second.id);
    let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM versions").fetch_one(&pool).await.unwrap();
    assert_eq!(row_count, 2);
}

// ---------------------------------------------------------------------
// Liste
// ---------------------------------------------------------------------

/// `list_versions` en yeniden eskiye sıralamalı.
#[tokio::test]
async fn list_versions_orders_newest_first() {
    let (dir, pool) = test_pool().await;
    let versions_dir = dir.path().join("versions");

    let first = create_version(&pool, &versions_dir, "Birinci", VersionKind::Manual, None, now(0)).await.unwrap();
    let second = create_version(&pool, &versions_dir, "İkinci", VersionKind::Manual, None, now(5)).await.unwrap();

    let listed = list_versions(&pool, &versions_dir).await.unwrap();
    assert_eq!(listed.iter().map(|v| v.id).collect::<Vec<_>>(), vec![second.id, first.id]);
}

/// Dosyası diskten silinmiş bir sürümün `isAvailable` alanı `false` olmalı.
#[tokio::test]
async fn list_versions_marks_a_version_with_a_deleted_file_as_unavailable() {
    let (dir, pool) = test_pool().await;
    let versions_dir = dir.path().join("versions");

    let version = create_version(&pool, &versions_dir, "Kaybolacak", VersionKind::Manual, None, now(0))
        .await
        .unwrap();

    let file_name: String = sqlx::query_scalar("SELECT file_name FROM versions WHERE id = ?1")
        .bind(version.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    std::fs::remove_file(versions_dir.join(file_name)).unwrap();

    let listed = list_versions(&pool, &versions_dir).await.unwrap();
    assert!(!listed[0].is_available);
}

// ---------------------------------------------------------------------
// Silme
// ---------------------------------------------------------------------

/// `delete_version` hem dosyayı hem satırı silmeli.
#[tokio::test]
async fn delete_version_removes_the_file_and_the_row() {
    let (dir, pool) = test_pool().await;
    let versions_dir = dir.path().join("versions");

    let version = create_version(&pool, &versions_dir, "Silinecek", VersionKind::Manual, None, now(0))
        .await
        .unwrap();
    let file_name: String = sqlx::query_scalar("SELECT file_name FROM versions WHERE id = ?1")
        .bind(version.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let path = versions_dir.join(&file_name);
    assert!(path.exists());

    delete_version(&pool, &versions_dir, version.id).await.unwrap();

    assert!(!path.exists(), "dosya silinmeli");
    let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM versions").fetch_one(&pool).await.unwrap();
    assert_eq!(row_count, 0, "satır silinmeli");
}

// ---------------------------------------------------------------------
// Açma
// ---------------------------------------------------------------------

/// `open_version`, orijinal sürüm dosyasına DOKUNMAZ: içerik ve değiştirme
/// zamanı önce/sonra aynı kalmalı.
#[tokio::test]
async fn open_version_never_touches_the_original_file() {
    let (dir, pool) = test_pool().await;
    let versions_dir = dir.path().join("versions");
    insert_company(&pool, "Test").await;

    let version = create_version(&pool, &versions_dir, "Değişmeyecek", VersionKind::Manual, None, now(0))
        .await
        .unwrap();
    let file_name: String = sqlx::query_scalar("SELECT file_name FROM versions WHERE id = ?1")
        .bind(version.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let path = versions_dir.join(&file_name);

    let before_bytes = std::fs::read(&path).unwrap();
    let before_modified = std::fs::metadata(&path).unwrap().modified().unwrap();

    let (temp_dir, version_pool) = open_version(&versions_dir, &file_name).await.unwrap();
    // Açılan kopyayı değiştirerek orijinalin GERÇEKTEN ayrı bir dosya
    // olduğunu kanıtla: bu yazma orijinale sızarsa aşağıdaki karşılaştırma
    // düşer.
    insert_company(&version_pool, "Yalnız kopyada").await;
    version_pool.close().await;
    drop(temp_dir);

    let after_bytes = std::fs::read(&path).unwrap();
    let after_modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    assert_eq!(before_bytes, after_bytes, "orijinal dosyanın içeriği değişmemeli");
    assert_eq!(before_modified, after_modified, "orijinal dosyanın değiştirme zamanı değişmemeli");
}

// ---------------------------------------------------------------------
// Uçtan uca
// ---------------------------------------------------------------------

/// Bir işletmenin saati GERÇEK değişiklik yolundan (`execute_change`,
/// `SetCompanyHours`) değiştirilmeden ÖNCE otomatik sürüm alınır, sonra saat
/// yine gerçek yoldan değiştirilir: `open_version_for_export`in açtığı
/// kopyadan okunan değer ESKİYİ, güncel veritabanından okunan değer YENİYİ
/// taşımalı — ayrıca o kopyadan `pdf_report::build_assignment_sheet`in
/// hatasız çalıştığını ve `TempDir` düşürülünce geçici dizinin silindiğini
/// kanıtlar (#17'den beri donmuş `company_term_hours`e değil,
/// `company_hour_periods` projeksiyonuna yazılır — bu test o projeksiyonu okur).
#[tokio::test]
async fn version_taken_before_a_change_still_reads_the_old_value() {
    let (dir, pool) = test_pool().await;
    let versions_dir = dir.path().join("versions");
    let company_id = create_test_company(&pool, "Saati Değişecek İşletme").await;
    place_one_student(&pool, company_id, "Öğrenci").await;
    // Planlama evresinde (dönem henüz başlamadan), tarih vermeden: gerçek
    // yol tarihi otomatik dönem başına çözer.
    set_company_hours_via_real_path(&pool, company_id, 4, None, ymd(2026, 8, 15)).await;

    let taken = record_auto_version(&pool, &versions_dir, "workbook", "Çalışma Kitabı çıktısı").await;
    assert!(taken.is_ok(), "sürüm alınabilmeli: {:?}", taken.err());

    // Dönem başladıktan SONRA, ayrı bir yürürlük tarihiyle: gerçek bir
    // "dönem ortasında saat değişti" senaryosu — ilk kayıtla aynı güne
    // denk gelip iki olayı ayırt edilemez kılmasın diye bilerek farklı.
    set_company_hours_via_real_path(&pool, company_id, 8, Some("2026-10-05"), ymd(2026, 10, 5)).await;

    let listed = list_versions(&pool, &versions_dir).await.unwrap();
    let version = listed.first().expect("bir sürüm alınmış olmalı");

    let (temp_dir, version_pool, version_term) =
        open_version_for_export(&pool, &versions_dir, version.id).await.unwrap();
    let temp_path = temp_dir.path().to_path_buf();

    // Dönen dönem, KOPYANIN kendi `active_term` ayarıdır (brief madde 4).
    assert_eq!(version_term, TERM);

    let old_rows = company_hours::list(&version_pool, &version_term, &ReadAt::Latest).await.unwrap();
    let old_row = old_rows.iter().find(|r| r.company_id == company_id).expect("sürümde satır bulunmalı");
    assert_eq!(old_row.awarded_hours, 4, "sürümdeki değer değişiklikten ÖNCEki olmalı");

    // Dışa aktarım yolunun sürüm kopyasında GERÇEKTEN çalıştığını kanıtla:
    // `open_version_for_export`in beş komuta verdiği havuz budur.
    let pdf_result = pdf_report::build_assignment_sheet(&version_pool, &version_term).await;
    assert!(pdf_result.is_ok(), "sürüm kopyasından PDF üretilebilmeli: {:?}", pdf_result.err());

    version_pool.close().await;
    drop(temp_dir);
    assert!(!temp_path.exists(), "havuz kapanıp TempDir düşürülünce geçici dizin silinmeli");

    let new_rows = company_hours::list(&pool, TERM, &ReadAt::Latest).await.unwrap();
    let new_row = new_rows.iter().find(|r| r.company_id == company_id).expect("güncel havuzda satır bulunmalı");
    assert_eq!(new_row.awarded_hours, 8, "güncel veritabanındaki değer değişiklikten SONRAki olmalı");
}

// ---------------------------------------------------------------------
// Hata yolu
// ---------------------------------------------------------------------

/// Sürüm dizini bir DOSYA tarafından işgal edilmişse (oluşturulamıyorsa)
/// sürüm alınamaz — hata döner, hiçbir dosya/satır yazılmaz.
#[tokio::test]
async fn create_version_fails_when_the_versions_directory_cannot_be_created() {
    let (dir, pool) = test_pool().await;
    let blocked_path = dir.path().join("versions-as-a-file");
    std::fs::write(&blocked_path, b"bu bir dizin degil").unwrap();

    let result = create_version(&pool, &blocked_path, "Olmayacak", VersionKind::Manual, None, now(0)).await;

    assert!(result.is_err());
    let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM versions").fetch_one(&pool).await.unwrap();
    assert_eq!(row_count, 0, "başarısız denemede satır kalmamalı");
}

/// `record_auto_version`in ürettiği hata mesajı, çıktının neden
/// döndürülmediğini açıklamalı (brief: "Sürüm kaydedilemediği için çıktı
/// verilmedi").
#[tokio::test]
async fn record_auto_version_reports_the_reason_when_it_fails() {
    let (dir, pool) = test_pool().await;
    let blocked_path = dir.path().join("versions-as-a-file");
    std::fs::write(&blocked_path, b"bu bir dizin degil").unwrap();

    let result = record_auto_version(&pool, &blocked_path, "workbook", "Çalışma Kitabı çıktısı").await;

    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("Sürüm kaydedilemediği için çıktı verilmedi"),
        "gerçek mesaj: {message}"
    );
}

// ---------------------------------------------------------------------
// Doğrulama
// ---------------------------------------------------------------------

#[tokio::test]
async fn create_version_rejects_a_blank_name() {
    let (dir, pool) = test_pool().await;
    let versions_dir = dir.path().join("versions");

    let result = create_version(&pool, &versions_dir, "   ", VersionKind::Manual, None, now(0)).await;

    assert!(matches!(result, Err(crate::error::AppError::Validation(_))));
}

#[tokio::test]
async fn create_version_rejects_a_name_longer_than_eighty_characters() {
    let (dir, pool) = test_pool().await;
    let versions_dir = dir.path().join("versions");
    let too_long = "a".repeat(81);

    let result = create_version(&pool, &versions_dir, &too_long, VersionKind::Manual, None, now(0)).await;

    assert!(matches!(result, Err(crate::error::AppError::Validation(_))));
}
