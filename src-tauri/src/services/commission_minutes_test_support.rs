//! Komisyon tutanağı testlerinin ortak tohum verisi. Veri katmanı, Excel ve
//! PDF testleri aynı senaryoyu paylaşır; böylece "üç çıktı aynı veriyi
//! gösterir" varsayımı ayrı ayrı uydurma fixture'lara dayanmaz.

use crate::commands::hours_commands::save_hours_for_term;
use crate::db::company_hours::HoursInput;
use crate::db::legacy_seed_test_support::{seed_coordinator, seed_hours as seed_hours_event};
use crate::db::teaching_load_test_support::change_chief_type_in_planning;
use crate::db::{companies, init_pool, teachers};
use crate::domain::history::decide::{ChangeCommand, ChangeRequest, NewStudentInput, NewTeacherProfile};
use crate::domain::history::events::TeacherLoad;
use crate::domain::models::{ChiefType, EmploymentType, NewCompany, NewTeacher};
use crate::services::change_service::{execute_change, ChangeMode, ChangeOutcome};
use chrono::NaiveDate;
use sqlx::SqlitePool;

pub const TERM: &str = "2026-2027/1";

pub async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
    (dir, pool)
}

pub async fn seed_teacher(pool: &SqlitePool, first: &str, last: &str) -> i64 {
    teachers::create(
        pool,
        &NewTeacher {
            first_name: first.into(),
            last_name: last.into(),
            registry_no: String::new(),
            field: "Elektrik-Elektronik Teknolojisi".into(),
            branches: vec![],
            employment_type: "tenured".into(),
            base_hours: 20,
            max_extra_hours: 24,
            other_extra_hours: 0,
            chief_type: "none".into(),
            is_active: true,
        },
    )
    .await
    .unwrap()
    .id
}

/// `distance_km` TEK YÖN mesafedir (`companies.one_way_distance_km`).
pub async fn seed_company(pool: &SqlitePool, name: &str, distance_km: Option<f64>) -> i64 {
    companies::create(
        pool,
        &NewCompany {
            name: name.into(),
            contact_first_name: "Test".into(),
            contact_last_name: "Yetkili".into(),
            phone: "(500) 000-0000".into(),
            email: String::new(),
            address_text: "Test Mahallesi, Test Sokak No:1".into(),
            latitude: None,
            longitude: None,
            one_way_distance_km: distance_km,
            district: String::new(),
            notes: String::new(),
        },
    )
    .await
    .unwrap()
    .id
}

/// GERÇEK yazma yolunu (`execute_change`) kullanır: yerleştirmenin tek
/// doğruluk kaynağı `student_placements` projeksiyonudur (bkz. `domain::models::
/// NewStudent` başındaki yorum) — ham `students::create` artık `company_id`
/// YAZMAZ. `commission_minutes.rs` öğrencileri `student.company_id` ile
/// gruplar (`ordered_companies`ın `has_students` denetimi dahil); bu kapıdan
/// geçmezse öğrenci "atanmamış" görünür ve testler sessizce iddiasını
/// kaybeder.
pub async fn seed_student(pool: &SqlitePool, company_id: Option<i64>, first: &str, last: &str) {
    let req = ChangeRequest {
        term: TERM.into(),
        effective_date: None,
        document_date: None,
        reason: "test".into(),
        command: ChangeCommand::CreateStudent {
            student: NewStudentInput {
                first_name: first.into(),
                last_name: last.into(),
                student_no: None,
                grade: "12/C".into(),
                branch: "Elektronik Haberleşme".into(),
                submitted_at: Some("2026-09-11".into()),
            },
            company_id,
        },
    };
    let outcome = execute_change(
        pool,
        req,
        ChangeMode::Commit {
            expected_high_water: None,
        },
        planning_today(),
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ChangeOutcome::Committed { .. }),
        "Committed beklenirdi: {outcome:?}"
    );
}

pub async fn seed_hours(pool: &SqlitePool, company_id: i64, awarded: i64, is_honorary: bool) {
    seed_hours_event(pool, TERM, company_id, awarded, is_honorary).await;
}

pub async fn seed_assignment(
    pool: &SqlitePool,
    teacher_id: i64,
    company_id: i64,
    day: i64,
    hour: i64,
) {
    seed_coordinator(pool, TERM, company_id, teacher_id, day, hour, false, None).await;
}

