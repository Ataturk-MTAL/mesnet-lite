//! Aktif dönemin atama, işletme ve öğrenci verilerini bellekte bir Excel (.xlsx)
//! çalışma kitabına dönüştürür. Dosya diske yazılmaz; baytlar çağırana döner ve
//! arayüz bunu indirilebilir bir dosyaya çevirir.

use crate::db::assignments;
use crate::db::companies;
use crate::db::company_hours;
use crate::db::read_at::ReadAt;
use crate::db::students;
use crate::db::teachers;
use crate::error::{AppError, AppResult};
use rust_xlsxwriter::{Format, Workbook, Worksheet, XlsxError};
use sqlx::SqlitePool;
use std::collections::BTreeMap;

/// `rust_xlsxwriter` kendi hata tipini döner; uygulamanın tek hata tipine
/// burada çevrilir ki çağıran taraflar `?` operatörünü doğrudan kullanabilsin.
impl From<XlsxError> for AppError {
    fn from(err: XlsxError) -> Self {
        AppError::Io(format!("Excel dosyası oluşturulamadı: {err}"))
    }
}

/// Ziyaret günü numarasını (1 = Pazartesi … 5 = Cuma) Türkçe gün adına çevirir.
/// Şema dışı bir değer gelirse (olmaması gerekir) "Bilinmiyor" döner.
fn day_name(day: i64) -> &'static str {
    match day {
        1 => "Pazartesi",
        2 => "Salı",
        3 => "Çarşamba",
        4 => "Perşembe",
        5 => "Cuma",
        _ => "Bilinmiyor",
    }
}

/// Ziyaret saatini blok olarak biçimlendirir.
///
/// Bir koordinatörlük ataması artık tek bir ders saati değil, ardışık
/// saatlerden oluşan bir BLOK kaplar: `hour` ile `hour + span - 1` arası, her
/// iki uç dahil. `span`, çağıran tarafından `max(1, awarded_hours)` olarak
/// hesaplanıp geçirilir. Tek saatlik blokta (`span <= 1`) yalnızca başlangıç
/// saati yazılır; `4-4` gibi çirkin ve yanıltıcı bir aralık üretilmez.
fn format_visit_hour(hour: i64, span: i64) -> String {
    let effective_span = span.max(1);
    if effective_span <= 1 {
        hour.to_string()
    } else {
        format!("{}-{}", hour, hour + effective_span - 1)
    }
}

/// İşletmenin coğrafi kodlama durumunu Türkçe etikete çevirir.
fn geocode_status_label(status: &str) -> &'static str {
    match status {
        "pending" => "Bekliyor",
        "resolved" => "Bulundu",
        "failed" => "Bulunamadı",
        "manual" => "Elle Düzeltildi",
        _ => "Bilinmiyor",
    }
}

/// Boole değeri Excel'de okunaklı Türkçe metne çevirir.
fn yes_no(value: bool) -> &'static str {
    if value {
        "Evet"
    } else {
        "Hayır"
    }
}

/// Adı verilen bir çalışma sayfası ekler.
fn add_sheet<'a>(workbook: &'a mut Workbook, name: &str) -> AppResult<&'a mut Worksheet> {
    let worksheet = workbook.add_worksheet();
    worksheet.set_name(name)?;
    Ok(worksheet)
}

/// Başlık satırını kalın yazar ve başlık satırının altını dondurur.
fn write_headers(worksheet: &mut Worksheet, headers: &[&str], bold: &Format) -> AppResult<()> {
    for (col, header) in headers.iter().enumerate() {
        worksheet.write_string_with_format(0, col as u16, *header, bold)?;
    }
    worksheet.set_freeze_panes(1, 0)?;
    Ok(())
}

/// Sütun genişliklerini sırasıyla uygular.
fn set_column_widths(worksheet: &mut Worksheet, widths: &[f64]) -> AppResult<()> {
    for (col, width) in widths.iter().enumerate() {
        worksheet.set_column_width(col as u16, *width)?;
    }
    Ok(())
}

