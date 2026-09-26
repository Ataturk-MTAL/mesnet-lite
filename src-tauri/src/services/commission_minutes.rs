//! İşletme Belirleme Komisyon Tutanağı'nın ortak, saf veri katmanı.
//!
//! PDF (`commission_minutes_pdf`) ve Excel (`commission_minutes_xlsx`) çıktıları
//! aynı `MinutesData`'yı okur; başlık metni, sıralama, "Fahri" kuralı gibi
//! hiçbir karar iki yerde yazılmaz. Böylece iki dosya birbirinden sessizce
//! ayrışamaz.
//!
//! Belge, okulun kendi Excel şablonunun (`işletme belirleme komisyon
//! tutanağı.xlsx`) birebir karşılığıdır. Şablondaki sabit metinler — yazım
//! hataları ve madde numaraları dahil — kullanıcının resmî metnidir; burada
//! düzeltilmez, olduğu gibi taşınır.

use crate::db::assignments::{self, Assignment};
use crate::db::company_hours::{self, CompanyTermHours};
use crate::db::read_at::ReadAt;
use crate::db::teachers::TeacherWithLoadAsOf;
use crate::db::{companies, settings, students, teachers, teaching_load};
use crate::domain::models::{ChiefType, Company, Student, Teacher};
use crate::error::{AppError, AppResult};
use crate::services::pdf_report::day_name;
use serde::Serialize;
use sqlx::SqlitePool;
use std::collections::{BTreeMap, HashMap};

/// Müdür adı ayarının anahtarı (isteğe bağlı metin).
pub const PRINCIPAL_NAME_KEY: &str = "principal_name";
/// Alan adı ayarının anahtarı (isteğe bağlı metin).
pub const FIELD_NAME_KEY: &str = "field_name";
const SCHOOL_NAME_KEY: &str = "school_name";

/// İsteğe bağlı iki ayar için üst sınır. Başlık tek satırlık bir hücreye/kutuya
/// basılır; çok uzun bir değer şablonu sessizce taşırırdı.
const OPTIONAL_TEXT_MAX_CHARS: usize = 100;

/// Alan adı boşken başlıkta ("............ ALANI") bırakılan noktalı yer.
/// Uzunluk şablondaki hücrenin (12 nokta) aynısıdır.
const TITLE_FIELD_PLACEHOLDER: &str = "............";
/// Alan adı boşken açıklama paragrafındaki noktalı yer (şablonda 21 nokta).
const INTRO_FIELD_PLACEHOLDER: &str = ".....................";
/// Müdür adı boşken onay bloğunda bırakılan yer.
const PRINCIPAL_PLACEHOLDER: &str = "…………………";

const COLUMN_HEADERS: [&str; 7] = [
    "Sıra No",
    "İşletmenin Adı",
    "ÖĞRENCİ \nADI SOYADI",
    "OKULA UZAKLIK(KM)",
    "KOORDİNATÖR ÖĞRETMEN",
    "GÖREV GÜNÜ",
    "ÜCRET",
];

/// Şablonun açıklama paragrafı. Yazım ("öğremenlerin") ve madde numarası (142)
/// kullanıcının şablonundandır; bilerek düzeltilmemiştir. `{field}` yeri alan
/// adı ile doldurulur.
const INTRO_TEMPLATE: &str = "{field} alanındaki öğrencilerimizin meslek eğitimi yapabilecekleri \
işletmeleri belirlemeyle ilgili olarak İl Mesleki Eğitim Kurulunca gönderilen listeler ve \
okulumuza yapılan yazılı başvurular, ortaöğretim kurumlar yönetmeliğinin 142. maddesine göre \
değerlendirilmiş, aşağıda adları yazılı işletmelerde, işletmelerde mesleki eğitim yaptırılması \
için isimleri yazılı koordinatör öğremenlerin gönderilmesine karar verilmiştir.";

/// Şablondaki açıklama satırı, "maddesinine" yazımı dahil olduğu gibi.
const NOTE_TEXT: &str = "AÇIKLAMA : Komisyon, Ortaöğretim Kurumlar Yönetmeliğinin 140. \
maddesinine istinaden oluşturulmuştur.";

const CLOSING_LINE: &str = "Olurlarınıza arz ederiz.";
/// Onay bloğunun sabit ilk satırı.
const APPROVAL_CONFIRMATION_LINE: &str = "Uygundur";
/// Onay bloğunun sabit son satırı.
const PRINCIPAL_TITLE_LINE: &str = "Okul Müdürü";
const CHIEF_SIGNATURE_LABEL: &str = "Alan Şefi\nİmza";
const TEACHERS_SIGNATURE_LABEL: &str = "Alan Öğretmenleri İmza";

/// Atölye/Laboratuvar şefinin unvanı (MADDE 6/4: haftada 6 saat).
const WORKSHOP_LAB_TITLE: &str = "Atölye/Laboratuvar Şefi";
/// Hiçbir şeflik taşımayan öğretmenin unvanı. Kullanıcının açık isteği:
/// "şef değil" değil, sade "Öğretmen" yazılır.
const TEACHER_TITLE: &str = "Öğretmen";

/// Tablodaki bir öğrenci satırı. D–G sütunları işletme düzeyindedir; aynı
/// işletmenin bütün satırlarında tekrarlanır, çıktılar yalnızca grubun ilk
/// satırındaki değeri birleşik hücreye yazar.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MinutesRow {
    /// Öğrenci satırı başına 1'den giden sıra numarası.
    pub index: usize,
    pub company_name: String,
    /// Öğrencisiz ama atanmış işletmede boş.
    pub student_name: String,
    /// Okula GİDİŞ-DÖNÜŞ km, bir ondalığa yuvarlanmış; konum yoksa None.
    pub round_trip_km: Option<f64>,
    /// `round_trip_km`'in basılacak hâli ("17.2"); konum yoksa boş.
    pub distance_label: String,
    /// "Ad SOYAD"; atanmamışsa boş.
    pub teacher: String,
    /// BÜYÜK HARF gün adı ("CUMA"); atanmamışsa boş.
    pub day: String,
    /// Takdir edilen haftalık saat veya "Fahri"; takdir yoksa boş.
    pub hours_label: String,
}

/// `rows` içindeki ardışık bir satır aralığı; dikey birleşik hücrenin sınırıdır.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MinutesGroup {
    pub start: usize,
    pub len: usize,
}

/// İmza şeridindeki bir alan öğretmeni: adı ve MADDE 6/4 unvanı.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureTeacher {
    pub name: String,
    pub title: String,
}

