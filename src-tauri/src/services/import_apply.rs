use crate::db::{companies, settings, students};
use crate::domain::history::decide::{ChangeCommand, ChangeRequest, NewStudentInput};
use crate::domain::models::{NewCompany, NewStudent};
use crate::domain::terms::today_local;
use crate::error::{AppError, AppResult};
use crate::services::change_service::{self, ChangeMode, ChangeOutcome};
use crate::services::csv_import::{parse_jotform_csv, ImportRow};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::BTreeMap;

/// `change_sets.reason` sütununa yazılan sabit gerekçe metni; kullanıcı bu
/// içe aktarma için ayrı bir gerekçe girmez (JotForm formu zaten "neden"i
/// taşır).
const IMPORT_REASON: &str = "CSV içe aktarımı (JotForm)";

/// Mevcut kayıtla çakışan işletme için ne yapılacağı.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DuplicatePolicy {
    /// Mevcut işletmeyi kullan, öğrencileri ona bağla. Varsayılan.
    #[default]
    Merge,
    /// Mevcut işletmenin alanlarını CSV'deki değerlerle güncelle, öğrencileri ona bağla.
    Update,
    /// Dosyadaki bu işletmeyi ve öğrencilerini hiç içe aktarma.
    Skip,
}

/// Dosyadaki bir işletme ve ona bağlı öğrenciler.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewGroup {
    /// Normalize edilmiş ad; kullanıcı seçimleri bu anahtarla eşlenir.
    pub key: String,
    pub company_name: String,
    pub address_text: String,
    pub one_way_distance_km: Option<f64>,
    pub round_trip_distance_km: Option<f64>,
    pub student_names: Vec<String>,
    pub student_count: usize,
    /// Aynı ada sahip mevcut kaydın id'si; yoksa None.
    pub existing_company_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub groups: Vec<PreviewGroup>,
    pub total_students: usize,
    pub duplicate_count: usize,
    /// Ayrıştırılamayan satırlar. Sessizce atılmaz.
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub companies_created: usize,
    pub companies_matched: usize,
    pub companies_updated: usize,
    pub companies_skipped: usize,
    pub students_created: usize,
    pub students_skipped: usize,
    pub errors: Vec<String>,
}

/// Dosya içindeki satırları işletme adına göre gruplar.
/// Aynı işletmeye giden birden çok öğrenci tek gruba iner — bu veri setinde
/// 32 öğrenci 28 işletmeye karşılık gelir.
fn group_rows(rows: Vec<ImportRow>) -> BTreeMap<String, (NewCompany, Vec<NewStudent>)> {
    let mut grouped: BTreeMap<String, (NewCompany, Vec<NewStudent>)> = BTreeMap::new();

    for row in rows {
        let key = companies::normalize_name(&row.company.name);
        grouped
            .entry(key)
            .and_modify(|(company, students)| {
                // Aynı işletmenin ikinci satırında mesafe boşsa doldur.
                if company.one_way_distance_km.is_none() {
                    company.one_way_distance_km = row.company.one_way_distance_km;
                }
                students.push(row.student.clone());
            })
            .or_insert_with(|| (row.company.clone(), vec![row.student.clone()]));
    }

    grouped
}

/// CSV içeriğini ayrıştırıp içe aktarma önizlemesi üretir. Hiçbir şey yazılmaz.
pub async fn preview(pool: &SqlitePool, content: &str) -> AppResult<ImportPreview> {
    let parsed = parse_jotform_csv(content)?;
    let grouped = group_rows(parsed.rows);

    let mut preview = ImportPreview {
        errors: parsed.errors,
        ..Default::default()
    };

    for (key, (company, group_students)) in grouped {
        let existing = companies::find_by_normalized_name(pool, &company.name).await?;
        if existing.is_some() {
            preview.duplicate_count += 1;
        }
        preview.total_students += group_students.len();

        preview.groups.push(PreviewGroup {
            key,
            company_name: company.name.clone(),
            address_text: company.address_text.clone(),
            one_way_distance_km: company.one_way_distance_km,
            round_trip_distance_km: company.one_way_distance_km.map(|km| km * 2.0),
            student_names: group_students
                .iter()
                .map(|s| format!("{} {}", s.first_name, s.last_name))
                .collect(),
            student_count: group_students.len(),
            existing_company_id: existing.map(|c| c.id),
        });
    }

    Ok(preview)
}