/// "Atamalar" sayfası: her satır bir işletmenin yerleşimini, öğretmenini ve
/// takdir edilen haftalık saatini gösterir.
async fn write_assignments_sheet(
    workbook: &mut Workbook,
    pool: &SqlitePool,
    term: &str,
    read_at: &ReadAt,
    bold: &Format,
) -> AppResult<()> {
    let assignment_rows = assignments::list(pool, term, read_at).await?;
    // `list_all`: dönem ortasında pasifleşen bir işletmenin atama satırı
    // dışa aktarımdan KAYBOLMAMALI (spec §5.4, dışa aktarım geçmişe bakar).
    let companies_by_id: BTreeMap<i64, _> = companies::list_all(pool)
        .await?
        .into_iter()
        .map(|company| (company.id, company))
        .collect();
    let teachers_by_id: BTreeMap<i64, _> = teachers::list(pool)
        .await?
        .into_iter()
        .map(|teacher| (teacher.id, teacher))
        .collect();
    let hours_by_company: BTreeMap<i64, _> = company_hours::list(pool, term, read_at)
        .await?
        .into_iter()
        .map(|hours| (hours.company_id, hours))
        .collect();

    let worksheet = add_sheet(workbook, "Atamalar")?;
    let headers = [
        "İşletme",
        "Adres",
        "Öğretmen",
        "Ziyaret Günü",
        "Ziyaret Saati",
        "Haftalık Saat",
        "Fahri",
        "Zorlanmış",
        "Gerekçe",
    ];
    write_headers(worksheet, &headers, bold)?;
    set_column_widths(
        worksheet,
        &[28.0, 34.0, 22.0, 14.0, 14.0, 14.0, 10.0, 12.0, 30.0],
    )?;

    for (row_index, assignment) in assignment_rows.iter().enumerate() {
        let row = (row_index + 1) as u32;

        let company_name = companies_by_id
            .get(&assignment.company_id)
            .map(|company| company.name.as_str())
            .unwrap_or_default();
        let address = companies_by_id
            .get(&assignment.company_id)
            .map(|company| company.address_text.as_str())
            .unwrap_or_default();
        let teacher_name = teachers_by_id
            .get(&assignment.teacher_id)
            .map(|teacher| format!("{} {}", teacher.first_name, teacher.last_name))
            .unwrap_or_default();

        let hours = hours_by_company.get(&assignment.company_id);
        let awarded_hours = hours.map(|h| h.awarded_hours).unwrap_or(0);
        let is_honorary = hours.map(|h| h.is_honorary == 1).unwrap_or(false);

        worksheet.write_string(row, 0, company_name)?;
        worksheet.write_string(row, 1, address)?;
        worksheet.write_string(row, 2, teacher_name)?;
        let hour_span = awarded_hours.max(1);
        worksheet.write_string(row, 3, day_name(assignment.visit_day))?;
        worksheet.write_string(row, 4, format_visit_hour(assignment.visit_hour, hour_span))?;
        worksheet.write_number(row, 5, awarded_hours as f64)?;
        worksheet.write_string(row, 6, yes_no(is_honorary))?;
        worksheet.write_string(row, 7, yes_no(assignment.is_forced == 1))?;
        worksheet.write_string(row, 8, assignment.force_reason.clone().unwrap_or_default())?;
    }

    Ok(())
}

/// "İşletmeler" sayfası: dönemden bağımsız kalıcı işletme kaydı, artı dönemlik
/// öğrenci sayısı.
async fn write_companies_sheet(
    workbook: &mut Workbook,
    pool: &SqlitePool,
    term: &str,
    read_at: &ReadAt,
    bold: &Format,
) -> AppResult<()> {
    // `list_all`: bu sayfa "dönemden bağımsız kalıcı işletme kaydı"nı dışa
    // aktarır; pasif bir işletme bu kayıttan silinmiş gibi görünmemeli.
    let all_companies = companies::list_all(pool).await?;
    let student_counts: BTreeMap<i64, i64> = students::count_by_company(pool, term, read_at)
        .await?
        .into_iter()
        .collect();

    let worksheet = add_sheet(workbook, "İşletmeler")?;
    let headers = [
        "İşletme Adı",
        "Yetkili Adı",
        "Yetkili Soyadı",
        "Telefon",
        "E-posta",
        "Adres",
        "Tek Yön (km)",
        "Gidiş-Dönüş (km)",
        "Öğrenci Sayısı",
        "Konum Durumu",
    ];
    write_headers(worksheet, &headers, bold)?;
    set_column_widths(
        worksheet,
        &[26.0, 16.0, 16.0, 16.0, 24.0, 34.0, 12.0, 16.0, 14.0, 18.0],
    )?;

    for (row_index, company) in all_companies.iter().enumerate() {
        let row = (row_index + 1) as u32;
        let student_count = student_counts.get(&company.id).copied().unwrap_or(0);

        worksheet.write_string(row, 0, company.name.as_str())?;
        worksheet.write_string(row, 1, company.contact_first_name.as_str())?;
        worksheet.write_string(row, 2, company.contact_last_name.as_str())?;
        worksheet.write_string(row, 3, company.phone.as_str())?;
        worksheet.write_string(row, 4, company.email.as_str())?;
        worksheet.write_string(row, 5, company.address_text.as_str())?;

        match company.one_way_distance_km {
            Some(km) => worksheet.write_number(row, 6, km)?,
            None => worksheet.write_string(row, 6, "")?,
        };
        match company.round_trip_distance_km() {
            Some(km) => worksheet.write_number(row, 7, km)?,
            None => worksheet.write_string(row, 7, "")?,
        };

        worksheet.write_number(row, 8, student_count as f64)?;
        worksheet.write_string(row, 9, geocode_status_label(&company.geocode_status))?;
    }

    Ok(())
}