/// Her iki çıktının da bastığı tüm metin ve tablo.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MinutesData {
    pub year_line: String,
    pub school_line: String,
    pub field_line: String,
    pub addressee_line: String,
    pub intro: String,
    pub closing_line: String,
    pub chief_label: String,
    pub teachers_label: String,
    /// Alan şefinin ("Ad SOYAD") adı; alan şefi yoksa boş. İmza şeridinde
    /// `chief_label` başlığının altına basılır.
    pub chief_name: String,
    /// Alan şefi DIŞINDAKİ tüm aktif öğretmenler, soyada göre Türkçe sırayla.
    /// İmza şeridinde `teachers_label` başlığının altına basılır.
    pub field_teachers: Vec<SignatureTeacher>,
    pub column_headers: Vec<String>,
    pub rows: Vec<MinutesRow>,
    /// İşletme başına satır aralıkları: D, F, G sütunlarındaki birleşik hücreler.
    pub groups: Vec<MinutesGroup>,
    /// Aynı öğretmenin ardışık işletmelerini kapsayan satır aralıkları: E
    /// sütunundaki birleşik hücreler. Atanmamış işletmeler hiçbirine girmez.
    pub teacher_groups: Vec<MinutesGroup>,
    /// Onay bloğunun "Uygundur" satırı; sabit metin, hiçbir ayardan gelmez.
    pub approval_line: String,
    /// Onay bloğunun tarih satırı; yıl dönemin başlangıç yılından gelir (bkz.
    /// `parse_academic_year`), gün/ay resmî belge elle doldurulacağından
    /// bugün de olduğu gibi noktalı bırakılır.
    pub approval_date_line: String,
    /// Müdür adı; ayarlardan (`principal_name`) boşsa yer tutucu basılır.
    /// Onay bloğunda tek KALIN basılan alan budur (kullanıcı isteği: imza
    /// şeridindeki öğretmen adlarıyla tutarlı olsun); bu yüzden diğer onay
    /// satırlarından ayrı bir alana çıkarılmıştır.
    pub principal_name: String,
    /// Onay bloğunun unvan satırı; sabit metin.
    pub principal_title_line: String,
    pub note_text: String,
}

/// Türkçe kurallarıyla büyük harfe çevirir: `i → İ`, `ı → I`.
///
/// `str::to_uppercase` yerel ayardan bağımsızdır ve `i → I` yapar; "MESLEKI"
/// gibi yanlış bir başlık üretirdi. Diğer harfler (ç, ğ, ö, ş, ü) standart
/// dönüşümde zaten doğrudur.
pub fn turkish_uppercase(text: &str) -> String {
    text.chars()
        .flat_map(|c| match c {
            'i' => vec!['İ'],
            'ı' => vec!['I'],
            other => other.to_uppercase().collect(),
        })
        .collect()
}

/// Türkçe alfabe sırasıyla karşılaştırılabilir anahtar.
///
/// Veritabanındaki `COLLATE NOCASE` yalnızca ASCII'yi katlar; Ç, Ğ, İ, Ö, Ş, Ü
/// harfleri Z'den sonraya düşerdi ve "Şirin" listenin sonuna giderdi. Bu belge
/// resmî bir liste olduğundan alfabetik sıra Türkçe olmalıdır.
pub fn turkish_sort_key(text: &str) -> Vec<u32> {
    // Türkçe'de olmayan q, w, x Latin komşularının yanına yerleştirilir.
    const ALPHABET: &str = "abcçdefgğhıijklmnoöpqrsştuüvwxyz";
    const LETTER_BASE: u32 = 0x1000;
    const OTHER_BASE: u32 = 0x2000;

    text.chars()
        .map(|c| match c {
            'I' => 'ı',
            'İ' => 'i',
            other => other.to_lowercase().next().unwrap_or(other),
        })
        .map(|c| match ALPHABET.chars().position(|letter| letter == c) {
            Some(position) => LETTER_BASE + position as u32,
            // Boşluk, rakam ve noktalama harflerden önce, tanınmayan
            // (şapkalı vb.) harfler ise sona sıralanır.
            None if c.is_alphabetic() => OTHER_BASE + c as u32,
            None => c as u32,
        })
        .collect()
}

/// Dönem dizgisinden ("2026-2027/1") eğitim-öğretim yılı etiketini ve başlangıç
/// yılını çıkarır. Tarih yeri ("…./…/2026") başlangıç yılını kullanır.
fn parse_academic_year(term: &str) -> AppResult<(String, i32)> {
    let invalid = || {
        AppError::Validation(format!(
            "Aktif dönem \"{term}\" tutanak için okunamadı. Ayarlar'dan YYYY-YYYY/N biçiminde \
             (ör. 2026-2027/1) bir aktif dönem seçin."
        ))
    };

    let years = term.split('/').next().unwrap_or_default();
    let (first, second) = years.split_once('-').ok_or_else(invalid)?;
    let first_year: i32 = first.parse().map_err(|_| invalid())?;
    let second_year: i32 = second.parse().map_err(|_| invalid())?;

    if first.len() != 4 || second.len() != 4 || second_year != first_year + 1 {
        return Err(invalid());
    }
    Ok((years.to_string(), first_year))
}

/// Tutanak ayarlarını kaydetmeden önce doğrular. Değerler başlığa ve onay
/// bloğuna basıldığından satır sonu/kontrol karakteri ve aşırı uzunluk
/// reddedilir; aksi hâlde belge düzeni sessizce bozulurdu.
pub fn validate_minutes_settings(entries: &BTreeMap<String, String>) -> AppResult<()> {
    for (key, label) in [
        (PRINCIPAL_NAME_KEY, "Müdür adı"),
        (FIELD_NAME_KEY, "Alan adı"),
    ] {
        let Some(value) = entries.get(key) else {
            continue;
        };
        if value.chars().count() > OPTIONAL_TEXT_MAX_CHARS {
            return Err(AppError::Validation(format!(
                "{label} en fazla {OPTIONAL_TEXT_MAX_CHARS} karakter olabilir. Lütfen kısaltın."
            )));
        }
        if value.chars().any(char::is_control) {
            return Err(AppError::Validation(format!(
                "{label} satır sonu veya denetim karakteri içeremez. Lütfen tek satır yazın."
            )));
        }
    }
    Ok(())
}

/// Bir ayarı okur; anahtar yoksa veya yalnızca boşluksa boş metin döner.
async fn optional_setting(pool: &SqlitePool, key: &str) -> AppResult<String> {
    Ok(settings::get(pool, key)
        .await?
        .map(|value| value.trim().to_string())
        .unwrap_or_default())
}

