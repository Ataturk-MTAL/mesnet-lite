//! Typst tabanlı PDF rapor üretimi.
//!
//! İki resmi belge üretir: Koordinatör Görevlendirme Çizelgesi (onaya sunulan
//! belge) ve öğretmen başına Ziyaret Listesi. Veri veritabanından okunur, JSON'a
//! çevrilir ve `sys.inputs` üzerinden Typst şablonuna aktarılır — şablon hiçbir
//! zaman dizgi birleştirmesiyle üretilmez.

use crate::db::assignments::Assignment;
use crate::db::company_hours::CompanyTermHours;
use crate::db::{assignments, companies, company_hours, settings, students, teachers};
use crate::domain::models::{Company, Student, Teacher};
use crate::error::{AppError, AppResult};
use serde::Serialize;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::OnceLock;
use typst::foundations::{Dict, IntoValue};
use typst_as_lib::{TypstEngine, TypstTemplateMainFile};
use typst_layout::PagedDocument;

const ASSIGNMENT_SHEET_TEMPLATE: &str = include_str!("../../templates/assignment_sheet.typ");
const VISIT_LIST_TEMPLATE: &str = include_str!("../../templates/visit_list.typ");

// DejaVu Sans, Latin Extended-A'yı (ğ, Ğ, ı, İ, ş, Ş, ö, Ö, ç, Ç, ü, Ü dahil)
// tam kapsayan, yeniden dağıtılabilir (Bitstream Vera lisansı) bir yazı tipi
// ailesidir. `typst-as-lib` sistem yazı tiplerine erişemediği için ikisi de
// derleme zamanında ikili olarak gömülür.
pub(crate) const FONT_REGULAR: &[u8] = include_bytes!("../../assets/fonts/DejaVuSans.ttf");
pub(crate) const FONT_BOLD: &[u8] = include_bytes!("../../assets/fonts/DejaVuSans-Bold.ttf");

/// Gün numarasını (1 = Pazartesi … 5 = Cuma) Türkçe gün adına çevirir.
///
/// Tanınmayan bir numara veri bozulmasını gizlemeden "Gün N" olarak basılır;
/// hiçbir zaman sessizce Pazartesi'ye düşülmez.
pub fn day_name(day: i64) -> String {
    match day {
        1 => "Pazartesi".to_string(),
        2 => "Salı".to_string(),
        3 => "Çarşamba".to_string(),
        4 => "Perşembe".to_string(),
        5 => "Cuma".to_string(),
        other => format!("Gün {other}"),
    }
}

/// Bir koordinatörlük ataması artık tek bir ders saati değil, ardışık
/// saatlerden oluşan bir BLOK kaplar: `visit_hour` ile `visit_hour + span - 1`
/// arası, her iki uç dahil. `span`, çağıran tarafından `max(1, awarded_hours)`
/// olarak hesaplanıp geçirilir (fahri ziyaretler dahil, tek saatten kısa blok
/// olmaz). Tek saatlik blok (`span <= 1`) aralık göstermez, yalnızca tek saat
/// yazılır — `4-4. Saat` çirkin ve yanıltıcıdır.
fn format_visit_schedule(day: i64, hour: i64, span: i64) -> String {
    let effective_span = span.max(1);
    if effective_span <= 1 {
        format!("{} / {}. Saat", day_name(day), hour)
    } else {
        let end_hour = hour + effective_span - 1;
        format!("{} / {}-{}. Saat", day_name(day), hour, end_hour)
    }
}

fn assignment_engine() -> &'static TypstEngine<TypstTemplateMainFile> {
    static ENGINE: OnceLock<TypstEngine<TypstTemplateMainFile>> = OnceLock::new();
    ENGINE.get_or_init(|| {
        TypstEngine::builder()
            .main_file(ASSIGNMENT_SHEET_TEMPLATE)
            .fonts([FONT_REGULAR, FONT_BOLD])
            .build()
    })
}