/// "Öğrenciler" sayfası: yalnızca verilen dönemin öğrencileri.
async fn write_students_sheet(
    workbook: &mut Workbook,
    pool: &SqlitePool,
    term: &str,
    read_at: &ReadAt,
    bold: &Format,
) -> AppResult<()> {
    let term_students = students::list_by_term(pool, term, read_at).await?;
    // `list_all`: öğrenci, artık pasif bir işletmeye yerleştirilmiş olabilir
    // (dönem ortasında birleştirme/pasifleşme); ad süzülmüş listede kaybolmamalı.
    let companies_by_id: BTreeMap<i64, _> = companies::list_all(pool)
        .await?
        .into_iter()
        .map(|company| (company.id, company))
        .collect();

    let worksheet = add_sheet(workbook, "Öğrenciler")?;
    let headers = ["Öğrenci No", "Ad", "Soyad", "Sınıf", "Dal", "İşletme"];
    write_headers(worksheet, &headers, bold)?;
    set_column_widths(worksheet, &[14.0, 16.0, 16.0, 10.0, 28.0, 26.0])?;

    for (row_index, student) in term_students.iter().enumerate() {
        let row = (row_index + 1) as u32;
        let company_name = student
            .company_id
            .and_then(|id| companies_by_id.get(&id))
            .map(|company| company.name.as_str())
            .unwrap_or_default();

        worksheet.write_string(row, 0, student.student_no.clone().unwrap_or_default())?;
        worksheet.write_string(row, 1, student.first_name.as_str())?;
        worksheet.write_string(row, 2, student.last_name.as_str())?;
        worksheet.write_string(row, 3, student.grade.as_str())?;
        worksheet.write_string(row, 4, student.branch.as_str())?;
        worksheet.write_string(row, 5, company_name)?;
    }

    Ok(())
}