/// Dönemin tutanak için gereken bütün kayıtları, id'ye göre indekslenmiş.
struct Source {
    /// Türkçe alfabe sırasına dizilmiş işletmeler.
    companies: Vec<Company>,
    teachers: HashMap<i64, Teacher>,
    hours: HashMap<i64, CompanyTermHours>,
    /// `assignments` tablosu (company_id, term) için tekildir.
    assignments: HashMap<i64, Assignment>,
    /// Soyada, sonra ada göre Türkçe sırada.
    students: HashMap<i64, Vec<Student>>,
}

async fn load_source(pool: &SqlitePool, term: &str) -> AppResult<Source> {
    // `list_all`: tutanak resmî bir belgedir, dönem ortasında pasifleşen bir
    // işletme (`ordered_companies` zaten yalnız o dönem öğrencisi/ataması
    // olanları seçiyor) tutanaktan sessizce düşmemeli.
    let mut companies = companies::list_all(pool).await?;
    companies.sort_by_cached_key(|c| (turkish_sort_key(&c.name), c.id));

    let teachers = teachers::list(pool)
        .await?
        .into_iter()
        .map(|t| (t.id, t))
        .collect();
    // Rapor her zaman GÜNCEL duruma göre üretilir (`ReadAt::Latest`); tarihe
    // göre komisyon tutanağı bu işin kapsamı dışındadır (spec §6, plan R5d).
    let hours = company_hours::list(pool, term, &ReadAt::Latest)
        .await?
        .into_iter()
        .map(|h| (h.company_id, h))
        .collect();
    let assignments = assignments::list(pool, term, &ReadAt::Latest)
        .await?
        .into_iter()
        .map(|a| (a.company_id, a))
        .collect();

    let mut students: HashMap<i64, Vec<Student>> = HashMap::new();
    for student in students::list_by_term(pool, term, &ReadAt::Latest).await? {
        if let Some(company_id) = student.company_id {
            students.entry(company_id).or_default().push(student);
        }
    }
    for list in students.values_mut() {
        list.sort_by_cached_key(|s| {
            (
                turkish_sort_key(&s.last_name),
                turkish_sort_key(&s.first_name),
                s.id,
            )
        });
    }

    Ok(Source {
        companies,
        teachers,
        hours,
        assignments,
        students,
    })
}

/// "Ad SOYAD" biçiminde basılabilir öğretmen adı; soyad her yerde (tablo
/// hücresi, imza şeridi) BÜYÜK HARFtir — resmî belge biçimi tekildir, bu
/// yüzden format tek yerde yazılır.
fn teacher_display_name(t: &Teacher) -> String {
    format!("{} {}", t.first_name, turkish_uppercase(&t.last_name))
}

/// Bir işletmenin D–G sütunlarına giren, grup düzeyindeki hücreler.
struct GroupCells {
    round_trip_km: Option<f64>,
    distance_label: String,
    teacher: String,
    day: String,
    hours_label: String,
}

/// Her hücre kendi verisi yoksa boş kalır: atanmamış işletmenin öğretmen ve
/// günü, takdir edilmemiş işletmenin ücreti boştur; "0" veya "Pazartesi" gibi
/// uydurma bir varsayılan basılmaz.
fn group_cells(source: &Source, company: &Company) -> GroupCells {
    // Sütun gidiş-dönüş mesafedir (tek yön × 2): saat tavanı kuralları da bu
    // değere bakar (`hour_rules`), referanstaki örnekler (4.4 km → 6 saat,
    // 17.2 km → 8 saat) ancak böyle uyar. Hesap `Company::round_trip_distance_km`
    // ile ortaktır; burada yalnızca gösterim için tek ondalığa yuvarlanır.
    let round_trip_km = company
        .round_trip_distance_km()
        .map(|km| (km * 10.0).round() / 10.0);
    let assignment = source.assignments.get(&company.id);

    let teacher = assigned_teacher(source, company.id)
        .map(teacher_display_name)
        .unwrap_or_default();
    let day = assignment
        .map(|a| turkish_uppercase(&day_name(a.visit_day)))
        .unwrap_or_default();

    // Fahri işletmede ek ders ücreti doğmaz (MADDE 6/4); mevcut çıktılarla
    // aynı biçimde "Fahri" yazılır.
    let hours_label = match source.hours.get(&company.id) {
        Some(h) if h.is_honorary != 0 => "Fahri".to_string(),
        Some(h) => h.awarded_hours.to_string(),
        None => String::new(),
    };

    GroupCells {
        round_trip_km,
        distance_label: round_trip_km
            .map(|km| format!("{km:.1}"))
            .unwrap_or_default(),
        teacher,
        day,
        hours_label,
    }
}

/// İşletmenin koordinatör öğretmeni; atanmamışsa None. Atamanın öğretmeni
/// tabloda yoksa (yabancı anahtar bunu engeller) işletme atanmamış sayılır.
fn assigned_teacher(source: &Source, company_id: i64) -> Option<&Teacher> {
    let assignment = source.assignments.get(&company_id)?;
    source.teachers.get(&assignment.teacher_id)
}

/// Tutanakta yer alacak işletmeleri referansın satır sırasına dizer.
///
/// Referans şablonda koordinatör hücresi aynı öğretmenin ardışık işletmeleri
/// boyunca birleşiktir; bu yüzden önce öğretmene göre, sonra işletme adına göre
/// sıralanır. Öğretmen sırası soyad → ad'dır (resmî listelerde alışılmış sıra).
/// Atanmamış işletmeler en sona, kendi içinde ada göre dizilir. Öğrencisiz bir
/// işletme ancak atanmışsa yer alır; öğrencisiz ve atanmamışın tutanakta işi yoktur.
fn ordered_companies(source: &Source) -> Vec<&Company> {
    let mut included: Vec<&Company> = source
        .companies
        .iter()
        .filter(|c| {
            let has_students = source.students.contains_key(&c.id);
            has_students || source.assignments.contains_key(&c.id)
        })
        .collect();

    included.sort_by_cached_key(|c| {
        let teacher_key = assigned_teacher(source, c.id).map(|t| {
            (
                turkish_sort_key(&t.last_name),
                turkish_sort_key(&t.first_name),
                t.id,
            )
        });
        // `None` (atanmamış) `Some`'tan sonra gelsin diye açık bir sıra değeri.
        (
            teacher_key.is_none(),
            teacher_key,
            turkish_sort_key(&c.name),
            c.id,
        )
    });
    included
}

/// Tablonun satırları ve iki düzeydeki birleşik hücre aralıkları.
struct TableLayout {
    rows: Vec<MinutesRow>,
    groups: Vec<MinutesGroup>,
    teacher_groups: Vec<MinutesGroup>,
}

