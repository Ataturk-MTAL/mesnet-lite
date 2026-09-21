//! İşletme Belirleme Komisyon Tutanağı'nın Excel çıktısı.
//!
//! Okulun kendi şablonunun (`işletme belirleme komisyon tutanağı.xlsx`) birebir
//! yeniden üretimidir: aynı sütun genişlikleri, birleşik aralıklar, yazı
//! tipleri, kenarlıklar ve A4 dikey / tek sayfa genişliğinde yazdırma
//! ayarı. Şablon 52 sabit satırdı; burada tablo öğrenci sayısı kadar uzar ve
//! onay ile açıklama blokları tablonun hemen ardından gelir.
//!
//! Veri `commission_minutes::MinutesData`'dan gelir; PDF çıktısıyla aynıdır.

use crate::error::AppResult;
use crate::services::commission_minutes::{build_minutes_data, MinutesData, MinutesGroup};
use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook, Worksheet};
use sqlx::SqlitePool;

const SHEET_NAME: &str = "Komisyon Tutanağı";

/// Şablonun A–G sütun genişlikleri (dosyadaki `<col width>` değerleri).
const COLUMN_WIDTHS: [f64; 7] = [6.57, 39.43, 32.43, 14.0, 35.71, 16.86, 10.14];
/// `rust_xlsxwriter` verilen genişliğe Excel'in hücre dolgusunu (~0,71) ekleyip
/// yazar; şablondaki değer zaten dolgu dahil olduğundan çıktı aynı çıksın diye
/// buradan düşülür. Aksi hâlde her sütun şablondan 0,71 geniş olurdu.
const COLUMN_PADDING: f64 = 0.7109375;
const LAST_COLUMN: u16 = 6;

// Satır numaraları 0 tabanlıdır; yorumdaki Excel satırı şablonunkiyle aynıdır.
const TITLE_FIRST_ROW: u32 = 0; // 1–4
const TITLE_LAST_ROW: u32 = 3;
const ADDRESSEE_ROW: u32 = 5; // 6
const INTRO_FIRST_ROW: u32 = 7; // 8–11
const INTRO_LAST_ROW: u32 = 10;
const CLOSING_ROW: u32 = 12; // 13
const SIGNATURE_TOP_ROW: u32 = 14; // 15
const SIGNATURE_FIRST_ROW: u32 = 15; // 16–17
const SIGNATURE_LAST_ROW: u32 = 16;
/// İmza etiketlerinin (SIGNATURE_FIRST/LAST_ROW) hemen altında başlayan, alan
/// şefinin adını ve alan öğretmenleri listesini taşıyan blok. Bu bloktan
/// sonraki HER ŞEY (başlık satırı, tablo, onay/açıklama) isim sayısına göre
/// kayar; bkz. `RowLayout`.
const SIGNATURE_NAMES_FIRST_ROW: u32 = SIGNATURE_LAST_ROW + 1; // 18…

const HEADER_ROW_HEIGHT: f64 = 24.75;
const APPROVAL_ROW_COUNT: u32 = 6;
const NOTE_ROW_COUNT: u32 = 2;

/// Kullanıcı isteği: imza şeridindeki alan öğretmenleri "4 sütunlu bir ızgara,
/// soldan sağa satır satır aksın, ismin altında unvan olsun" biçiminde
/// gösterilir. Ad ve unvan iki ayrı Excel satırına yazıldığından her ızgara
/// satırı 2 Excel satırı kaplar.
const TEACHER_GRID_COLUMNS: usize = 4;
const ROWS_PER_TEACHER_ENTRY: u32 = 2;
/// Ad/unvan satırlarına normalden biraz fazla yükseklik verilir; brief'in
/// istediği "ferah boşluk" burada satır yüksekliğiyle sağlanır (aradaki boş
/// bir Excel satırı yerine — o durumda `signature_names_row_count`'un "her
/// ızgara satırı 2 Excel satırı" varsayımı bozulurdu).
const TEACHER_GRID_ROW_HEIGHT: f64 = 16.5;