/// Verilen dönem için üç sayfalı bir Excel çalışma kitabı üretir ve baytlarını
/// döner. Sırasıyla: Atamalar, İşletmeler, Öğrenciler. `read_at`, kenar
/// çubuğunda seçilen tarihtir (`Latest` = güncel durum).
pub async fn build_workbook(pool: &SqlitePool, term: &str, read_at: &ReadAt) -> AppResult<Vec<u8>> {
    let mut workbook = Workbook::new();
    let bold = Format::new().set_bold();

    write_assignments_sheet(&mut workbook, pool, term, read_at, &bold).await?;
    write_companies_sheet(&mut workbook, pool, term, read_at, &bold).await?;
    write_students_sheet(&mut workbook, pool, term, read_at, &bold).await?;

    Ok(workbook.save_to_buffer()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;
    use crate::db::legacy_seed_test_support::{seed_coordinator, seed_hours};
    use crate::domain::history::decide::{ChangeCommand, ChangeRequest, NewStudentInput};
    use crate::domain::models::{NewCompany, NewStudent, NewTeacher};
    use crate::services::change_service::{execute_change, ChangeMode, ChangeOutcome};
    use chrono::NaiveDate;

    const TERM: &str = "2026-2027/1";

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    #[tokio::test]
    async fn empty_database_still_produces_a_non_empty_workbook() {
        let (_dir, pool) = test_pool().await;
        let bytes = build_workbook(&pool, TERM, &ReadAt::Latest).await.unwrap();

        assert!(!bytes.is_empty(), "boş veritabanında da başlıklar yazılmalı");
    }

    /// İşletme, öğretmen, öğrenci ve atama girildiğinde çıkan dosya daha büyük
    /// olmalı; bu, verinin sayfalara gerçekten yazıldığının dolaylı kanıtıdır.
    #[tokio::test]
    async fn seeded_records_produce_a_larger_workbook_than_an_empty_one() {
        let (_empty_dir, empty_pool) = test_pool().await;
        let empty_bytes = build_workbook(&empty_pool, TERM, &ReadAt::Latest).await.unwrap();

        let (_dir, pool) = test_pool().await;

        let company = companies::create(
            &pool,
            &NewCompany {
                name: "Test İşletme A".into(),
                contact_first_name: "Test".into(),
                contact_last_name: "Yetkili".into(),
                phone: "(500) 000-0000".into(),
                email: "test@example.com".into(),
                address_text: "Test Mahallesi, Test Sokak No:1, Mersin".into(),
                latitude: None,
                longitude: None,
                one_way_distance_km: Some(6.8),
                district: String::new(),
                notes: String::new(),
            },
        )
        .await
        .unwrap();

        let teacher = teachers::create(
            &pool,
            &NewTeacher {
                first_name: "Test".into(),
                last_name: "Öğretmen".into(),
                registry_no: "123456".into(),
                field: "Elektrik-Elektronik Teknolojisi".into(),
                branches: vec!["Elektronik Haberleşme".into()],
                employment_type: "tenured".into(),
                base_hours: 20,
                max_extra_hours: 24,
                other_extra_hours: 0,
                chief_type: "none".into(),
                is_active: true,
            },
        )
        .await
        .unwrap();

        students::create(
            &pool,
            &NewStudent {
                first_name: "Test".into(),
                last_name: "Öğrenci".into(),
                student_no: Some("1001".into()),
                grade: "12/C".into(),
                branch: "Elektronik Haberleşme".into(),
                submitted_at: Some("2026-09-11".into()),
                term: TERM.into(),
            },
        )
        .await
        .unwrap();

        seed_hours(&pool, TERM, company.id, 6, false).await;
        seed_coordinator(&pool, TERM, company.id, teacher.id, 2, 3, false, None).await;

        let seeded_bytes = build_workbook(&pool, TERM, &ReadAt::Latest).await.unwrap();

        assert!(
            seeded_bytes.len() > empty_bytes.len(),
            "doldurulmuş dönem boş dönemden daha büyük bir dosya üretmeli"
        );
    }

    /// Dönem başlamadan önceki "bugün" — `pdf_report`teki AYNI sabit
    /// (`ChangeRequest::effective_date` boş bırakılırsa dönem başına çözülür).
    fn planning_today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 15).unwrap()
    }

    /// GERÇEK yazma yoluyla (`execute_change`) işletmeye tek bir öğrenci
    /// yerleştirir; `cap_for` öğrencisiz işletmede 0'a zorladığı için
    /// (`domain::history::policy::cap_for`) saat takdirinden ÖNCE gerekir.
    async fn place_student(pool: &SqlitePool, company_id: i64, first: &str) {
        let req = ChangeRequest {
            term: TERM.into(),
            effective_date: None,
            document_date: None,
            reason: "test".into(),
            command: ChangeCommand::CreateStudent {
                student: NewStudentInput {
                    first_name: first.into(),
                    last_name: "Öğrenci".into(),
                    student_no: None,
                    grade: "12/C".into(),
                    branch: "Elektronik Haberleşme".into(),
                    submitted_at: None,
                },
                company_id: Some(company_id),
            },
        };
        let outcome = execute_change(pool, req, ChangeMode::Commit { expected_high_water: None }, planning_today())
            .await
            .unwrap();
        assert!(matches!(outcome, ChangeOutcome::Committed { .. }), "Committed beklenirdi: {outcome:?}");
    }

    /// GERÇEK yazma yoluyla (`execute_change` → `SetCompanyHours`) bir
    /// işletmenin saatini `effective_date`ten itibaren değiştirir; iki farklı
    /// tarihte çağrılırsa `company_hour_periods`te GERÇEKTEN iki ayrı satır
    /// açar — `legacy_seed_test_support::seed_hours`in aksine (o tek satırlık
    /// bir sahne kurar, `AsOf`/`Latest` farkını sınayamaz).
    async fn set_hours(pool: &SqlitePool, company_id: i64, awarded_hours: i64, effective_date: Option<&str>, today: NaiveDate) {
        let row = crate::db::company_hours::HoursInput {
            company_id,
            max_hours_snapshot: 1000,
            awarded_hours,
            is_honorary: false,
            is_locked: false,
            notes: String::new(),
        };
        crate::commands::hours_commands::save_hours_for_term(pool, TERM, &[row], effective_date.map(str::to_string), None, today)
            .await
            .unwrap();
    }

    /// Kenar çubuğunda seçilen tarihe göre üretim (kullanıcı kararı, spec §6):
    /// `AsOf` değişiklikten ÖNCEki saati (4), `Latest` (açık satır) SONRAki
    /// saati (8) basmalı — "Atamalar" sayfasındaki hücre farklı olduğu için
    /// iki çalışma kitabının baytları da farklı olmalı.
    #[tokio::test]
    async fn as_of_workbook_reflects_the_hours_in_effect_on_that_date() {
        let (_dir, pool) = test_pool().await;

        let company = companies::create(&pool, &NewCompany {
            name: "Tarihli İşletme".into(),
            contact_first_name: String::new(),
            contact_last_name: String::new(),
            phone: String::new(),
            email: String::new(),
            address_text: "Test Mahallesi".into(),
            latitude: None,
            longitude: None,
            one_way_distance_km: Some(6.8),
            district: String::new(),
            notes: String::new(),
        })
        .await
        .unwrap();
        let teacher = teachers::create(&pool, &NewTeacher {
            first_name: "Test".into(),
            last_name: "Öğretmen".into(),
            registry_no: "1".into(),
            field: "Elektrik-Elektronik Teknolojisi".into(),
            branches: vec![],
            employment_type: "tenured".into(),
            base_hours: 20,
            max_extra_hours: 24,
            other_extra_hours: 0,
            chief_type: "none".into(),
            is_active: true,
        })
        .await
        .unwrap();

        place_student(&pool, company.id, "Ada").await;
        seed_coordinator(&pool, TERM, company.id, teacher.id, 2, 3, false, None).await;
        set_hours(&pool, company.id, 4, None, planning_today()).await;
        set_hours(&pool, company.id, 8, Some("2026-10-05"), NaiveDate::from_ymd_opt(2026, 10, 5).unwrap()).await;

        let before = NaiveDate::from_ymd_opt(2026, 9, 15).unwrap();
        let as_of_bytes = build_workbook(&pool, TERM, &ReadAt::AsOf(before)).await.unwrap();
        let latest_bytes = build_workbook(&pool, TERM, &ReadAt::Latest).await.unwrap();

        assert_ne!(
            as_of_bytes, latest_bytes,
            "AsOf ve Latest farklı saat basmalı, çıktı baytları aynı olmamalı"
        );
    }

    /// Tek saatlik blok (span = 1): aralık gösterilmez, yalnızca başlangıç
    /// saati yazılır.
    #[test]
    fn format_visit_hour_prints_a_single_number_when_span_is_one() {
        assert_eq!(format_visit_hour(4, 1), "4");
    }

    /// Çok saatlik blok (span > 1): `başlangıç-bitiş` aralığı basılır, bitiş =
    /// hour + span - 1 (her iki uç dahil).
    #[test]
    fn format_visit_hour_prints_a_range_when_span_is_greater_than_one() {
        assert_eq!(format_visit_hour(4, 6), "4-9");
    }

    /// Fahri ziyaret gibi span <= 0 durumları savunmacı biçimde tek saate
    /// zorlanır; asla `4-3` gibi geçersiz bir aralık üretilmez.
    #[test]
    fn format_visit_hour_treats_zero_or_negative_span_as_a_single_hour() {
        assert_eq!(format_visit_hour(2, 0), "2");
        assert_eq!(format_visit_hour(2, -3), "2");
    }

    #[test]
    fn day_name_maps_weekday_numbers_to_turkish_names() {
        assert_eq!(day_name(1), "Pazartesi");
        assert_eq!(day_name(2), "Salı");
        assert_eq!(day_name(3), "Çarşamba");
        assert_eq!(day_name(4), "Perşembe");
        assert_eq!(day_name(5), "Cuma");
        // Şema dışı bir gün numarası gelirse çökmemeli.
        assert_eq!(day_name(0), "Bilinmiyor");
        assert_eq!(day_name(9), "Bilinmiyor");
    }

    #[test]
    fn geocode_status_label_maps_known_statuses_to_turkish() {
        assert_eq!(geocode_status_label("pending"), "Bekliyor");
        assert_eq!(geocode_status_label("resolved"), "Bulundu");
        assert_eq!(geocode_status_label("failed"), "Bulunamadı");
        assert_eq!(geocode_status_label("manual"), "Elle Düzeltildi");
        assert_eq!(geocode_status_label("bilinmeyen"), "Bilinmiyor");
    }

    #[test]
    fn yes_no_translates_booleans_to_turkish() {
        assert_eq!(yes_no(true), "Evet");
        assert_eq!(yes_no(false), "Hayır");
    }
}