/// `execute_in`in `Committed` dışındaki sonuçlarını `AppError::Validation`a
/// çevirir; hata mesajına HANGİ öğrencinin reddedildiği eklenir (brief madde
/// 3: "hangi öğrencinin neden reddedildiği kullanıcıya Türkçe ulaşsın").
/// `Err` dönüşü transaction'ı düşürür, sqlx TÜM içe aktarmayı geri alır.
fn commit_outcome_to_result(student: &NewStudent, outcome: ChangeOutcome) -> AppResult<()> {
    match outcome {
        ChangeOutcome::Committed { .. } => Ok(()),
        ChangeOutcome::Rejected { reason, .. } => Err(AppError::Validation(format!(
            "{} {}: {reason}",
            student.first_name, student.last_name
        ))),
        ChangeOutcome::Stale { message } => Err(AppError::Validation(message)),
        ChangeOutcome::Preview { .. } => {
            unreachable!("ChangeMode::Commit ile çağrıldığında Preview dönmez")
        }
    }
}

/// Önizlemede onaylanan içe aktarmayı TEK bir transaction'da uygular.
/// `policies` yalnızca mevcut kayıtla çakışan gruplar için anlamlıdır; anahtarı
/// `PreviewGroup::key` değeridir. Belirtilmeyen çakışmalar `Merge` sayılır.
///
/// Öğrenci yerleştirmesi KAPIDAN geçer (teşhis: `students::create` ham
/// yazımı `student_placements` projeksiyonuna hiç dokunmaz — içe aktarılan
/// her öğrenci "atanmamış" görünürdü). Bir öğrenci kapıdan reddedilirse
/// (`Rejected`) TÜM içe aktarma geri alınır: transaction commit edilmeden
/// düşer, önceki gruplarda zaten oluşturulmuş işletmeler dahil hiçbir şey
/// yazılmaz (`services::student_list_apply`teki tek-transaction desenin
/// AYNISI).
pub async fn apply(
    pool: &SqlitePool,
    content: &str,
    policies: &BTreeMap<String, DuplicatePolicy>,
) -> AppResult<ImportSummary> {
    let today = today_local();
    // İçe aktarılan öğrenciler aktif eğitim-öğretim yılına damgalanır.
    let term = settings::get_active_term(pool).await?;
    let parsed = parse_jotform_csv(content)?;
    let grouped = group_rows(parsed.rows);

    let mut summary = ImportSummary {
        errors: parsed.errors,
        ..Default::default()
    };

    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

    for (key, (company, group_students)) in grouped {
        let existing = companies::find_by_normalized_name_in(&mut tx, &company.name).await?;
        let policy = policies.get(&key).copied().unwrap_or_default();

        let company_id = match (&existing, policy) {
            (Some(_), DuplicatePolicy::Skip) => {
                summary.companies_skipped += 1;
                summary.students_skipped += group_students.len();
                continue;
            }
            (Some(found), DuplicatePolicy::Update) => {
                companies::update_in(&mut tx, found.id, &company).await?;
                summary.companies_updated += 1;
                found.id
            }
            (Some(found), DuplicatePolicy::Merge) => {
                summary.companies_matched += 1;
                found.id
            }
            (None, _) => {
                let created = companies::create_in(&mut tx, &company).await?;
                summary.companies_created += 1;
                created.id
            }
        };

        for student in group_students {
            // Aynı öğrenci iki kez içe aktarılmaz. Kimlik önce öğrenci
            // numarasından, yoksa ad + soyad + sınıf + dal dörtlüsünden gelir.
            // Arama aktif dönem içinde yapılır.
            let mut candidate = student.clone();
            candidate.term = term.clone();
            if students::find_duplicate_in(&mut tx, &candidate).await?.is_some() {
                summary.students_skipped += 1;
                continue;
            }

            // Yürürlük tarihi BUGÜNdür: içe aktarma "şimdi" olan bir eylemdir.
            // `None` bırakılırsa dönem başladıktan sonra (MADDE 5/1-ç) kapı
            // her öğrenciyi EffectiveDateRequired ile reddeder — CSV içe
            // aktarımının kendi bir tarih alanı yoktur, o yüzden `today`
            // burada açıkça verilir (`resolve_effective_date` `today`nin
            // dönem içinde olduğunu zaten doğrular).
            let req = ChangeRequest {
                term: term.clone(),
                effective_date: Some(today),
                document_date: None,
                reason: IMPORT_REASON.to_string(),
                command: ChangeCommand::CreateStudent {
                    student: NewStudentInput {
                        first_name: student.first_name.clone(),
                        last_name: student.last_name.clone(),
                        student_no: student.student_no.clone(),
                        grade: student.grade.clone(),
                        branch: student.branch.clone(),
                        submitted_at: student.submitted_at.clone(),
                    },
                    company_id: Some(company_id),
                },
            };
            let outcome =
                change_service::execute_in(&mut tx, req, ChangeMode::Commit { expected_high_water: None }, today).await?;
            commit_outcome_to_result(&student, outcome)?;
            summary.students_created += 1;
        }
    }

    tx.commit().await?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;

    const LOCATOR: &str = "Result: Test Lisesi, Toroslar/Mersin\n\
         Distance: 6.8 km\n\
         Address: Test Mahallesi, 1. Sokak No:1, Mersin";

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    fn csv(rows: &str) -> String {
        let header = "\"Submission Date\",Ad,Soyad,\"İşletme Adı\",Ad,Soyad,\
             \"Telefon Numarası\",\"İşletmenizi bulun\",E-posta,\
             \"DAL Bİlgisi Seçiniz\",\"Öğrenci No\",\"Sınıf Seçiniz\"\n";
        format!("\u{feff}{header}{rows}")
    }

    fn row(student_first: &str, student_last: &str, company: &str, grade: &str) -> String {
        format!(
            "\"Sep 11, 2026\",{student_first},{student_last},\"{company}\",Yetkili,Soyad,\
             \"(500) 000-0000\",\"{LOCATOR}\",,\"Elektronik Haberleşme\",,{grade}\n"
        )
    }

    /// Aynı işletmeye giden iki öğrenci tek işletme kaydına inmeli.
    #[tokio::test]
    async fn preview_groups_students_under_one_company() {
        let (_dir, pool) = test_pool().await;
        let content = csv(&format!(
            "{}{}",
            row("Ahmet", "Yilmaz", "MEKA OTOMASYON", "12/C"),
            row("Ayse", "Demir", "meka   otomasyon", "12/D")
        ));

        let preview = preview(&pool, &content).await.unwrap();

        assert_eq!(preview.groups.len(), 1, "tek işletme bekleniyordu");
        assert_eq!(preview.groups[0].student_count, 2);
        assert_eq!(preview.total_students, 2);
        assert_eq!(preview.duplicate_count, 0);
    }

    /// Gidiş-dönüş mesafe önizlemede tek yönün iki katı gösterilmeli.
    #[tokio::test]
    async fn preview_reports_round_trip_distance() {
        let (_dir, pool) = test_pool().await;
        let preview = preview(&pool, &csv(&row("Ahmet", "Yilmaz", "TEST A", "12/C")))
            .await
            .unwrap();

        assert_eq!(preview.groups[0].one_way_distance_km, Some(6.8));
        assert_eq!(preview.groups[0].round_trip_distance_km, Some(13.6));
    }

    #[tokio::test]
    async fn preview_flags_existing_company_as_duplicate() {
        let (_dir, pool) = test_pool().await;
        companies::create(
            &pool,
            &NewCompany {
                name: "MEKA OTOMASYON".into(),
                contact_first_name: String::new(),
                contact_last_name: String::new(),
                phone: String::new(),
                email: String::new(),
                address_text: "Eski adres".into(),
                latitude: None,
                longitude: None,
                one_way_distance_km: Some(1.0),
                district: String::new(),
                notes: String::new(),
            },
        )
        .await
        .unwrap();

        let preview = preview(&pool, &csv(&row("Ahmet", "Yilmaz", "meka otomasyon", "12/C")))
            .await
            .unwrap();

        assert_eq!(preview.duplicate_count, 1);
        assert!(preview.groups[0].existing_company_id.is_some());
    }

    #[tokio::test]
    async fn apply_creates_company_and_students() {
        let (_dir, pool) = test_pool().await;
        let content = csv(&format!(
            "{}{}",
            row("Ahmet", "Yilmaz", "TEST A", "12/C"),
            row("Ayse", "Demir", "TEST B", "12/D")
        ));

        let summary = apply(&pool, &content, &BTreeMap::new()).await.unwrap();

        assert_eq!(summary.companies_created, 2);
        assert_eq!(summary.students_created, 2);
        assert_eq!(companies::list(&pool).await.unwrap().len(), 2);
        assert_eq!(students::list(&pool).await.unwrap().len(), 2);
    }

    /// Teşhis'in kanıtı: içe aktarılan öğrenci kapıdan (`CreateStudent{ companyId }`)
    /// geçer, bu yüzden hem `student_placements`te açık bir satırı hem de
    /// `change_events`te bir `student_placed` olayı vardır — ham `students::create`
    /// bunların HİÇBİRİNİ üretmezdi.
    #[tokio::test]
    async fn apply_places_the_imported_student_through_the_gate() {
        let (_dir, pool) = test_pool().await;
        let content = csv(&row("Ahmet", "Yilmaz", "TEST A", "12/C"));

        apply(&pool, &content, &BTreeMap::new()).await.unwrap();

        let term = settings::get_active_term(&pool).await.unwrap();
        let student = students::list_by_term(&pool, &term).await.unwrap().into_iter().next().unwrap();
        let company = companies::list(&pool).await.unwrap().into_iter().find(|c| c.name == "TEST A").unwrap();

        assert_eq!(student.company_id, Some(company.id), "öğrenci kapıdan işletmeye yerleştirilmeli");

        let placed_kind_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM change_events WHERE kind = 'student_placed' AND subject_id = ?1",
        )
        .bind(student.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(placed_kind_count, 1, "student_placed olayı oluşmalı");

        let open_row: Option<i64> = sqlx::query_scalar(
            "SELECT company_id FROM student_placements WHERE student_id = ?1 AND term = ?2 AND valid_to IS NULL",
        )
        .bind(student.id)
        .bind(&term)
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert_eq!(open_row, Some(company.id), "açık yerleştirme satırı oluşmalı");
    }

    /// Bir öğrenci kapıdan reddedilirse (`Rejected`) TÜM içe aktarma geri
    /// alınır: önceki gruplarda zaten oluşturulmuş ya da eşleşmiş kayıtlar
    /// dahil hiçbir şey kalıcı olmaz (brief madde 3).
    #[tokio::test]
    async fn apply_rolls_back_everything_when_the_gate_rejects_a_student() {
        let (_dir, pool) = test_pool().await;
        // "TEST A" pasif olarak önceden var: o gruptaki öğrenci reddedilecek
        // (pasif işletme yeni atama hedefi olamaz).
        let inactive = companies::create(
            &pool,
            &NewCompany {
                name: "TEST A".into(),
                contact_first_name: String::new(),
                contact_last_name: String::new(),
                phone: String::new(),
                email: String::new(),
                address_text: "Eski adres".into(),
                latitude: None,
                longitude: None,
                one_way_distance_km: Some(1.0),
                district: String::new(),
                notes: String::new(),
            },
        )
        .await
        .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        companies::set_active_in(&mut conn, inactive.id, false).await.unwrap();
        drop(conn);

        let content = csv(&format!(
            "{}{}",
            row("Ahmet", "Yilmaz", "TEST A", "12/C"),
            row("Ayse", "Demir", "TEST B", "12/D"),
        ));

        let result = apply(&pool, &content, &BTreeMap::new()).await;

        assert!(matches!(result, Err(AppError::Validation(_))), "beklenmeyen sonuç: {result:?}");
        // `list_all`: hayatta kalan tek kayıt PASİF (`list` artık onu süzerdi;
        // burada sınanan geri alma davranışı, işletmenin aktiflik durumu
        // DEĞİL — süzülmemiş sayım kullanılmalı).
        assert_eq!(companies::list_all(&pool).await.unwrap().len(), 1, "yalnız önceden var olan pasif işletme kalmalı");
        assert_eq!(students::list(&pool).await.unwrap().len(), 0, "hiçbir öğrenci kalıcı olmamalı");
    }

    /// Varsayılan politika Merge: mevcut işletme kullanılır, yenisi açılmaz.
    #[tokio::test]
    async fn apply_merges_into_existing_company_by_default() {
        let (_dir, pool) = test_pool().await;
        let content = csv(&row("Ahmet", "Yilmaz", "TEST A", "12/C"));
        apply(&pool, &content, &BTreeMap::new()).await.unwrap();

        let second = csv(&row("Ayse", "Demir", "test   a", "12/D"));
        let summary = apply(&pool, &second, &BTreeMap::new()).await.unwrap();

        assert_eq!(summary.companies_created, 0);
        assert_eq!(summary.companies_matched, 1);
        assert_eq!(summary.students_created, 1);
        assert_eq!(companies::list(&pool).await.unwrap().len(), 1);
        assert_eq!(students::list(&pool).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn apply_skip_policy_leaves_company_and_students_untouched() {
        let (_dir, pool) = test_pool().await;
        apply(&pool, &csv(&row("Ahmet", "Yilmaz", "TEST A", "12/C")), &BTreeMap::new())
            .await
            .unwrap();

        let mut policies = BTreeMap::new();
        policies.insert("test a".to_string(), DuplicatePolicy::Skip);

        let summary = apply(
            &pool,
            &csv(&row("Ayse", "Demir", "TEST A", "12/D")),
            &policies,
        )
        .await
        .unwrap();

        assert_eq!(summary.companies_skipped, 1);
        assert_eq!(summary.students_skipped, 1);
        assert_eq!(students::list(&pool).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn apply_update_policy_overwrites_company_fields() {
        let (_dir, pool) = test_pool().await;
        companies::create(
            &pool,
            &NewCompany {
                name: "TEST A".into(),
                contact_first_name: String::new(),
                contact_last_name: String::new(),
                phone: String::new(),
                email: String::new(),
                address_text: "Eski adres".into(),
                latitude: None,
                longitude: None,
                one_way_distance_km: Some(1.0),
                district: String::new(),
                notes: String::new(),
            },
        )
        .await
        .unwrap();

        let mut policies = BTreeMap::new();
        policies.insert("test a".to_string(), DuplicatePolicy::Update);

        let summary = apply(
            &pool,
            &csv(&row("Ahmet", "Yilmaz", "TEST A", "12/C")),
            &policies,
        )
        .await
        .unwrap();

        assert_eq!(summary.companies_updated, 1);
        let updated = &companies::list(&pool).await.unwrap()[0];
        assert_eq!(updated.one_way_distance_km, Some(6.8));
        assert!(updated.address_text.contains("1. Sokak"));
    }

    /// Aynı dosya iki kez uygulanırsa öğrenciler çoğalmamalı.
    #[tokio::test]
    async fn applying_same_file_twice_does_not_duplicate_students() {
        let (_dir, pool) = test_pool().await;
        let content = csv(&row("Ahmet", "Yilmaz", "TEST A", "12/C"));

        apply(&pool, &content, &BTreeMap::new()).await.unwrap();
        let summary = apply(&pool, &content, &BTreeMap::new()).await.unwrap();

        assert_eq!(summary.students_created, 0);
        assert_eq!(summary.students_skipped, 1);
        assert_eq!(students::list(&pool).await.unwrap().len(), 1);
    }

    /// Gerçek JotForm dışa aktarımını baştan sona içe aktarır. Dosya
    /// `data/` klasöründe yoksa test atlanır (depoya commit edilmez, bkz.
    /// `.gitignore`: `/data`), böylece CSV olmadan da `cargo test` yeşil
    /// kalır — ama atlama `real_export_fixture::find_real_export` içinde
    /// `eprintln!` ile açıkça bildirilir.
    #[tokio::test]
    async fn imports_the_real_jotform_export_end_to_end() {
        let Some(csv_path) = crate::services::real_export_fixture::find_real_export(
            "csv",
            "ATLANDI: data/ klasöründe gerçek JotForm CSV'si yok",
        ) else {
            return;
        };

        let content = std::fs::read_to_string(&csv_path).unwrap();
        let (_dir, pool) = test_pool().await;

        // Önizleme: 28 tekil işletme, 32 öğrenci, hiçbiri mevcut değil.
        let preview = preview(&pool, &content).await.unwrap();
        assert!(preview.errors.is_empty(), "ayrıştırma hataları: {:?}", preview.errors);
        assert_eq!(preview.groups.len(), 28, "beklenen 28 tekil işletme");
        assert_eq!(preview.total_students, 32, "beklenen 32 öğrenci");
        assert_eq!(preview.duplicate_count, 0, "boş veritabanında çakışma olmamalı");

        // 4 işletme iki öğrencili olmalı.
        let with_two = preview.groups.iter().filter(|g| g.student_count == 2).count();
        assert_eq!(with_two, 4, "beklenen 4 adet iki öğrencili işletme");

        // Her grupta gidiş-dönüş mesafe tek yönün iki katı olmalı.
        for group in &preview.groups {
            let one_way = group.one_way_distance_km.expect("mesafe okunamadı");
            let round_trip = group.round_trip_distance_km.expect("gidiş-dönüş yok");
            assert!((round_trip - one_way * 2.0).abs() < 1e-9);
        }

        // Uygulama
        let summary = apply(&pool, &content, &BTreeMap::new()).await.unwrap();
        assert_eq!(summary.companies_created, 28);
        assert_eq!(summary.students_created, 32);
        assert!(summary.errors.is_empty());

        assert_eq!(companies::list(&pool).await.unwrap().len(), 28);
        assert_eq!(students::list(&pool).await.unwrap().len(), 32);

        // İkinci kez uygulamak hiçbir şey eklememeli.
        let again = apply(&pool, &content, &BTreeMap::new()).await.unwrap();
        assert_eq!(again.companies_created, 0);
        assert_eq!(again.companies_matched, 28);
        assert_eq!(again.students_created, 0);
        assert_eq!(again.students_skipped, 32);
        assert_eq!(companies::list(&pool).await.unwrap().len(), 28);
        assert_eq!(students::list(&pool).await.unwrap().len(), 32);
    }

    /// Ayrıştırılamayan satır özet içinde raporlanmalı, sessizce yutulmamalı.
    #[tokio::test]
    async fn apply_reports_unparsable_rows() {
        let (_dir, pool) = test_pool().await;
        let broken = "\"Sep 11, 2026\",A,B,\"ADRESSIZ\",C,D,\"\",\"Result: sadece okul\",,DAL,,12/C\n";
        let content = csv(&format!("{}{}", row("Ahmet", "Yilmaz", "TEST A", "12/C"), broken));

        let summary = apply(&pool, &content, &BTreeMap::new()).await.unwrap();

        assert_eq!(summary.companies_created, 1);
        assert_eq!(summary.errors.len(), 1);
        assert!(summary.errors[0].contains("ADRESSIZ"));
    }
}