/// 4 ızgara sütununun C–G (5 fiziksel sütun, 0 tabanlı indeks 2–6) üzerindeki
/// karşılığı. 5 fiziksel sütunu 4 gruba bölmenin tek yolu ikisini birleştirmek;
/// D (14) ve G (10.14) tek başına en dar sütunlardı, bu yüzden G, komşusu F
/// (16.86) ile birleştirilip dört grup arasındaki genişlik farkı en aza
/// indirildi (C 32.43, D 14, E 35.71, F+G 27 — alternatif gruplamalar, ör.
/// C+D tek grup, aradaki farkı daha da büyütüyordu).
const TEACHER_GRID_COLUMN_GROUPS: [(u16, u16); TEACHER_GRID_COLUMNS] =
    [(2, 2), (3, 3), (4, 4), (5, LAST_COLUMN)];

/// Şablonun sayfa kenar boşlukları (inç): 0,236" sağ/sol, 0,748" üst/alt.
const MARGIN_SIDE: f64 = 0.236_220_472_440_944_9;
const MARGIN_VERTICAL: f64 = 0.748_031_496_062_992_1;
const MARGIN_HEADER_FOOTER: f64 = 0.314_960_629_921_259_84;
/// `set_paper_size` için A4 kodu (ECMA-376, paperSize = 9).
const PAPER_A4: u8 = 9;

const SANS_FONT: &str = "Calibri";
const SERIF_FONT: &str = "Times New Roman";

/// Sütunlar (0 tabanlı).
const COL_INDEX: u16 = 0;
const COL_COMPANY: u16 = 1;
const COL_STUDENT: u16 = 2;
const COL_DISTANCE: u16 = 3;
const COL_TEACHER: u16 = 4;
const COL_DAY: u16 = 5;
const COL_HOURS: u16 = 6;

/// Şablondaki hücre biçimleri. Kenarlık ince ve dört yanlıdır; birleşik
/// aralıkta `merge_range` biçimi bütün hücrelere uyguladığı için şablondaki
/// üst/orta/alt kenarlık parçalarının toplamıyla aynı görünümü verir.
struct Formats {
    title: Format,
    addressee: Format,
    intro: Format,
    plain: Format,
    signature: Format,
    /// 4 sütunlu ızgaradaki bir öğretmenin adı (üst satır); kalın ve ortalı.
    teacher_grid_name: Format,
    /// Aynı hücrenin unvan satırı (alt satır); "isim altında unvan" isteği
    /// gereği addan ayrışsın diye küçük punto ve soluk (gri) renkte basılır.
    teacher_grid_title: Format,
    header: Format,
    header_serif: Format,
    header_small: Format,
    index: Format,
    company: Format,
    student: Format,
    centered: Format,
    distance: Format,
    approval: Format,
    note: Format,
}

fn base(font: &str, size: f64) -> Format {
    Format::new().set_font_name(font).set_font_size(size)
}

fn boxed(format: Format) -> Format {
    format.set_border(FormatBorder::Thin)
}

fn centered(format: Format) -> Format {
    format
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
}

impl Formats {
    fn new() -> Self {
        let sans = || base(SANS_FONT, 11.0);
        Self {
            title: boxed(centered(base(SERIF_FONT, 12.0).set_bold().set_text_wrap())),
            addressee: base(SERIF_FONT, 10.5)
                .set_bold()
                .set_align(FormatAlign::Center),
            intro: sans()
                .set_align(FormatAlign::Left)
                .set_align(FormatAlign::Top)
                .set_text_wrap(),
            plain: sans(),
            signature: centered(sans().set_text_wrap()),
            teacher_grid_name: centered(sans()).set_bold(),
            teacher_grid_title: centered(base(SANS_FONT, 9.0))
                .set_italic()
                .set_font_color(Color::Gray),
            header: boxed(centered(sans().set_text_wrap())),
            header_serif: boxed(centered(base(SERIF_FONT, 11.0).set_text_wrap())),
            header_small: boxed(centered(base(SERIF_FONT, 9.0).set_text_wrap())),
            index: boxed(centered(sans())),
            company: boxed(sans().set_align(FormatAlign::VerticalCenter)),
            student: boxed(sans().set_align(FormatAlign::VerticalCenter)),
            centered: boxed(centered(sans())),
            // Uzaklık tek ondalıkla gösterilir ("9.0"); PDF'teki basımla aynı.
            distance: boxed(centered(sans().set_num_format("0.0"))),
            approval: boxed(centered(sans().set_text_wrap())),
            note: boxed(
                sans()
                    .set_align(FormatAlign::Center)
                    .set_align(FormatAlign::Top)
                    .set_text_wrap(),
            ),
        }
    }
}