fn build_table(source: &Source) -> TableLayout {
    let mut layout = TableLayout {
        rows: Vec::new(),
        groups: Vec::new(),
        teacher_groups: Vec::new(),
    };
    let mut current_teacher: Option<i64> = None;

    for company in ordered_companies(source) {
        let students = source
            .students
            .get(&company.id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let cells = group_cells(source, company);
        let names: Vec<String> = if students.is_empty() {
            vec![String::new()]
        } else {
            students
                .iter()
                .map(|s| format!("{} {}", s.first_name, s.last_name))
                .collect()
        };

        let start = layout.rows.len();
        layout.groups.push(MinutesGroup {
            start,
            len: names.len(),
        });
        extend_teacher_groups(
            &mut layout,
            &mut current_teacher,
            source,
            company.id,
            names.len(),
        );

        for student_name in names {
            layout.rows.push(MinutesRow {
                index: layout.rows.len() + 1,
                company_name: company.name.clone(),
                student_name,
                round_trip_km: cells.round_trip_km,
                distance_label: cells.distance_label.clone(),
                teacher: cells.teacher.clone(),
                day: cells.day.clone(),
                hours_label: cells.hours_label.clone(),
            });
        }
    }
    layout
}

/// İşletmenin `len` satırını öğretmen aralıklarına işler: önceki işletmeyle aynı
/// öğretmense son aralığı uzatır, farklıysa yenisini açar, atanmamışsa hiçbir
/// aralığa girmez ve zinciri keser.
fn extend_teacher_groups(
    layout: &mut TableLayout,
    current_teacher: &mut Option<i64>,
    source: &Source,
    company_id: i64,
    len: usize,
) {
    let teacher_id = assigned_teacher(source, company_id).map(|t| t.id);
    let same_as_previous = teacher_id.is_some() && teacher_id == *current_teacher;
    *current_teacher = teacher_id;

    match (teacher_id, layout.teacher_groups.last_mut()) {
        (None, _) => {}
        (Some(_), Some(last)) if same_as_previous => last.len += len,
        (Some(_), _) => layout.teacher_groups.push(MinutesGroup {
            start: layout.rows.len(),
            len,
        }),
    }
}

/// Şeflik türünün imza şeridinde basılacak unvanı (MADDE 6/4). Bölüm şefliği
/// burada hiç görünmez; o zaten kendi bloğuna (chief_name) ayrılmıştır.
fn signature_title(chief_type: ChiefType) -> &'static str {
    match chief_type {
        ChiefType::WorkshopLab => WORKSHOP_LAB_TITLE,
        ChiefType::None | ChiefType::Department => TEACHER_TITLE,
    }
}

/// İmza şeridi: alan şefinin adı ve şef DIŞINDAKİ aktif öğretmenlerin ad ve
/// unvan listesi, ADA göre Türkçe sırayla (kullanıcı isteği; tablodaki
/// koordinatör sırası bundan bağımsızdır, bkz. `ordered_companies`). Şeflik
/// `teacher_load_periods` projeksiyonundan (`as_of` günü geçerli aralık)
/// okunur; eski `teachers.chief_type` sütunu artık kaynak değildir (bkz.
/// dosya başı ve `db::teachers::list_with_load_as_of`). Okulda en fazla bir
/// bölüm şefi olabilir (`domain::history::decide::chief`), bu yüzden
/// `chief_name` tek bir isimdir.
fn build_signature_block(mut with_load: Vec<TeacherWithLoadAsOf>) -> (String, Vec<SignatureTeacher>) {
    with_load.retain(|t| t.teacher.is_active != 0);
    with_load.sort_by_cached_key(|t| {
        (
            turkish_sort_key(&t.teacher.first_name),
            turkish_sort_key(&t.teacher.last_name),
            t.teacher.id,
        )
    });

    let chief_name = with_load
        .iter()
        .find(|t| t.load.chief_type == ChiefType::Department)
        .map(|t| teacher_display_name(&t.teacher))
        .unwrap_or_default();

    let field_teachers = with_load
        .into_iter()
        .filter(|t| t.load.chief_type != ChiefType::Department)
        .map(|t| SignatureTeacher {
            name: teacher_display_name(&t.teacher),
            title: signature_title(t.load.chief_type).to_string(),
        })
        .collect();

    (chief_name, field_teachers)
}

/// Boş değer yerine noktalı yer tutucuyu seçer.
fn or_placeholder(value: &str, placeholder: &str) -> String {
    if value.is_empty() {
        placeholder.to_string()
    } else {
        value.to_string()
    }
}