fn visit_list_engine() -> &'static TypstEngine<TypstTemplateMainFile> {
    static ENGINE: OnceLock<TypstEngine<TypstTemplateMainFile>> = OnceLock::new();
    ENGINE.get_or_init(|| {
        TypstEngine::builder()
            .main_file(VISIT_LIST_TEMPLATE)
            .fonts([FONT_REGULAR, FONT_BOLD])
            .build()
    })
}

/// Verilen Typst motorunu, veriyi tek bir JSON dizgisi olarak `sys.inputs.data`
/// üzerinden aktararak derler ve PDF baytlarını döner.
pub(crate) fn render_pdf<T: Serialize>(
    engine: &TypstEngine<TypstTemplateMainFile>,
    data: &T,
) -> AppResult<Vec<u8>> {
    let json = serde_json::to_string(data)
        .map_err(|e| AppError::Io(format!("Rapor verisi JSON'a çevrilemedi: {e}")))?;

    let mut inputs = Dict::new();
    inputs.insert("data".into(), json.into_value());

    let doc: PagedDocument = engine
        .compile_with_input(inputs)
        .output
        .map_err(|e| AppError::Io(format!("Typst şablonu derlenemedi: {e}")))?;

    let pdf = typst_pdf::pdf(&doc, &Default::default()).map_err(|diagnostics| {
        let message = diagnostics
            .iter()
            .map(|d| d.message.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        AppError::Io(format!("PDF oluşturulamadı: {message}"))
    })?;

    Ok(pdf)
}

/// Bir satırın kaynaklandığı ortak veri: her iki rapor da aynı birleştirmeyi
/// kullanır, yalnızca hangi alanları bastıkları farklıdır.
struct RowData {
    company_name: String,
    address: String,
    phone: String,
    contact: String,
    student_count: usize,
    students: String,
    visit_schedule: String,
    hours: i64,
    is_forced: bool,
    is_honorary: bool,
}

/// Rapor üretimi için tek seferde okunan, id'ye göre indekslenmiş veri.
struct ReportContext {
    teachers: HashMap<i64, Teacher>,
    companies: HashMap<i64, Company>,
    hours: HashMap<i64, CompanyTermHours>,
    students_by_company: HashMap<i64, Vec<Student>>,
}

async fn load_context(pool: &SqlitePool, term: &str) -> AppResult<ReportContext> {
    let teachers = teachers::list(pool)
        .await?
        .into_iter()
        .map(|t| (t.id, t))
        .collect();

    let companies = companies::list(pool)
        .await?
        .into_iter()
        .map(|c| (c.id, c))
        .collect();

    let hours = company_hours::list(pool, term)
        .await?
        .into_iter()
        .map(|h| (h.company_id, h))
        .collect();

    let mut students_by_company: HashMap<i64, Vec<Student>> = HashMap::new();
    for student in students::list_by_term(pool, term).await? {
        if let Some(company_id) = student.company_id {
            students_by_company.entry(company_id).or_default().push(student);
        }
    }

    Ok(ReportContext {
        teachers,
        companies,
        hours,
        students_by_company,
    })
}

/// Bir atamayı, iki raporun da temel aldığı ortak satır verisine çevirir.
///
/// Saat, atamadan değil işletmenin dönemlik takdirinden (`company_term_hours`)
/// gelir; atama yalnızca ziyaretin ne zaman yapılacağını taşır.
fn build_row(ctx: &ReportContext, assignment: &Assignment) -> RowData {
    let company = ctx.companies.get(&assignment.company_id);
    let hours_row = ctx.hours.get(&assignment.company_id);
    let students = ctx
        .students_by_company
        .get(&assignment.company_id)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    let student_names = students
        .iter()
        .map(|s| format!("{} {}", s.first_name, s.last_name))
        .collect::<Vec<_>>()
        .join(", ");

    let contact = company
        .map(|c| format!("{} {}", c.contact_first_name, c.contact_last_name).trim().to_string())
        .unwrap_or_default();

    let awarded_hours = hours_row.map(|h| h.awarded_hours).unwrap_or(0);
    let span = awarded_hours.max(1);

    RowData {
        company_name: company.map(|c| c.name.clone()).unwrap_or_default(),
        address: company.map(|c| c.address_text.clone()).unwrap_or_default(),
        phone: company.map(|c| c.phone.clone()).unwrap_or_default(),
        contact,
        student_count: students.len(),
        students: student_names,
        visit_schedule: format_visit_schedule(assignment.visit_day, assignment.visit_hour, span),
        hours: awarded_hours,
        is_forced: assignment.is_forced != 0,
        is_honorary: hours_row.map(|h| h.is_honorary != 0).unwrap_or(false),
    }
}

/// Atamaları öğretmene göre gruplar; öğretmen sırası, öğretmenin soyadı ve
/// adına göre (Türkçe belgelerde beklenen sıra) belirlenir.
fn group_by_teacher<'a>(
    ctx: &ReportContext,
    assignments: &'a [Assignment],
) -> Vec<(i64, Vec<&'a Assignment>)> {
    let mut grouped: HashMap<i64, Vec<&Assignment>> = HashMap::new();
    for assignment in assignments {
        grouped.entry(assignment.teacher_id).or_default().push(assignment);
    }

    let mut teacher_ids: Vec<i64> = grouped.keys().copied().collect();
    teacher_ids.sort_by(|a, b| {
        let sort_key = |id: &i64| {
            ctx.teachers
                .get(id)
                .map(|t| (t.last_name.clone(), t.first_name.clone()))
                .unwrap_or_default()
        };
        sort_key(a).cmp(&sort_key(b))
    });

    teacher_ids
        .into_iter()
        .map(|id| {
            let rows = grouped.remove(&id).unwrap_or_default();
            (id, rows)
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssignmentRow {
    company_name: String,
    address: String,
    student_count: usize,
    students: String,
    visit_schedule: String,
    hours: i64,
    is_forced: bool,
    is_honorary: bool,
}

impl From<RowData> for AssignmentRow {
    fn from(row: RowData) -> Self {
        Self {
            company_name: row.company_name,
            address: row.address,
            student_count: row.student_count,
            students: row.students,
            visit_schedule: row.visit_schedule,
            hours: row.hours,
            is_forced: row.is_forced,
            is_honorary: row.is_honorary,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssignmentTeacherGroup {
    name: String,
    rows: Vec<AssignmentRow>,
    subtotal_hours: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssignmentSheetData {
    school_name: String,
    academic_year: String,
    printed_at: String,
    has_forced_rows: bool,
    teachers: Vec<AssignmentTeacherGroup>,
    grand_total_hours: i64,
}

/// Koordinatör Görevlendirme Çizelgesi'ni üretir (MADDE 15/2: okul müdürlüğünce
/// hazırlanıp millî eğitim müdürlüğünce onaylanacak program).
pub async fn build_assignment_sheet(pool: &SqlitePool, term: &str) -> AppResult<Vec<u8>> {
    let school_name = settings::get(pool, "school_name").await?.unwrap_or_default();
    let ctx = load_context(pool, term).await?;
    let assignment_list = assignments::list(pool, term).await?;

    let mut has_forced_rows = false;
    let mut grand_total_hours = 0i64;

    let teacher_groups = group_by_teacher(&ctx, &assignment_list)
        .into_iter()
        .filter_map(|(teacher_id, rows)| {
            let teacher = ctx.teachers.get(&teacher_id)?;
            let rows: Vec<AssignmentRow> = rows
                .into_iter()
                .map(|a| build_row(&ctx, a))
                .inspect(|row| {
                    if row.is_forced {
                        has_forced_rows = true;
                    }
                })
                .map(AssignmentRow::from)
                .collect();
            let subtotal_hours = rows.iter().map(|r| r.hours).sum();
            grand_total_hours += subtotal_hours;

            Some(AssignmentTeacherGroup {
                name: format!("{} {}", teacher.first_name, teacher.last_name),
                rows,
                subtotal_hours,
            })
        })
        .collect();

    let data = AssignmentSheetData {
        school_name,
        academic_year: term.to_string(),
        printed_at: chrono::Local::now().format("%d.%m.%Y").to_string(),
        has_forced_rows,
        teachers: teacher_groups,
        grand_total_hours,
    };

    render_pdf(assignment_engine(), &data)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VisitRow {
    company_name: String,
    address: String,
    phone: String,
    contact: String,
    students: String,
    visit_schedule: String,
    hours: i64,
    is_honorary: bool,
}

impl From<RowData> for VisitRow {
    fn from(row: RowData) -> Self {
        Self {
            company_name: row.company_name,
            address: row.address,
            phone: row.phone,
            contact: row.contact,
            students: row.students,
            visit_schedule: row.visit_schedule,
            hours: row.hours,
            is_honorary: row.is_honorary,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VisitTeacherPage {
    name: String,
    rows: Vec<VisitRow>,
    total_hours: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VisitListData {
    school_name: String,
    academic_year: String,
    teachers: Vec<VisitTeacherPage>,
}

/// Öğretmen başına bir sayfa olacak şekilde Ziyaret Listeleri'ni üretir.
/// Satırlar gün, ardından saat sırasına göre dizilir.
pub async fn build_visit_lists(pool: &SqlitePool, term: &str) -> AppResult<Vec<u8>> {
    let school_name = settings::get(pool, "school_name").await?.unwrap_or_default();
    let ctx = load_context(pool, term).await?;
    let assignment_list = assignments::list(pool, term).await?;

    let teacher_pages = group_by_teacher(&ctx, &assignment_list)
        .into_iter()
        .filter_map(|(teacher_id, mut rows)| {
            let teacher = ctx.teachers.get(&teacher_id)?;
            // Gün, ardından saat sırasına göre dizilir.
            rows.sort_by_key(|a| (a.visit_day, a.visit_hour));

            let rows: Vec<VisitRow> = rows
                .into_iter()
                .map(|a| VisitRow::from(build_row(&ctx, a)))
                .collect();
            let total_hours = rows.iter().map(|r| r.hours).sum();

            Some(VisitTeacherPage {
                name: format!("{} {}", teacher.first_name, teacher.last_name),
                rows,
                total_hours,
            })
        })
        .collect();

    let data = VisitListData {
        school_name,
        academic_year: term.to_string(),
        teachers: teacher_pages,
    };

    render_pdf(visit_list_engine(), &data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::company_hours::HoursInput;
    use crate::db::init_pool;
    use crate::domain::models::{NewCompany, NewStudent, NewTeacher};

    const TERM: &str = "2026-2027/1";

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    async fn seed_teacher(pool: &SqlitePool, last_name: &str) -> i64 {
        teachers::create(
            pool,
            &NewTeacher {
                first_name: "Test".into(),
                last_name: last_name.into(),
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

    async fn seed_company(pool: &SqlitePool, name: &str) -> i64 {
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
                one_way_distance_km: Some(6.8),
                notes: String::new(),
            },
        )
        .await
        .unwrap()
        .id
    }

    async fn seed_hours(pool: &SqlitePool, company_id: i64, awarded: i64) {
        company_hours::upsert(
            pool,
            TERM,
            &HoursInput {
                company_id,
                max_hours_snapshot: 12,
                awarded_hours: awarded,
                is_honorary: false,
                is_locked: false,
                notes: String::new(),
            },
        )
        .await
        .unwrap();
    }

    async fn seed_student(pool: &SqlitePool, company_id: i64, first: &str, last: &str) {
        students::create(
            pool,
            &NewStudent {
                first_name: first.into(),
                last_name: last.into(),
                student_no: None,
                grade: "12/C".into(),
                branch: "Elektronik Haberleşme".into(),
                company_id: Some(company_id),
                submitted_at: Some("2026-09-11".into()),
                term: TERM.into(),
            },
        )
        .await
        .unwrap();
    }

    /// Boş bir dönem (hiç öğretmen, işletme veya atama yok) bile geçerli bir
    /// PDF üretmeli; belge içeriksiz olsa dahi Typst en az bir sayfa basar.
    #[tokio::test]
    async fn empty_term_produces_a_valid_pdf() {
        let (_dir, pool) = test_pool().await;

        let pdf = build_assignment_sheet(&pool, TERM).await.unwrap();
        assert!(pdf.starts_with(b"%PDF"), "PDF imzasıyla başlamalı");

        let pdf2 = build_visit_lists(&pool, TERM).await.unwrap();
        assert!(pdf2.starts_with(b"%PDF"), "PDF imzasıyla başlamalı");
    }

    /// Tek bir öğretmen, işletme, öğrenci ve atama; saat de işletmenin
    /// dönemlik takdirinden gelmeli. Dolu belge boş belgeden büyük olmalı.
    #[tokio::test]
    async fn seeded_term_produces_a_larger_valid_pdf() {
        let (_dir, pool) = test_pool().await;

        let empty_pdf = build_assignment_sheet(&pool, TERM).await.unwrap();

        let teacher_id = seed_teacher(&pool, "Yılmaz").await;
        let company_id = seed_company(&pool, "Test İşletme A").await;
        seed_student(&pool, company_id, "Ahmet", "Öztürk").await;
        seed_hours(&pool, company_id, 6).await;
        assignments::assign(
            &pool,
            TERM,
            &assignments::NewAssignment {
                teacher_id,
                company_id,
                visit_day: 2,
                visit_hour: 3,
                is_forced: false,
                force_reason: None,
            },
        )
        .await
        .unwrap();

        let filled_pdf = build_assignment_sheet(&pool, TERM).await.unwrap();
        assert!(filled_pdf.starts_with(b"%PDF"));
        assert!(
            filled_pdf.len() > empty_pdf.len(),
            "dolu çizelge boş çizelgeden büyük olmalı"
        );

        let visit_pdf = build_visit_lists(&pool, TERM).await.unwrap();
        assert!(visit_pdf.starts_with(b"%PDF"));
    }

    /// Tek saatlik blok (span = 1): aralık gösterilmez, yalnızca tek saat yazılır.
    #[test]
    fn format_visit_schedule_prints_a_single_hour_when_span_is_one() {
        assert_eq!(format_visit_schedule(3, 4, 1), "Çarşamba / 4. Saat");
    }

    /// Çok saatlik blok (span > 1): `başlangıç-bitiş. Saat` aralığı basılır,
    /// bitiş = visit_hour + span - 1 (her iki uç dahil).
    #[test]
    fn format_visit_schedule_prints_an_hour_range_when_span_is_greater_than_one() {
        assert_eq!(format_visit_schedule(3, 4, 6), "Çarşamba / 4-9. Saat");
    }

    /// Fahri ziyaret gibi span <= 0 durumları savunmacı biçimde tek saate
    /// zorlanır; asla `4-3. Saat` gibi geçersiz bir aralık üretilmez.
    #[test]
    fn format_visit_schedule_treats_zero_or_negative_span_as_a_single_hour() {
        assert_eq!(format_visit_schedule(1, 2, 0), "Pazartesi / 2. Saat");
        assert_eq!(format_visit_schedule(1, 2, -3), "Pazartesi / 2. Saat");
    }

    /// Uçtan uca: 6 saat takdir edilmiş bir atama, 4. saatten başlıyorsa
    /// çizelgede "4-9. Saat" olarak görünmeli (eskiden yanlışlıkla "4. Saat").
    #[tokio::test]
    async fn multi_hour_award_renders_the_full_block_as_an_hour_range() {
        let (_dir, pool) = test_pool().await;

        let teacher_id = seed_teacher(&pool, "Aydın").await;
        let company_id = seed_company(&pool, "Test İşletme D").await;
        seed_hours(&pool, company_id, 6).await;
        assignments::assign(
            &pool,
            TERM,
            &assignments::NewAssignment {
                teacher_id,
                company_id,
                visit_day: 3,
                visit_hour: 4,
                is_forced: false,
                force_reason: None,
            },
        )
        .await
        .unwrap();

        let ctx = load_context(&pool, TERM).await.unwrap();
        let assignment_list = assignments::list(&pool, TERM).await.unwrap();
        let groups = group_by_teacher(&ctx, &assignment_list);
        let (_, rows) = groups.first().expect("bir grup olmalı");
        let row = build_row(&ctx, rows[0]);

        assert_eq!(row.visit_schedule, "Çarşamba / 4-9. Saat");
    }

    /// Fahri ziyaret (awarded_hours = 0) tam 1 saat gibi gösterilmeli; aralık
    /// yazılmamalı.
    #[tokio::test]
    async fn honorary_visit_renders_as_a_single_hour_not_a_range() {
        let (_dir, pool) = test_pool().await;

        let teacher_id = seed_teacher(&pool, "Koç").await;
        let company_id = seed_company(&pool, "Test İşletme E").await;
        company_hours::upsert(
            &pool,
            TERM,
            &HoursInput {
                company_id,
                max_hours_snapshot: 8,
                awarded_hours: 8,
                is_honorary: true,
                is_locked: false,
                notes: String::new(),
            },
        )
        .await
        .unwrap();
        assignments::assign(
            &pool,
            TERM,
            &assignments::NewAssignment {
                teacher_id,
                company_id,
                visit_day: 4,
                visit_hour: 2,
                is_forced: false,
                force_reason: None,
            },
        )
        .await
        .unwrap();

        let ctx = load_context(&pool, TERM).await.unwrap();
        let assignment_list = assignments::list(&pool, TERM).await.unwrap();
        let groups = group_by_teacher(&ctx, &assignment_list);
        let (_, rows) = groups.first().expect("bir grup olmalı");
        let row = build_row(&ctx, rows[0]);

        assert_eq!(row.visit_schedule, "Perşembe / 2. Saat");
    }

    /// Gün numarası → Türkçe gün adı eşlemesi (MADDE'de kullanılan 1..5 sırası).
    #[test]
    fn day_name_maps_numbers_to_turkish_day_names() {
        assert_eq!(day_name(1), "Pazartesi");
        assert_eq!(day_name(2), "Salı");
        assert_eq!(day_name(3), "Çarşamba");
        assert_eq!(day_name(4), "Perşembe");
        assert_eq!(day_name(5), "Cuma");
        // Tanınmayan numara sessizce yutulmaz, olduğu gibi görünür kalır.
        assert_eq!(day_name(9), "Gün 9");
    }

    /// Zorlanmış bir atama, uyarı bandını tetikleyen bayrağı ayarlamalı.
    #[tokio::test]
    async fn forced_assignment_sets_the_has_forced_rows_flag() {
        let (_dir, pool) = test_pool().await;

        let teacher_id = seed_teacher(&pool, "Demir").await;
        let company_id = seed_company(&pool, "Test İşletme B").await;
        seed_hours(&pool, company_id, 4).await;
        assignments::assign(
            &pool,
            TERM,
            &assignments::NewAssignment {
                teacher_id,
                company_id,
                visit_day: 1,
                visit_hour: 1,
                is_forced: true,
                force_reason: Some("Ulaşım zorunluluğu".into()),
            },
        )
        .await
        .unwrap();

        let ctx = load_context(&pool, TERM).await.unwrap();
        let assignment_list = assignments::list(&pool, TERM).await.unwrap();
        let groups = group_by_teacher(&ctx, &assignment_list);
        let (_, rows) = groups.first().expect("bir grup olmalı");
        let row = build_row(&ctx, rows[0]);

        assert!(row.is_forced, "zorlanmış atama işaretlenmeli");

        // Uçtan uca: üretilen PDF de sorunsuz derlenmeli (uyarı bandı devrede).
        let pdf = build_assignment_sheet(&pool, TERM).await.unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    /// Fahri ziyaret satırı saat basmamalı; şablon "Fahri" göstermeli.
    /// Bu test veri tarafını doğrular (saat 0'a zorlanır, bayrak taşınır).
    #[tokio::test]
    async fn honorary_row_carries_zero_hours_and_the_honorary_flag() {
        let (_dir, pool) = test_pool().await;

        let teacher_id = seed_teacher(&pool, "Kaya").await;
        let company_id = seed_company(&pool, "Test İşletme C").await;
        company_hours::upsert(
            &pool,
            TERM,
            &HoursInput {
                company_id,
                max_hours_snapshot: 8,
                awarded_hours: 8,
                is_honorary: true,
                is_locked: false,
                notes: String::new(),
            },
        )
        .await
        .unwrap();
        assignments::assign(
            &pool,
            TERM,
            &assignments::NewAssignment {
                teacher_id,
                company_id,
                visit_day: 4,
                visit_hour: 2,
                is_forced: false,
                force_reason: None,
            },
        )
        .await
        .unwrap();

        let ctx = load_context(&pool, TERM).await.unwrap();
        let assignment_list = assignments::list(&pool, TERM).await.unwrap();
        let groups = group_by_teacher(&ctx, &assignment_list);
        let (_, rows) = groups.first().expect("bir grup olmalı");
        let row = build_row(&ctx, rows[0]);

        assert!(row.is_honorary);
        assert_eq!(row.hours, 0, "fahri satırda saat 0'a zorlanır");
    }

    /// Türkçe alfabenin tamamı (küçük ve BÜYÜK: ğ/Ğ, ı/I, i/İ, ş/Ş, ö/Ö, ç/Ç,
    /// ü/Ü) ile şapkalı â, Typst'e verilip PDF'e dönüşebilmeli.
    ///
    /// DİKKAT: Bu test glif KAPSAMINI kanıtlamaz. Typst eksik glifi hata değil
    /// uyarı sayar ve `render_pdf` uyarıları kullanmaz; eksik glif sessizce
    /// tofu (□) olarak basılırdı. Kapsamı `fonts_cover_every_turkish_character`
    /// doğrudan fontun cmap tablosundan doğrular.
    #[tokio::test]
    async fn renders_every_turkish_character_including_capitals() {
        let (_dir, pool) = test_pool().await;

        settings::set(&pool, "school_name", "Şükrü Saracoğlu Mesleki ve Teknik Anadolu Lisesi")
            .await
            .unwrap();
        settings::set(&pool, "active_term", TERM).await.unwrap();

        // Büyük Ğ ve İ en sık düşen gliflerdir; adlarda açıkça geçmeleri şart.
        let teacher_id = seed_teacher(&pool, "ÇAĞIL İŞIKÖZÜ").await;
        let company_id = seed_company(&pool, "ÇELİK ÖĞÜT SANAYİ A.Ş. — Gıda ve Şişeleme").await;
        seed_student(&pool, company_id, "Gökçe", "Ünlü").await;
        seed_hours(&pool, company_id, 6).await;
        assignments::assign(
            &pool,
            TERM,
            &assignments::NewAssignment {
                teacher_id,
                company_id,
                visit_day: 3,
                visit_hour: 4,
                is_forced: true,
                force_reason: Some("Güzergâh zorunluluğu".into()),
            },
        )
        .await
        .unwrap();

        let pdf = build_assignment_sheet(&pool, TERM).await.unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        // Boş/uç bir PDF değil: gerçekten sayfa çizilmiş olmalı.
        assert!(pdf.len() > 2_000, "PDF beklenenden küçük: {} bayt", pdf.len());
    }

    /// Gömülü iki fontun da Türkçe'ye özgü her kod noktası için gerçek bir glifi
    /// olmalı. Eksik glif render sırasında sessizce tofu (□) basılmasına yol
    /// açar; tek kesin kontrol fontun cmap tablosudur.
    #[test]
    fn fonts_cover_every_turkish_character() {
        // Türkçe'ye özgü harfler + rapor metinlerinde geçen şapkalı sesliler.
        const TURKISH: &str = "çÇğĞıIiİöÖşŞüÜâÂîÎûÛ";

        for (name, bytes) in [("DejaVuSans", FONT_REGULAR), ("DejaVuSans-Bold", FONT_BOLD)] {
            let face = ttf_parser::Face::parse(bytes, 0)
                .unwrap_or_else(|e| panic!("{name} ayrıştırılamadı: {e}"));

            let missing: Vec<char> = TURKISH
                .chars()
                .filter(|c| face.glyph_index(*c).is_none())
                .collect();

            assert!(
                missing.is_empty(),
                "{name} fontunda şu karakterlerin glifi yok: {missing:?}"
            );
        }
    }
}