/// Tek hücreli aralıkta `merge_range` hata verdiğinden, birleştirilecek bir
/// satır yoksa hücreyi doğrudan yazar.
fn write_text_span(
    sheet: &mut Worksheet,
    (first_row, last_row): (u32, u32),
    (first_col, last_col): (u16, u16),
    text: &str,
    format: &Format,
) -> AppResult<()> {
    if first_row == last_row && first_col == last_col {
        sheet.write_string_with_format(first_row, first_col, text, format)?;
    } else {
        sheet.merge_range(first_row, first_col, last_row, last_col, text, format)?;
    }
    Ok(())
}

fn write_preamble(sheet: &mut Worksheet, data: &MinutesData, f: &Formats) -> AppResult<()> {
    // Şablondaki başlık, boşluklarla satırlara bölünmüş tek hücredir; burada
    // gerçek satır sonları kullanılır ki sütun genişliği değişince bozulmasın.
    let title = [
        data.year_line.as_str(),
        data.school_line.as_str(),
        data.field_line.as_str(),
    ]
    .join("\n");
    write_text_span(
        sheet,
        (TITLE_FIRST_ROW, TITLE_LAST_ROW),
        (0, LAST_COLUMN),
        &title,
        &f.title,
    )?;
    write_text_span(
        sheet,
        (ADDRESSEE_ROW, ADDRESSEE_ROW),
        (0, LAST_COLUMN),
        &data.addressee_line,
        &f.addressee,
    )?;

    // Şablonda paragraf 16 boşlukla girintilenmiştir; Excel'de ilk satır
    // girintisi olmadığından aynı yöntem korunur.
    let intro = format!("{}{}", " ".repeat(16), data.intro);
    write_text_span(
        sheet,
        (INTRO_FIRST_ROW, INTRO_LAST_ROW),
        (0, LAST_COLUMN),
        &intro,
        &f.intro,
    )?;

    sheet.write_string_with_format(CLOSING_ROW, 1, &data.closing_line, &f.plain)?;

    // Şablonun imza şeridi: üstteki boş birleşik hücreler ve altındaki iki
    // etiket (A16:B17 Alan Şefi, C16:E17 Alan Öğretmenleri).
    write_text_span(
        sheet,
        (SIGNATURE_TOP_ROW, SIGNATURE_TOP_ROW),
        (0, 1),
        "",
        &f.plain,
    )?;
    write_text_span(
        sheet,
        (SIGNATURE_TOP_ROW, SIGNATURE_TOP_ROW),
        (2, LAST_COLUMN),
        "",
        &f.plain,
    )?;
    let signature_rows = (SIGNATURE_FIRST_ROW, SIGNATURE_LAST_ROW);
    write_text_span(
        sheet,
        signature_rows,
        (0, 1),
        &data.chief_label,
        &f.signature,
    )?;
    // Öğretmenler ızgarası artık C:E değil C:G (F-G eskiden boştu) kullanır;
    // etiket de aynı genişliğe yayılır ki alttaki ızgarayla hizalı görünsün.
    write_text_span(
        sheet,
        signature_rows,
        (2, LAST_COLUMN),
        &data.teachers_label,
        &f.signature,
    )?;
    Ok(())
}

/// İmza altı isim bloğunun satır sayısı. Alan öğretmenleri artık 4 sütunlu bir
/// ızgarada, ad üstte/unvan altta iki ayrı satırda basılır (kullanıcı isteği);
/// bu yüzden gereken Excel satırı `ceil(n / 4) * 2`'dir. Alan şefi tek isim
/// olduğundan bu bloğun tamamı boyunca (A:B) ortalanır. En az 1 ızgara satırı
/// (2 Excel satırı) — 0 satırlık bir aralık `merge_range`'i (ve boş
/// senaryoda görünürlüğü) bozardı.
fn signature_names_row_count(data: &MinutesData) -> u32 {
    let grid_rows = data
        .field_teachers
        .len()
        .div_ceil(TEACHER_GRID_COLUMNS)
        .max(1);
    grid_rows as u32 * ROWS_PER_TEACHER_ENTRY
}