/// Dönemin komisyon tutanağı verisini toplar.
pub async fn build_minutes_data(pool: &SqlitePool, term: &str) -> AppResult<MinutesData> {
    let (academic_year, start_year) = parse_academic_year(term)?;

    let school_name = optional_setting(pool, SCHOOL_NAME_KEY).await?;
    if school_name.is_empty() {
        return Err(AppError::Validation(
            "Okul adı boş. Tutanak başlığı okul adını kullanır; Ayarlar'dan okul adını girin."
                .into(),
        ));
    }
    let field_name = optional_setting(pool, FIELD_NAME_KEY).await?;
    let principal_name = optional_setting(pool, PRINCIPAL_NAME_KEY).await?;

    let source = load_source(pool, term).await?;
    let table = build_table(&source);

    // İmza şeridi ayrı bir okuma: kapasite hesaplayan her yerin (`teacher_
    // commands`, `dashboard_commands`) kullandığı aynı "bugün geçerli"
    // deseni (`current_as_of` + `list_with_load_as_of`), çünkü şeflik burada
    // da projeksiyondan gelmeli, eski sütundan değil.
    let as_of = teaching_load::current_as_of(pool, term).await?;
    let teachers_with_load = teachers::list_with_load_as_of(pool, term, as_of).await?;
    let (chief_name, field_teachers) = build_signature_block(teachers_with_load);

    let school_upper = turkish_uppercase(&school_name);
    let title_field = or_placeholder(&turkish_uppercase(&field_name), TITLE_FIELD_PLACEHOLDER);
    let intro_field = or_placeholder(&field_name, INTRO_FIELD_PLACEHOLDER);
    let principal = or_placeholder(&turkish_uppercase(&principal_name), PRINCIPAL_PLACEHOLDER);

    Ok(MinutesData {
        year_line: format!("{academic_year} EĞİTİM ÖĞRETİM YILI"),
        addressee_line: format!("{school_upper} MÜDÜRLÜĞÜNE"),
        school_line: school_upper,
        field_line: format!(
            "{title_field} ALANI KOORDİNATÖR ÖĞRETMEN  BELİRLEME KOMİSYON TUTANAĞI"
        ),
        intro: INTRO_TEMPLATE.replace("{field}", &intro_field),
        closing_line: CLOSING_LINE.to_string(),
        chief_label: CHIEF_SIGNATURE_LABEL.to_string(),
        teachers_label: TEACHERS_SIGNATURE_LABEL.to_string(),
        chief_name,
        field_teachers,
        column_headers: COLUMN_HEADERS.iter().map(|h| h.to_string()).collect(),
        rows: table.rows,
        groups: table.groups,
        teacher_groups: table.teacher_groups,
        approval_line: APPROVAL_CONFIRMATION_LINE.to_string(),
        approval_date_line: format!("…./…/{start_year}"),
        principal_name: principal,
        principal_title_line: PRINCIPAL_TITLE_LINE.to_string(),
        note_text: NOTE_TEXT.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::commission_minutes_test_support::*;

    #[test]
    fn turkish_uppercase_maps_dotted_and_dotless_i_correctly() {
        assert_eq!(
            turkish_uppercase("Mesleki ve Teknik Anadolu Lisesi"),
            "MESLEKİ VE TEKNİK ANADOLU LİSESİ"
        );
        assert_eq!(turkish_uppercase("ışık"), "IŞIK");
        assert_eq!(turkish_uppercase("çğöşü"), "ÇĞÖŞÜ");
        assert_eq!(turkish_uppercase("Perşembe"), "PERŞEMBE");
    }

    #[test]
    fn turkish_sort_key_follows_the_turkish_alphabet() {
        let mut names = [
            "Zeytin", "Şirin", "İyi", "Işık", "Iğdır", "Çiftçi", "Cem", "Acar",
        ];
        names.sort_by_cached_key(|n| turkish_sort_key(n));
        assert_eq!(
            names,
            ["Acar", "Cem", "Çiftçi", "Iğdır", "Işık", "İyi", "Şirin", "Zeytin"]
        );
    }

    #[test]
    fn turkish_sort_key_is_case_insensitive_and_puts_space_before_letters() {
        assert_eq!(turkish_sort_key("acar"), turkish_sort_key("ACAR"));
        assert!(turkish_sort_key("Ay Elektrik") < turkish_sort_key("Aya"));
    }

    #[test]
    fn academic_year_is_extracted_from_the_term() {
        assert_eq!(
            parse_academic_year("2026-2027/1").unwrap(),
            ("2026-2027".to_string(), 2026)
        );
    }

    #[test]
    fn unreadable_terms_are_rejected_with_a_turkish_message() {
        for term in ["", "2026", "2026-2028/1", "26-27/1", "abcd-efgh/1"] {
            let err = parse_academic_year(term).unwrap_err();
            assert!(matches!(err, AppError::Validation(_)), "{term:?}");
            assert!(err.to_string().contains("Ayarlar"), "{term:?}: {err}");
        }
    }

    #[tokio::test]
    async fn builds_one_row_per_student_with_sequential_index_and_group_ranges() {
        let (_dir, pool) = test_pool().await;
        seed_full_scenario(&pool).await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        assert_eq!(data.rows.len(), 8);
        let indexes: Vec<usize> = data.rows.iter().map(|r| r.index).collect();
        assert_eq!(indexes, (1..=8).collect::<Vec<_>>());
        let groups: Vec<(usize, usize)> = data.groups.iter().map(|g| (g.start, g.len)).collect();
        // Öztürk: Acar, İyi · Yılmaz: Çiftçi (2), Şirin (3) · atanmamış: Zeytin.
        assert_eq!(groups, vec![(0, 1), (1, 1), (2, 2), (4, 3), (7, 1)]);
    }

    #[tokio::test]
    async fn rows_sort_by_teacher_then_company_then_student_in_turkish_order() {
        let (_dir, pool) = test_pool().await;
        seed_full_scenario(&pool).await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();
        let pairs: Vec<(&str, &str)> = data
            .rows
            .iter()
            .map(|r| (r.company_name.as_str(), r.student_name.as_str()))
            .collect();

        assert_eq!(
            pairs,
            vec![
                ("Acar Otomasyon", "Emre Kaya"),
                ("İyi Aydınlatma", "Can Uçar"),
                ("Çiftçi Pano Sanayi", "Deniz Arı"),
                ("Çiftçi Pano Sanayi", "Ece Bulut"),
                ("Şirin Elektrik Ltd.", "Burak Çelik"),
                ("Şirin Elektrik Ltd.", "Ali Demir"),
                ("Şirin Elektrik Ltd.", "Zeynep Şahin"),
                ("Zeytin Bobinaj", "Selin Ak"),
            ]
        );
    }

    fn spans(groups: &[MinutesGroup]) -> Vec<(usize, usize)> {
        groups.iter().map(|g| (g.start, g.len)).collect()
    }

    /// Referans şablonda koordinatör hücresi aynı öğretmenin ARDIŞIK işletmeleri
    /// boyunca tek birleşik hücredir. Mehmet Öztürk'ün işletmeleri ad sırasında
    /// (Acar, İyi) Çiftçi ile bölünürdü; öğretmene göre gruplu sırada bitişiktir.
    #[tokio::test]
    async fn same_teachers_companies_are_consecutive_and_share_one_teacher_span() {
        let (_dir, pool) = test_pool().await;
        seed_full_scenario(&pool).await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        // Öztürk: satır 0–1 (Acar, İyi); Yılmaz: satır 2–6 (Çiftçi, Şirin).
        assert_eq!(spans(&data.teacher_groups), vec![(0, 2), (2, 5)]);
        assert_eq!(data.rows[0].teacher, data.rows[1].teacher);
        assert_eq!(data.rows[2].teacher, data.rows[6].teacher);
    }

    #[tokio::test]
    async fn teachers_are_ordered_by_last_then_first_name_in_turkish_alphabet() {
        let (_dir, pool) = test_pool().await;
        // Soyad sırası: Çakır < Öztürk < Yılmaz; aynı soyadta ada göre: Ali < Zeki.
        let teachers = [
            ("Ayşe", "Yılmaz"),
            ("Zeki", "Çakır"),
            ("Ali", "Çakır"),
            ("Mehmet", "Öztürk"),
        ];
        for (i, (first, last)) in teachers.iter().enumerate() {
            let teacher = seed_teacher(&pool, first, last).await;
            let company = seed_company(&pool, &format!("İşletme {i}"), None).await;
            seed_student(&pool, Some(company), "Öğ", &format!("Renci{i}")).await;
            seed_assignment(&pool, teacher, company, 1 + i as i64, 1).await;
        }

        let data = build_minutes_data(&pool, TERM).await.unwrap();
        let order: Vec<&str> = data.rows.iter().map(|r| r.teacher.as_str()).collect();

        assert_eq!(
            order,
            ["Ali ÇAKIR", "Zeki ÇAKIR", "Mehmet ÖZTÜRK", "Ayşe YILMAZ"]
        );
    }

    #[tokio::test]
    async fn unassigned_companies_come_last_sorted_by_name_without_a_teacher_span() {
        let (_dir, pool) = test_pool().await;
        seed_full_scenario(&pool).await;
        // Adı alfabede en başta olsa da atanmamış işletme sona gider.
        let aaa = seed_company(&pool, "Aaa Atanmamış", None).await;
        seed_student(&pool, Some(aaa), "Kemal", "Sunal").await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();
        let tail: Vec<(&str, &str)> = data.rows[7..]
            .iter()
            .map(|r| (r.company_name.as_str(), r.teacher.as_str()))
            .collect();

        assert_eq!(tail, [("Aaa Atanmamış", ""), ("Zeytin Bobinaj", "")]);
        // Atanmamışlar hiçbir öğretmen aralığına girmez.
        let covered: usize = data.teacher_groups.iter().map(|g| g.len).sum();
        assert_eq!(covered, 7);
        assert!(data.teacher_groups.iter().all(|g| g.start + g.len <= 7));
    }

    /// D/F/G işletme başına birleşiktir; hiçbir işletme grubu iki öğretmen
    /// grubuna taşmamalı, yoksa xlsx'te iki birleşik aralık çakışırdı.
    #[tokio::test]
    async fn every_company_group_stays_inside_one_teacher_group_or_outside_all() {
        let (_dir, pool) = test_pool().await;
        seed_full_scenario(&pool).await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        for company in &data.groups {
            let end = company.start + company.len;
            let containing: Vec<_> = data
                .teacher_groups
                .iter()
                .filter(|t| company.start < t.start + t.len && t.start < end)
                .collect();
            assert!(
                containing.len() <= 1,
                "işletme {company:?} birden çok öğretmene taşıyor"
            );
            if let Some(teacher) = containing.first() {
                assert!(teacher.start <= company.start && end <= teacher.start + teacher.len);
            }
        }
    }

    #[tokio::test]
    async fn no_assignments_means_no_teacher_groups() {
        let (_dir, pool) = test_pool().await;
        let company = seed_company(&pool, "Yalnız", None).await;
        seed_student(&pool, Some(company), "Bir", "Kişi").await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        assert_eq!(data.rows.len(), 1);
        assert!(data.teacher_groups.is_empty());
    }

    #[tokio::test]
    async fn assigned_company_carries_teacher_uppercase_day_and_hours() {
        let (_dir, pool) = test_pool().await;
        seed_full_scenario(&pool).await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();
        let sirin = &data.rows[4];

        assert_eq!(sirin.teacher, "Ayşe YILMAZ");
        assert_eq!(sirin.day, "CUMA");
        assert_eq!(sirin.hours_label, "8");
        // Türkçe büyük harf: gün adlarında da i/ı/ş/ç dönüşümü doğru olmalı.
        assert_eq!(data.rows[2].day, "ÇARŞAMBA");
        assert_eq!(data.rows[1].day, "PERŞEMBE");
        assert_eq!(data.rows[0].day, "SALI");
        assert_eq!(data.rows[0].teacher, "Mehmet ÖZTÜRK");
    }

    #[tokio::test]
    async fn unassigned_company_leaves_teacher_day_and_hours_empty() {
        let (_dir, pool) = test_pool().await;
        seed_full_scenario(&pool).await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();
        let zeytin = data.rows.last().unwrap();

        assert_eq!(zeytin.company_name, "Zeytin Bobinaj");
        assert_eq!(zeytin.teacher, "");
        assert_eq!(zeytin.day, "");
        assert_eq!(zeytin.hours_label, "");
        // Uzaklık atamadan bağımsızdır; yine de basılır.
        assert_eq!(zeytin.distance_label, "12.5");
    }

    #[tokio::test]
    async fn honorary_company_prints_fahri_instead_of_hours() {
        let (_dir, pool) = test_pool().await;
        seed_full_scenario(&pool).await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        assert_eq!(data.rows[2].hours_label, "Fahri");
        assert_eq!(data.rows[3].hours_label, "Fahri");
    }

    #[tokio::test]
    async fn distance_is_round_trip_with_one_decimal_or_empty_when_unknown() {
        let (_dir, pool) = test_pool().await;
        seed_full_scenario(&pool).await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        // Tek yön 2.2 → 4.4 ve 8.6 → 17.2: saat kuralları gidiş-dönüşe bakar ve
        // referanstaki örnekler (4.4 km → 6 saat, 17.2 km → 8 saat) ancak böyle uyar.
        assert_eq!(data.rows[0].distance_label, "4.4");
        assert_eq!(data.rows[0].round_trip_km, Some(4.4));
        assert_eq!(data.rows[4].distance_label, "17.2");
        assert_eq!(data.rows[4].round_trip_km, Some(17.2));
        assert_eq!(
            data.rows[2].distance_label, "9.0",
            "tam sayı da tek ondalıkla basılır"
        );
        assert_eq!(data.rows[1].distance_label, "");
        assert_eq!(data.rows[1].round_trip_km, None);
    }

    /// Fixture'daki tek yön mesafeler bilerek yarım değerlidir; hiçbir çıktı
    /// tek yön değeri basmamalı.
    #[tokio::test]
    async fn one_way_value_is_never_printed_as_is() {
        let (_dir, pool) = test_pool().await;
        seed_full_scenario(&pool).await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();
        let printed: Vec<&str> = data
            .rows
            .iter()
            .map(|r| r.distance_label.as_str())
            .collect();

        for one_way in ["2.2", "8.6", "4.5"] {
            assert!(
                !printed.contains(&one_way),
                "{one_way} tek yön olarak basılmış: {printed:?}"
            );
        }
    }

    #[tokio::test]
    async fn assigned_company_without_students_gets_a_single_row_with_empty_student() {
        let (_dir, pool) = test_pool().await;
        let teacher = seed_teacher(&pool, "Ali", "Kaya").await;
        let company = seed_company(&pool, "Boş İşletme", Some(3.0)).await;
        seed_hours(&pool, company, 4, false).await;
        seed_assignment(&pool, teacher, company, 1, 1).await;
        // Öğrencisiz ve atanmamış: tutanakta yer almamalı.
        seed_company(&pool, "Unutulmuş İşletme", None).await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        assert_eq!(data.rows.len(), 1);
        assert_eq!(data.rows[0].company_name, "Boş İşletme");
        assert_eq!(data.rows[0].student_name, "");
        assert_eq!(data.rows[0].day, "PAZARTESİ");
        assert_eq!(data.groups, vec![MinutesGroup { start: 0, len: 1 }]);
    }

    #[tokio::test]
    async fn students_without_a_company_do_not_appear() {
        let (_dir, pool) = test_pool().await;
        seed_student(&pool, None, "Yer", "Siz").await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        assert!(data.rows.is_empty());
    }

    #[tokio::test]
    async fn empty_term_yields_valid_data_with_no_rows() {
        let (_dir, pool) = test_pool().await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        assert!(data.rows.is_empty());
        assert!(data.groups.is_empty());
        assert_eq!(data.column_headers.len(), 7);
    }

    #[tokio::test]
    async fn header_uses_dotted_placeholders_when_optional_settings_are_empty() {
        let (_dir, pool) = test_pool().await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        assert_eq!(data.year_line, "2026-2027 EĞİTİM ÖĞRETİM YILI");
        assert_eq!(data.school_line, "ATATÜRK MESLEKİ VE TEKNİK ANADOLU LİSESİ");
        assert_eq!(
            data.addressee_line,
            "ATATÜRK MESLEKİ VE TEKNİK ANADOLU LİSESİ MÜDÜRLÜĞÜNE"
        );
        assert_eq!(
            data.field_line,
            "............ ALANI KOORDİNATÖR ÖĞRETMEN  BELİRLEME KOMİSYON TUTANAĞI"
        );
        assert!(data
            .intro
            .starts_with("..................... alanındaki öğrencilerimizin"));
        assert_eq!(data.approval_line, "Uygundur");
        assert_eq!(data.approval_date_line, "…./…/2026");
        assert_eq!(data.principal_name, "…………………");
        assert_eq!(data.principal_title_line, "Okul Müdürü");
    }

    #[tokio::test]
    async fn header_uses_the_configured_field_and_principal_names() {
        let (_dir, pool) = test_pool().await;
        settings::set(&pool, FIELD_NAME_KEY, "Elektrik-Elektronik Teknolojisi")
            .await
            .unwrap();
        settings::set(&pool, PRINCIPAL_NAME_KEY, "Ömer Yiğit")
            .await
            .unwrap();

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        assert!(data
            .field_line
            .starts_with("ELEKTRİK-ELEKTRONİK TEKNOLOJİSİ ALANI KOORDİNATÖR"));
        assert!(data
            .intro
            .starts_with("Elektrik-Elektronik Teknolojisi alanındaki"));
        assert!(!data.intro.contains("....."), "paragrafta nokta kalmamalı");
        assert_eq!(data.approval_date_line, "…./…/2026");
        assert_eq!(data.principal_name, "ÖMER YİĞİT");
    }

    /// Brief: onay bloğu parçalara ayrılır ki yalnız müdür adı kalın basılabilsin.
    /// Müdür adı doluyken kendi alanında, büyük harfe çevrilmiş olarak gelir.
    #[tokio::test]
    async fn approval_block_splits_principal_name_into_its_own_field_when_set() {
        let (_dir, pool) = test_pool().await;
        settings::set(&pool, PRINCIPAL_NAME_KEY, "Ömer Yiğit")
            .await
            .unwrap();

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        assert_eq!(data.approval_line, "Uygundur");
        assert_eq!(data.principal_name, "ÖMER YİĞİT");
        assert_eq!(data.principal_title_line, "Okul Müdürü");
    }

    /// Müdür adı boşken bugünkü noktalı yer tutucu davranışı DEĞİŞMEMELİ; bu
    /// alan artık ayrı basılsa da yer tutucu üretimi aynı `or_placeholder`
    /// yolundan geçer.
    #[tokio::test]
    async fn approval_block_keeps_the_placeholder_when_principal_name_is_empty() {
        let (_dir, pool) = test_pool().await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        assert_eq!(data.principal_name, "…………………");
    }

    #[tokio::test]
    async fn whitespace_only_optional_settings_count_as_empty() {
        let (_dir, pool) = test_pool().await;
        settings::set(&pool, FIELD_NAME_KEY, "   ").await.unwrap();

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        assert!(data.field_line.starts_with("............ ALANI"));
    }

    #[tokio::test]
    async fn fixed_texts_match_the_school_template_verbatim() {
        let (_dir, pool) = test_pool().await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        assert_eq!(
            data.note_text,
            "AÇIKLAMA : Komisyon, Ortaöğretim Kurumlar Yönetmeliğinin 140. maddesinine istinaden \
             oluşturulmuştur."
        );
        assert!(data.intro.contains("142. maddesine göre"));
        assert!(data.intro.contains("koordinatör öğremenlerin"));
        assert_eq!(data.closing_line, "Olurlarınıza arz ederiz.");
        assert_eq!(data.column_headers[2], "ÖĞRENCİ \nADI SOYADI");
    }

    #[tokio::test]
    async fn blank_school_name_is_rejected_with_a_message_pointing_to_settings() {
        let (_dir, pool) = test_pool().await;
        settings::set(&pool, SCHOOL_NAME_KEY, "").await.unwrap();

        let err = build_minutes_data(&pool, TERM).await.unwrap_err();

        assert!(matches!(err, AppError::Validation(_)));
        assert!(err.to_string().contains("Ayarlar"));
    }

    #[tokio::test]
    async fn unreadable_active_term_is_rejected() {
        let (_dir, pool) = test_pool().await;

        let err = build_minutes_data(&pool, "").await.unwrap_err();

        assert!(matches!(err, AppError::Validation(_)));
    }

    fn entries(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn settings_validation_accepts_normal_and_absent_values() {
        assert!(validate_minutes_settings(&entries(&[])).is_ok());
        assert!(validate_minutes_settings(&entries(&[
            (PRINCIPAL_NAME_KEY, "Ömer Yiğit"),
            (FIELD_NAME_KEY, ""),
            (
                "school_name",
                "Herhangi\nbir şey; bu anahtar burada denetlenmez"
            ),
        ]))
        .is_ok());
    }

    #[test]
    fn settings_validation_rejects_newlines_and_overlong_values() {
        let long = "a".repeat(OPTIONAL_TEXT_MAX_CHARS + 1);
        for (key, value) in [
            (PRINCIPAL_NAME_KEY, "Ömer\nYiğit"),
            (FIELD_NAME_KEY, "Alan\tAdı"),
            (FIELD_NAME_KEY, long.as_str()),
        ] {
            let err = validate_minutes_settings(&entries(&[(key, value)])).unwrap_err();
            assert!(matches!(err, AppError::Validation(_)), "{key}");
        }
    }

    /// Komisyon tutanağı toplantı tutanağının gerçek boyutu: 1 bölüm şefi, 9
    /// atölye/laboratuvar şefi, 2 sade öğretmen — hepsi aktif. İsimler Türkçe
    /// sıralamayı da sınıyor (Ç, İ, Ö, Ş, Ü, I/İ ayrımı).
    async fn seed_signature_scenario(pool: &SqlitePool) -> Vec<i64> {
        let mut ids = vec![seed_teacher_with_chief(pool, "Ayşe", "Yılmaz", ChiefType::Department).await];
        for (first, last) in [
            ("Mehmet", "Öztürk"),
            ("Zeynep", "Çelik"),
            ("Ali", "Şahin"),
            ("Fatma", "Güneş"),
            ("Kemal", "İyi"),
            ("Elif", "Ünlü"),
            ("Burak", "Işık"),
            ("Ece", "Arı"),
            ("Deniz", "Doğan"),
        ] {
            ids.push(seed_teacher_with_chief(pool, first, last, ChiefType::WorkshopLab).await);
        }
        for (first, last) in [("Selin", "Ak"), ("Emre", "Bulut")] {
            ids.push(seed_teacher_with_chief(pool, first, last, ChiefType::None).await);
        }
        ids
    }

    #[tokio::test]
    async fn signature_block_lists_the_chief_apart_and_titles_the_rest() {
        let (_dir, pool) = test_pool().await;
        seed_signature_scenario(&pool).await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        assert_eq!(data.chief_name, "Ayşe YILMAZ");
        assert_eq!(data.field_teachers.len(), 11, "12 aktif öğretmen - 1 alan şefi");
        assert!(
            data.field_teachers.iter().all(|t| t.name != "Ayşe YILMAZ"),
            "alan şefi kendi bloğunda görünmemeli"
        );

        let workshop_count = data
            .field_teachers
            .iter()
            .filter(|t| t.title == "Atölye/Laboratuvar Şefi")
            .count();
        assert_eq!(workshop_count, 9);

        let none_titles: Vec<&str> = ["Selin AK", "Emre BULUT"]
            .iter()
            .map(|name| {
                data.field_teachers
                    .iter()
                    .find(|t| t.name == *name)
                    .unwrap_or_else(|| panic!("{name} bulunamadı: {:?}", data.field_teachers))
                    .title
                    .as_str()
            })
            .collect();
        assert_eq!(none_titles, ["Öğretmen", "Öğretmen"]);
    }

    #[tokio::test]
    async fn without_a_department_chief_every_active_teacher_is_a_field_teacher() {
        let (_dir, pool) = test_pool().await;
        for (first, last) in [
            ("Mehmet", "Öztürk"),
            ("Zeynep", "Çelik"),
            ("Ali", "Şahin"),
            ("Fatma", "Güneş"),
            ("Kemal", "İyi"),
            ("Elif", "Ünlü"),
            ("Burak", "Işık"),
            ("Ece", "Arı"),
            ("Deniz", "Doğan"),
        ] {
            seed_teacher_with_chief(&pool, first, last, ChiefType::WorkshopLab).await;
        }
        for (first, last) in [("Selin", "Ak"), ("Emre", "Bulut"), ("Kaan", "Er")] {
            seed_teacher_with_chief(&pool, first, last, ChiefType::None).await;
        }

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        assert_eq!(data.chief_name, "", "alan şefi yoksa başlık altı boş kalır");
        assert_eq!(data.field_teachers.len(), 12);
    }

    #[tokio::test]
    async fn inactive_teachers_appear_in_neither_signature_block() {
        let (_dir, pool) = test_pool().await;
        seed_teacher_with_chief(&pool, "Ayşe", "Yılmaz", ChiefType::Department).await;
        let inactive = seed_teacher_with_chief(&pool, "Pasif", "Kişi", ChiefType::WorkshopLab).await;
        deactivate_teacher(&pool, inactive).await;

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        assert_eq!(data.chief_name, "Ayşe YILMAZ");
        assert!(
            data.field_teachers.iter().all(|t| !t.name.contains("Pasif")),
            "pasif öğretmen listede: {:?}",
            data.field_teachers
        );
        assert!(data.field_teachers.is_empty());
    }

    #[tokio::test]
    async fn field_teachers_are_sorted_by_first_name_in_turkish_alphabet() {
        let (_dir, pool) = test_pool().await;
        // Kullanıcı isteği: imza şeridi ADA göre sıralanır (soyada göre değil,
        // bkz. `build_signature_block`). Aynı sekiz adın Türkçe sırası zaten
        // `turkish_sort_key_follows_the_turkish_alphabet`de kanıtlı; burada
        // isimler kasıtlı olarak eklenme sırasının TERSİNDE verilir ki test
        // gerçekten sıralamayı sınasın, ekleme sırasını değil.
        for (first, last) in [
            ("Zeytin", "H"),
            ("Şirin", "G"),
            ("İyi", "F"),
            ("Işık", "E"),
            ("Iğdır", "D"),
            ("Çiftçi", "C"),
            ("Cem", "B"),
            ("Acar", "A"),
        ] {
            seed_teacher_with_chief(&pool, first, last, ChiefType::None).await;
        }

        let data = build_minutes_data(&pool, TERM).await.unwrap();
        let order: Vec<&str> = data.field_teachers.iter().map(|t| t.name.as_str()).collect();

        assert_eq!(
            order,
            [
                "Acar A", "Cem B", "Çiftçi C", "Iğdır D", "Işık E", "İyi F", "Şirin G", "Zeytin H",
            ]
        );
    }

    /// R5c: `teacher_load_periods` projeksiyonunda satırı olmayan öğretmen
    /// `None` sayılır (bkz. `db::teachers::zero_load`). Eski `teachers.
    /// chief_type` sütunu burada bilerek yanlış ("department") bırakılır;
    /// çıktı bunu yok sayıp projeksiyon varsayılanını (None → "Öğretmen")
    /// vermeli — aksi hâlde bu test, bayat sütun okunduğunda düşerdi.
    #[tokio::test]
    async fn signature_titles_follow_the_projection_even_when_the_legacy_column_is_stale() {
        let (_dir, pool) = test_pool().await;
        let id = seed_teacher(&pool, "Test", "Öğretmen").await;
        sqlx::query("UPDATE teachers SET chief_type = 'department' WHERE id = ?1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();

        let data = build_minutes_data(&pool, TERM).await.unwrap();

        assert_eq!(
            data.chief_name, "",
            "projeksiyonda satır yok; bayat sütun şef üretmemeli"
        );
        assert_eq!(data.field_teachers.len(), 1);
        assert_eq!(data.field_teachers[0].title, "Öğretmen");
    }
}