/// Her durumu içeren tek senaryo: çok öğrencili işletme, tek öğrencili
/// işletme, atanmamış işletme, fahri işletme, uzaklığı olmayan işletme ve
/// Türkçe sıralamayı sınayan Ç/Ş/İ ile başlayan adlar.
pub async fn seed_full_scenario(pool: &SqlitePool) {
    let teacher_a = seed_teacher(pool, "Ayşe", "Yılmaz").await;
    let teacher_b = seed_teacher(pool, "Mehmet", "Öztürk").await;

    // Çok öğrencili; öğrenciler soyada göre sıralanmalı (Çelik < Demir < Şahin).
    let sirin = seed_company(pool, "Şirin Elektrik Ltd.", Some(8.6)).await;
    seed_student(pool, Some(sirin), "Zeynep", "Şahin").await;
    seed_student(pool, Some(sirin), "Ali", "Demir").await;
    seed_student(pool, Some(sirin), "Burak", "Çelik").await;
    seed_hours(pool, sirin, 8, false).await;
    seed_assignment(pool, teacher_a, sirin, 5, 2).await;

    // Tek öğrenci, ASCII 'A' ile başlar: sırada Şirin'den önce gelmeli.
    let acar = seed_company(pool, "Acar Otomasyon", Some(2.2)).await;
    seed_student(pool, Some(acar), "Emre", "Kaya").await;
    seed_hours(pool, acar, 6, false).await;
    seed_assignment(pool, teacher_b, acar, 2, 3).await;

    // Fahri: ücret yerine "Fahri" basılmalı.
    let ciftci = seed_company(pool, "Çiftçi Pano Sanayi", Some(4.5)).await;
    seed_student(pool, Some(ciftci), "Deniz", "Arı").await;
    seed_student(pool, Some(ciftci), "Ece", "Bulut").await;
    seed_hours(pool, ciftci, 0, true).await;
    seed_assignment(pool, teacher_a, ciftci, 3, 1).await;

    // Uzaklığı bilinmiyor.
    let iyi = seed_company(pool, "İyi Aydınlatma", None).await;
    seed_student(pool, Some(iyi), "Can", "Uçar").await;
    seed_hours(pool, iyi, 4, false).await;
    seed_assignment(pool, teacher_b, iyi, 4, 5).await;

    // Öğrencisi var ama hiç atanmamış ve saati de yok.
    let zeytin = seed_company(pool, "Zeytin Bobinaj", Some(6.25)).await;
    seed_student(pool, Some(zeytin), "Selin", "Ak").await;
}

/// Dönem başlamadan önceki "bugün": tarih verilmeden yapılan açılış dönem
/// başına (2026-09-01) yürürlüğe girer. Okuma tarafı (`teaching_load::
/// current_as_of`) GERÇEK bugünü kullanır; o gün dönem aralığında
/// (2026-09-01 – 2027-01-31) kaldığı sürece bu satır hemen görünür.
fn planning_today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 8, 15).unwrap()
}

/// Şeflik türü artık `teacher_load_periods` projeksiyonundan okunur
/// (`db::teachers::list_with_load_as_of`); bu yüzden eski `teachers::create`
/// (yalnız eski `teachers.chief_type` sütununu doldurur) yerine gerçek yazma
/// yolu (`execute_change`) kullanılır. Böylece imza şeridi testleri, üretimde
/// şefliğin gerçekten nasıl kaydedildiğini görür.
pub async fn seed_teacher_with_chief(
    pool: &SqlitePool,
    first: &str,
    last: &str,
    chief_type: ChiefType,
) -> i64 {
    let teacher = NewTeacherProfile {
        first_name: first.into(),
        last_name: last.into(),
        registry_no: String::new(),
        field: "Elektrik-Elektronik Teknolojisi".into(),
        branches: vec![],
        is_active: true,
    };
    let load = TeacherLoad {
        base_hours: 15,
        max_extra_hours: 24,
        other_extra_hours: 0,
        chief_type,
        employment_type: EmploymentType::Tenured,
    };
    let req = ChangeRequest {
        term: TERM.into(),
        effective_date: None,
        document_date: None,
        reason: "test".into(),
        command: ChangeCommand::CreateTeacher { teacher, load },
    };
    let outcome = execute_change(
        pool,
        req,
        ChangeMode::Commit {
            expected_high_water: None,
        },
        planning_today(),
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ChangeOutcome::Committed { .. }),
        "Committed beklenirdi: {outcome:?}"
    );
    sqlx::query_scalar("SELECT id FROM teachers WHERE first_name = ?1 AND last_name = ?2")
        .bind(first)
        .bind(last)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Öğretmeni pasif işaretler; imza şeridi yalnız aktif öğretmenleri listeler.