/// Etiketlerin (SIGNATURE_FIRST/LAST_ROW) hemen altına alan şefinin adını ve
/// alan öğretmenleri ızgarasını yazar. `chief_label`/`teachers_label`
/// metinleri burada TEKRAR yazılmaz; isimler yalnızca onların altına gelir.
/// Alan şefi ızgaraya karışmaz (kullanıcı isteği: "ayrı kalır"), A:B'de tüm
/// blok boyunca tek başına ortalanır. Alan öğretmenleri C:G üzerinde 4 sütunlu
/// bir ızgaraya soldan sağa, satır satır dizilir; her giriş adı (üstte) ve
/// unvanı (altta, küçük/soluk) ayrı Excel satırlarına yazar.
fn write_signature_names(
    sheet: &mut Worksheet,
    data: &MinutesData,
    f: &Formats,
    names_rows: u32,
) -> AppResult<()> {
    let rows = (
        SIGNATURE_NAMES_FIRST_ROW,
        SIGNATURE_NAMES_FIRST_ROW + names_rows - 1,
    );
    write_text_span(sheet, rows, (0, 1), &data.chief_name, &f.signature)?;

    for (i, teacher) in data.field_teachers.iter().enumerate() {
        let grid_row = (i / TEACHER_GRID_COLUMNS) as u32;
        let (col_first, col_last) = TEACHER_GRID_COLUMN_GROUPS[i % TEACHER_GRID_COLUMNS];
        let name_row = SIGNATURE_NAMES_FIRST_ROW + grid_row * ROWS_PER_TEACHER_ENTRY;
        let title_row = name_row + 1;
        sheet.set_row_height(name_row, TEACHER_GRID_ROW_HEIGHT)?;
        sheet.set_row_height(title_row, TEACHER_GRID_ROW_HEIGHT)?;
        write_text_span(
            sheet,
            (name_row, name_row),
            (col_first, col_last),
            &teacher.name,
            &f.teacher_grid_name,
        )?;
        write_text_span(
            sheet,
            (title_row, title_row),
            (col_first, col_last),
            &teacher.title,
            &f.teacher_grid_title,
        )?;
    }
    Ok(())
}

fn write_header_row(sheet: &mut Worksheet, data: &MinutesData, f: &Formats, header_row: u32) -> AppResult<()> {
    sheet.set_row_height(header_row, HEADER_ROW_HEIGHT)?;
    for (col, text) in data.column_headers.iter().enumerate() {
        // Şablonda öğrenci başlığı Times 11, uzaklık başlığı Times 9'dur.
        let format = match col as u16 {
            COL_STUDENT => &f.header_serif,
            COL_DISTANCE => &f.header_small,
            _ => &f.header,
        };
        sheet.write_string_with_format(header_row, col as u16, text, format)?;
    }
    Ok(())
}

/// Grup düzeyindeki bir sütunu, işletmenin bütün satırları boyunca dikey
/// birleşik hücre olarak yazar. Sayısal değerler sayı olarak saklanır
/// (şablondaki uzaklık ve ücret hücreleri de sayıdır); "Fahri" ve boş değer
/// metin kalır.
fn write_group_cell(
    sheet: &mut Worksheet,
    rows: (u32, u32),
    col: u16,
    text: &str,
    format: &Format,
) -> AppResult<()> {
    write_text_span(sheet, rows, (col, col), text, format)?;
    if let Ok(number) = text.parse::<f64>() {
        sheet.write_number_with_format(rows.0, col, number, format)?;
    }
    Ok(())
}

