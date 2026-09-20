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
use rust_xlsxwriter::{Format, FormatAlign, FormatBorder, Workbook, Worksheet};
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
const HEADER_ROW: u32 = 17; // 18
const TABLE_FIRST_ROW: u32 = 18; // 19…

const HEADER_ROW_HEIGHT: f64 = 24.75;
const APPROVAL_ROW_COUNT: u32 = 6;
const NOTE_ROW_COUNT: u32 = 2;

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
    write_text_span(
        sheet,
        signature_rows,
        (2, 4),
        &data.teachers_label,
        &f.signature,
    )?;
    Ok(())
}

fn write_header_row(sheet: &mut Worksheet, data: &MinutesData, f: &Formats) -> AppResult<()> {
    sheet.set_row_height(HEADER_ROW, HEADER_ROW_HEIGHT)?;
    for (col, text) in data.column_headers.iter().enumerate() {
        // Şablonda öğrenci başlığı Times 11, uzaklık başlığı Times 9'dur.
        let format = match col as u16 {
            COL_STUDENT => &f.header_serif,
            COL_DISTANCE => &f.header_small,
            _ => &f.header,
        };
        sheet.write_string_with_format(HEADER_ROW, col as u16, text, format)?;
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

fn write_table(sheet: &mut Worksheet, data: &MinutesData, f: &Formats) -> AppResult<()> {
    for (offset, row) in data.rows.iter().enumerate() {
        let excel_row = TABLE_FIRST_ROW + offset as u32;
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
        let span = group_rows(group);
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
            group_rows(group),
            COL_TEACHER,
            &first.teacher,
            &f.centered,
        )?;
    }
    Ok(())
}

/// Bir satır grubunun sayfadaki (ilk, son) satırı, her iki uç dahil.
fn group_rows(group: &MinutesGroup) -> (u32, u32) {
    let first = TABLE_FIRST_ROW + group.start as u32;
    (first, first + group.len as u32 - 1)
}

/// Onay ve açıklama blokları tablonun hemen ardından gelir; şablondaki boş
/// yedek satırlar dinamik tabloda yoktur.
fn write_footer(sheet: &mut Worksheet, data: &MinutesData, f: &Formats) -> AppResult<()> {
    let approval_first = TABLE_FIRST_ROW + data.rows.len() as u32;
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

fn apply_page_setup(sheet: &mut Worksheet) -> AppResult<()> {
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
    sheet.set_repeat_rows(HEADER_ROW, HEADER_ROW)?;
    Ok(())
}

/// Hazır bir `MinutesData`'yı Excel baytlarına çevirir (saf; veritabanı yok).
pub fn render_minutes_xlsx(data: &MinutesData) -> AppResult<Vec<u8>> {
    let mut workbook = Workbook::new();
    let formats = Formats::new();

    let sheet = workbook.add_worksheet();
    sheet.set_name(SHEET_NAME)?;
    for (col, width) in COLUMN_WIDTHS.iter().enumerate() {
        sheet.set_column_width(col as u16, *width - COLUMN_PADDING)?;
    }

    write_preamble(sheet, data, &formats)?;
    write_header_row(sheet, data, &formats)?;
    write_table(sheet, data, &formats)?;
    write_footer(sheet, data, &formats)?;
    apply_page_setup(sheet)?;

    Ok(workbook.save_to_buffer()?)
}

/// Dönemin komisyon tutanağını Excel dosyası olarak üretir.
pub async fn build_minutes_xlsx(pool: &SqlitePool, term: &str) -> AppResult<Vec<u8>> {
    render_minutes_xlsx(&build_minutes_data(pool, term).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::commission_minutes_test_support::*;

    /// Bir xlsx dosyası ZIP kapsayıcısıdır; ilk yerel dosya başlığı "PK\x03\x04".
    fn assert_is_zip(bytes: &[u8]) {
        assert!(bytes.starts_with(b"PK\x03\x04"), "xlsx bir ZIP olmalı");
    }

    #[test]
    fn group_rows_are_inclusive_and_offset_by_the_header() {
        assert_eq!(
            group_rows(&MinutesGroup { start: 0, len: 1 }),
            (TABLE_FIRST_ROW, TABLE_FIRST_ROW)
        );
        assert_eq!(
            group_rows(&MinutesGroup { start: 3, len: 4 }),
            (TABLE_FIRST_ROW + 3, TABLE_FIRST_ROW + 6)
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
}