pub async fn deactivate_teacher(pool: &SqlitePool, teacher_id: i64) {
    sqlx::query("UPDATE teachers SET is_active = 0 WHERE id = ?1")
        .bind(teacher_id)
        .execute(pool)
        .await
        .unwrap();
}

/// Çok sayfalık uzun tablo: 3 öğretmen, 40 işletme (her biri 3 öğrenci), her
/// yedinci işletme atanmamış. Öğretmen aralıklarının sayfa arasında bölünmesini
/// ve atanmamış satırların E hücresini birlikte sınar.
pub async fn seed_long_table(pool: &SqlitePool) {
    let mut teachers = Vec::new();
    for last in ["Çakır", "Öztürk", "Yılmaz"] {
        teachers.push(seed_teacher(pool, "Test", last).await);
    }
    for i in 0..40usize {
        let company = seed_company(pool, &format!("İşletme {i:02}"), Some(i as f64)).await;
        for s in 0..3 {
            seed_student(pool, Some(company), &format!("Ad{s}"), &format!("Soyad{i}")).await;
        }
        if i % 7 == 0 {
            continue;
        }
        // 1 saatlik blok: aynı öğretmenin ardışık hücreleri çakışmasın.
        seed_hours(pool, company, 1, false).await;
        let teacher = teachers[i % teachers.len()];
        seed_assignment(
            pool,
            teacher,
            company,
            1 + (i % 5) as i64,
            1 + (i / 5) as i64,
        )
        .await;
    }
}

/// GERÇEK yazma yoluyla (`execute_change` → `SetCompanyHours`) bir işletmenin
/// saatini `effective_date`ten itibaren değiştirir. `seed_hours`in (tepedeki
/// `legacy_seed_test_support` sarmalayıcısı) AKSİNE bu, `company_hour_periods`te
/// GERÇEKTEN iki ayrı satır açar — `AsOf`/`Latest` farkını sınamak, aynı
/// işletmenin farklı tarihlerde farklı bir açık/kapalı aralığa sahip olmasını
/// gerektirir; tek olaylık bir sahne iki kipte de aynı görünür.
pub async fn set_hours_via_real_path(
    pool: &SqlitePool,
    company_id: i64,
    awarded_hours: i64,
    effective_date: Option<&str>,
    today: NaiveDate,
) {
    let row = HoursInput {
        company_id,
        max_hours_snapshot: 1000,
        awarded_hours,
        is_honorary: false,
        is_locked: false,
        notes: String::new(),
    };
    save_hours_for_term(pool, TERM, &[row], effective_date.map(str::to_string), None, today)
        .await
        .unwrap();
}

/// Bir öğretmene atanmış, öğrencili bir işletme kurar; işletmenin saati dönem
/// başında 4, 2026-10-05'ten itibaren 8'e çıkar. Dönen `(company_id, tarih)`
/// çiftindeki tarih, ikinci değişiklikten ÖNCEki bir gündür: `AsOf` bu günde
/// 4, `Latest` (açık satır) 8 görmeli. Üç çıktının da (PDF/Excel/veri
/// katmanı) `read_at` parametresini gerçekten kullandığını sınamak için ortak.
pub async fn seed_two_period_hours_scenario(pool: &SqlitePool) -> (i64, NaiveDate) {
    let teacher_id = seed_teacher(pool, "Test", "Vance").await;
    let company_id = seed_company(pool, "Tarihli İşletme", Some(6.8)).await;
    seed_student(pool, Some(company_id), "Ada", "Quill").await;
    seed_assignment(pool, teacher_id, company_id, 2, 3).await;
    set_hours_via_real_path(pool, company_id, 4, None, planning_today()).await;
    set_hours_via_real_path(
        pool,
        company_id,
        8,
        Some("2026-10-05"),
        NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(),
    )
    .await;
    (company_id, NaiveDate::from_ymd_opt(2026, 9, 15).unwrap())
}

/// Bir bölüm şefinin türünü `date` gününden itibaren `None`'a çevirir; önceki
/// aralık o gün kapanır (`teacher_load_periods` projeksiyonunda). İmza şeridi
/// `AsOf`/`Latest` farkını sınamak için kullanılır: `AsOf(date'ten önce)` şefi
/// hâlâ görür, `Latest` (gerçek bugün `date`ten sonra olduğu sürece, bkz.
/// `db::teaching_load_test_support` başlığı) görmez.
pub async fn end_chief_role_from(pool: &SqlitePool, teacher_id: i64, date: NaiveDate) {
    change_chief_type_in_planning(pool, teacher_id, ChiefType::None, date).await;
}