fn write_table(sheet: &mut Worksheet, data: &MinutesData, f: &Formats, table_first_row: u32) -> AppResult<()> {
    for (offset, row) in data.rows.iter().enumerate() {
        let excel_row = table_first_row + offset as u32;
        sheet.write_number_with_format(excel_row, COL_INDEX, row.index as f64, &f.index)?;
        sheet.write_string_with_format(excel_row, COL_COMPANY, &row.company_name, &f.company)?;
        sheet.write_string_with_format(excel_row, COL_STUDENT, &row.student_name, &f.student)?;
        // Koordinatör hücresi her satırda kenarlıklı bir boş hücredir; öğretmen
        // aralıkları aşağıda bunların üzerine birleştirilir. Atanmamış
        // işletmelerin satırları böylece boş ama çizgili kalır, birleşmez.
        sheet.write_string_with_format(excel_row, COL_TEACHER, "", &f.centered)?;
    }

    for group in &data.groups {
        let first = &data.rows[group.start];
        let span = group_rows(group, table_first_row);
        write_group_cell(
            sheet,
            span,
            COL_DISTANCE,
            &first.distance_label,
            &f.distance,
        )?;
        write_group_cell(sheet, span, COL_DAY, &first.day, &f.centered)?;
        write_group_cell(sheet, span, COL_HOURS, &first.hours_label, &f.centered)?;
    }

    // E sütunu (koordinatör) referansta aynı öğretmenin ardışık işletmeleri
    // boyunca birleşiktir; D, F, G ise işletme başına.
    for group in &data.teacher_groups {
        let first = &data.rows[group.start];
        write_group_cell(
            sheet,
            group_rows(group, table_first_row),
            COL_TEACHER,
            &first.teacher,
            &f.centered,
        )?;
    }
    Ok(())
}

/// Bir satır grubunun sayfadaki (ilk, son) satırı, her iki uç dahil. Tablo
/// artık imza altı isim bloğunun boyuna göre kaydığından başlangıç sabit bir
/// sabit değil, çağırandan gelir (bkz. `RowLayout`).
fn group_rows(group: &MinutesGroup, table_first_row: u32) -> (u32, u32) {
    let first = table_first_row + group.start as u32;
    (first, first + group.len as u32 - 1)
}

/// Onay ve açıklama blokları tablonun hemen ardından gelir; şablondaki boş
/// yedek satırlar dinamik tabloda yoktur.
fn write_footer(sheet: &mut Worksheet, data: &MinutesData, f: &Formats, table_first_row: u32) -> AppResult<()> {
    let approval_first = table_first_row + data.rows.len() as u32;
    let approval_last = approval_first + APPROVAL_ROW_COUNT - 1;
    write_text_span(
        sheet,
        (approval_first, approval_last),
        (0, LAST_COLUMN),
        &data.approval_text,
        &f.approval,
    )?;

    let note_first = approval_last + 1;
    let note_last = note_first + NOTE_ROW_COUNT - 1;
    write_text_span(
        sheet,
        (note_first, note_last),
        (0, LAST_COLUMN),
        &data.note_text,
        &f.note,
    )?;
    Ok(())
}

fn apply_page_setup(sheet: &mut Worksheet, header_row: u32) -> AppResult<()> {
    sheet
        .set_paper_size(PAPER_A4)
        .set_portrait()
        .set_margins(
            MARGIN_SIDE,
            MARGIN_SIDE,
            MARGIN_VERTICAL,
            MARGIN_VERTICAL,
            MARGIN_HEADER_FOOTER,
            MARGIN_HEADER_FOOTER,
        )
        .set_print_center_horizontally(true)
        // Tek sayfa genişliği; yükseklik sınırsız, çünkü tablo dinamik uzunlukta.
        .set_print_fit_to_pages(1, 0);
    // Tablo birden çok sayfaya taşarsa sütun başlıkları her sayfada yinelenir.
    sheet.set_repeat_rows(header_row, header_row)?;
    Ok(())
}

/// İmza altı isim bloğundan sonraki satırların (başlık, tablo) nereden
/// başladığı. Blok yüksekliği alan öğretmeni sayısına göre büyüdüğü için bu
/// satırlar artık SABİT değil; her render bu veriye göre yeniden hesaplanır.
struct RowLayout {
    header_row: u32,
    table_first_row: u32,
}

impl RowLayout {
    fn for_data(data: &MinutesData) -> Self {
        let header_row = SIGNATURE_NAMES_FIRST_ROW + signature_names_row_count(data);
        Self {
            header_row,
            table_first_row: header_row + 1,
        }
    }
}

/// Hazır bir `MinutesData`'yı Excel baytlarına çevirir (saf; veritabanı yok).
pub fn render_minutes_xlsx(data: &MinutesData) -> AppResult<Vec<u8>> {
    let mut workbook = Workbook::new();
    let formats = Formats::new();
    let layout = RowLayout::for_data(data);

    let sheet = workbook.add_worksheet();
    sheet.set_name(SHEET_NAME)?;
    for (col, width) in COLUMN_WIDTHS.iter().enumerate() {
        sheet.set_column_width(col as u16, *width - COLUMN_PADDING)?;
    }

    write_preamble(sheet, data, &formats)?;
    write_signature_names(sheet, data, &formats, signature_names_row_count(data))?;
    write_header_row(sheet, data, &formats, layout.header_row)?;
    write_table(sheet, data, &formats, layout.table_first_row)?;
    write_footer(sheet, data, &formats, layout.table_first_row)?;
    apply_page_setup(sheet, layout.header_row)?;

    Ok(workbook.save_to_buffer()?)
}

/// Dönemin komisyon tutanağını Excel dosyası olarak üretir.
pub async fn build_minutes_xlsx(pool: &SqlitePool, term: &str) -> AppResult<Vec<u8>> {
    render_minutes_xlsx(&build_minutes_data(pool, term).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::commission_minutes::SignatureTeacher;
    use crate::services::commission_minutes_test_support::*;

    /// Bir xlsx dosyası ZIP kapsayıcısıdır; ilk yerel dosya başlığı "PK\x03\x04".
    fn assert_is_zip(bytes: &[u8]) {
        assert!(bytes.starts_with(b"PK\x03\x04"), "xlsx bir ZIP olmalı");
    }

    /// Tablo başlangıcı artık sabit değil (imza altı isim bloğunun boyuna göre
    /// kayar); bu test `group_rows`'un kendi payını doğru offsetlediğini,
    /// verilen HERHANGİ bir tablo başlangıcı için sınar.
    #[test]
    fn group_rows_are_inclusive_and_offset_by_the_given_table_start() {
        let table_first_row = 19;
        assert_eq!(
            group_rows(&MinutesGroup { start: 0, len: 1 }, table_first_row),
            (table_first_row, table_first_row)
        );
        assert_eq!(
            group_rows(&MinutesGroup { start: 3, len: 4 }, table_first_row),
            (table_first_row + 3, table_first_row + 6)
        );
    }

    #[tokio::test]
    async fn empty_term_produces_a_valid_non_empty_workbook() {
        let (_dir, pool) = test_pool().await;

        let bytes = build_minutes_xlsx(&pool, TERM).await.unwrap();

        assert!(!bytes.is_empty());
        assert_is_zip(&bytes);
    }

    /// `rust_xlsxwriter` çakışan birleşik aralıkları hata olarak reddeder; bu
    /// senaryo tek satırlık ve çok satırlık grupları birlikte içerdiğinden,
    /// grup aralığı hesabı kaydığında test düşer.
    #[tokio::test]
    async fn full_scenario_merges_without_overlap_and_grows_with_data() {
        let (_empty_dir, empty_pool) = test_pool().await;
        let empty = build_minutes_xlsx(&empty_pool, TERM).await.unwrap();

        let (_dir, pool) = test_pool().await;
        seed_full_scenario(&pool).await;
        let filled = build_minutes_xlsx(&pool, TERM).await.unwrap();

        assert_is_zip(&filled);
        assert!(
            filled.len() > empty.len(),
            "dolu tutanak boş olandan büyük olmalı"
        );
    }

    #[tokio::test]
    async fn a_long_table_still_builds() {
        let (_dir, pool) = test_pool().await;
        seed_long_table(&pool).await;

        let bytes = build_minutes_xlsx(&pool, TERM).await.unwrap();

        assert_is_zip(&bytes);
    }

    #[tokio::test]
    async fn unreadable_term_is_reported_not_swallowed() {
        let (_dir, pool) = test_pool().await;

        assert!(build_minutes_xlsx(&pool, "").await.is_err());
    }

    /// İmza altı isim bloğu büyüdükçe tablo/onay/açıklama satırları kayar;
    /// bu senaryo (1 alan şefi + 11 alan öğretmeni) o kaymanın birleşik
    /// aralıkları çakıştırmadığını (`rust_xlsxwriter` hata verir) sınar.
    #[tokio::test]
    async fn a_large_signature_roster_still_builds_and_grows_the_workbook() {
        use crate::domain::models::ChiefType;

        let (_empty_dir, empty_pool) = test_pool().await;
        let empty = build_minutes_xlsx(&empty_pool, TERM).await.unwrap();

        let (_dir, pool) = test_pool().await;
        seed_full_scenario(&pool).await;
        seed_teacher_with_chief(&pool, "Ayşe", "Yılmaz İkinci", ChiefType::Department).await;
        for i in 0..10 {
            seed_teacher_with_chief(&pool, "Test", &format!("Öğretmen{i}"), ChiefType::WorkshopLab).await;
        }
        let filled = build_minutes_xlsx(&pool, TERM).await.unwrap();

        assert_is_zip(&filled);
        assert!(
            filled.len() > empty.len(),
            "isim listesi eklenince dosya büyümeli"
        );
    }

    /// Kullanıcının gerçek senaryosu: 1 alan şefi + 11 alan öğretmeni. 11,
    /// 4 sütunlu ızgarada tam 3 satır dolduran (`ceil(11/4) = 3`) sınır
    /// durumdur; birleşik aralık çakışırsa `rust_xlsxwriter` hata verir.
    #[tokio::test]
    async fn eleven_field_teachers_fill_the_grid_without_merge_overlap() {
        use crate::domain::models::ChiefType;

        let (_dir, pool) = test_pool().await;
        seed_teacher_with_chief(&pool, "Ayşe", "Yılmaz", ChiefType::Department).await;
        for i in 0..11 {
            seed_teacher_with_chief(&pool, "Test", &format!("Öğretmen{i}"), ChiefType::WorkshopLab).await;
        }

        let bytes = build_minutes_xlsx(&pool, TERM).await.unwrap();

        assert_is_zip(&bytes);
    }

    /// 13 kişi (`ceil(13/4) = 4` satır) son ızgara satırını yarım bırakır;
    /// eksik hücreler boş kalmalı, birleşik aralık yine çakışmamalı.
    #[tokio::test]
    async fn thirteen_field_teachers_fill_a_partial_last_row_without_merge_overlap() {
        use crate::domain::models::ChiefType;

        let (_dir, pool) = test_pool().await;
        seed_teacher_with_chief(&pool, "Ayşe", "Yılmaz", ChiefType::Department).await;
        for i in 0..13 {
            seed_teacher_with_chief(&pool, "Test", &format!("Öğretmen{i}"), ChiefType::WorkshopLab).await;
        }

        let bytes = build_minutes_xlsx(&pool, TERM).await.unwrap();

        assert_is_zip(&bytes);
    }

    /// Saf fonksiyon testi için asgari bir `MinutesData`; yalnızca
    /// `field_teachers` sayısı değişir, geri kalan alanlar boş verilir.
    fn minutes_data_with_field_teacher_count(count: usize) -> MinutesData {
        MinutesData {
            year_line: String::new(),
            school_line: String::new(),
            field_line: String::new(),
            addressee_line: String::new(),
            intro: String::new(),
            closing_line: String::new(),
            chief_label: String::new(),
            teachers_label: String::new(),
            chief_name: String::new(),
            field_teachers: (0..count)
                .map(|i| SignatureTeacher {
                    name: format!("Öğretmen {i}"),
                    title: "Öğretmen".into(),
                })
                .collect(),
            column_headers: vec![],
            rows: vec![],
            groups: vec![],
            teacher_groups: vec![],
            approval_text: String::new(),
            note_text: String::new(),
        }
    }

    /// 4 sütun ⇒ `ceil(n / 4)` ızgara satırı, her ızgara satırı ad+unvan için
    /// 2 Excel satırı kaplar (bkz. `TEACHER_GRID_COLUMNS`,
    /// `ROWS_PER_TEACHER_ENTRY`). 0 öğretmende bile en az 1 ızgara satırı
    /// (2 Excel satırı) ayrılır.
    #[test]
    fn signature_names_row_count_uses_a_four_column_grid_with_two_rows_per_entry() {
        let cases = [
            (0, 2),
            (1, 2),
            (4, 2),
            (5, 4),
            (8, 4),
            (9, 6),
            (11, 6),
            (12, 6),
            (13, 8),
            (16, 8),
        ];
        for (count, expected_rows) in cases {
            let data = minutes_data_with_field_teacher_count(count);
            assert_eq!(
                signature_names_row_count(&data),
                expected_rows,
                "{count} öğretmen için {expected_rows} satır bekleniyordu"
            );
        }
    }
}
