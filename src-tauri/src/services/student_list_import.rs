//! e-Okul sınıf listesi (.XLS) ayrıştırıcısı.
//!
//! Dosya, e-Okul'un Crystal Reports ile ürettiği ESKİ BIFF/OLE2 `.XLS`
//! biçimindedir (`file` çıktısı "Composite Document File V2"), OOXML `xlsx`
//! DEĞİLDİR — bu yüzden `rust_xlsxwriter` (yalnız YAZAR) ya da bir OOXML
//! okuyucu bu dosyaları açamaz; okuma için `calamine` (eski BIFF okuyabilen
//! tek bağımlılık) kullanılır.
//!
//! Sütun konumları RAPORA göre değişir (bir okulda "Soyadı" 6. sütunda,
//! başka birinde 7.), bu yüzden sütunlar KONUMDAN değil başlık METNİNDEN
//! eşlenir (`csv_import.rs`'in tam tersi: JotForm dışa aktarımı sabit
//! konumludur, e-Okul raporu değildir).

use calamine::{open_workbook_from_rs, Data, Range, Reader, Xls};
use std::io::Cursor;

use crate::error::{AppError, AppResult};

/// Başlık satırı ilk kaç satırda aranır. e-Okul raporlarında başlık 4. satırda
/// (0-indeksli 3) durur; okul logosu/şube başlığı üstte üç satır kaplar.
const HEADER_SEARCH_LIMIT: usize = 10;

/// e-Okul sınıf listesindeki tek öğrenci satırı.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedStudentRow {
    pub student_no: Option<String>,
    pub first_name: String,
    pub last_name: String,
    /// Dosyada "Dalı" sütunu yoksa boş dizgi (bkz. modül başı; R020 raporları
    /// dal yerine Cinsiyeti/Pansiyon Durum taşır).
    pub branch: String,
}

/// Bir sınıf listesi dosyasının tamamı: A1 başlığından çözülen sınıf/alan
/// bilgisi artı öğrenci satırları.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedClassList {
    /// `"12/C"` biçiminde: rakam + şube harfi.
    pub grade: String,
    pub field_name: String,
    pub rows: Vec<ParsedStudentRow>,
}

fn invalid_file() -> AppError {
    AppError::Validation("Bu dosya e-Okul sınıf listesi değil.".into())
}

/// Başlık satırında isme göre bulunan sütunların konumu.
struct ColumnMap {
    serial: u32,
    student_no: u32,
    first_name: u32,
    last_name: u32,
    /// "Dalı" sütunu her raporda yok (R020 örnekleri).
    branch: Option<u32>,
}

fn cell_text(range: &Range<Data>, row: u32, col: u32) -> String {
    match range.get_value((row, col)) {
        Some(Data::String(s)) => s.trim().to_string(),
        _ => String::new(),
    }
}

/// Sayısal bir hücreyi tam sayı metnine çevirir. e-Okul'un dışa aktardığı
/// `Öğrenci No`/`S.No` hücreleri float gelir (`9001.0`); kesir sıfırsa tam
/// sayı biçiminde yazılır ki `"9001.0"` değil `"9001"` saklansın.
fn numeric_cell_to_id(value: &Data) -> Option<String> {
    match value {
        Data::Int(i) => Some(i.to_string()),
        Data::Float(f) if f.fract() == 0.0 => Some((*f as i64).to_string()),
        Data::Float(f) => Some(f.to_string()),
        Data::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ => None,
    }
}

/// A1 hücresindeki çok satırlı başlığın SON satırından sınıf ve alan adını
/// çıkarır, ör. `"AMP - 12. Sınıf / C Şubesi (ELEKTRİK-ELEKTRONİK
/// TEKNOLOJİSİ ALANI) Sınıf Listesi"` → `("12/C", "ELEKTRİK-ELEKTRONİK
/// TEKNOLOJİSİ ALANI")`.
fn parse_class_title(raw: &str) -> AppResult<(String, String)> {
    const CLASS_MARKER: &str = ". Sınıf / ";
    const SECTION_MARKER: &str = " Şubesi";

    let last_line = raw.lines().last().unwrap_or("").trim();

    let class_idx = last_line.find(CLASS_MARKER).ok_or_else(invalid_file)?;
    let grade_number: String = last_line[..class_idx]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if grade_number.is_empty() {
        return Err(invalid_file());
    }

    let after_class = &last_line[class_idx + CLASS_MARKER.len()..];
    let section_idx = after_class.find(SECTION_MARKER).ok_or_else(invalid_file)?;
    let section_letter = after_class[..section_idx].trim();
    if section_letter.is_empty() {
        return Err(invalid_file());
    }

    let open = last_line.find('(').ok_or_else(invalid_file)?;
    let close = last_line[open..].find(')').map(|i| open + i).ok_or_else(invalid_file)?;
    let field_name = last_line[open + 1..close].trim().to_string();
    if field_name.is_empty() {
        return Err(invalid_file());
    }

    Ok((format!("{grade_number}/{section_letter}"), field_name))
}

/// Başlık satırını ilk `HEADER_SEARCH_LIMIT` satırda `S.No` + `Öğrenci No`
/// ikilisini arayarak bulur, sonra AYNI satırdaki diğer sütunları isimden eşler.
fn find_header(range: &Range<Data>) -> AppResult<(u32, ColumnMap)> {
    let (rows, cols) = range.get_size();
    let search_limit = (rows.min(HEADER_SEARCH_LIMIT)) as u32;
    let cols = cols as u32;

    for row in 0..search_limit {
        let mut serial = None;
        let mut student_no = None;
        let mut first_name = None;
        let mut last_name = None;
        let mut branch = None;

        for col in 0..cols {
            match cell_text(range, row, col).as_str() {
                "S.No" => serial = Some(col),
                "Öğrenci No" => student_no = Some(col),
                "Adı" => first_name = Some(col),
                "Soyadı" => last_name = Some(col),
                "Dalı" => branch = Some(col),
                _ => {}
            }
        }

        if let (Some(serial), Some(student_no), Some(first_name), Some(last_name)) =
            (serial, student_no, first_name, last_name)
        {
            return Ok((row, ColumnMap { serial, student_no, first_name, last_name, branch }));
        }
    }

    Err(invalid_file())
}

/// Başlık satırından sonraki satırları `S.No` hücresi sayı olmaktan
/// çıkana (özet satırı, "Kız Öğrenci Sayısı ...") kadar okur.
fn read_student_rows(range: &Range<Data>, header_row: u32, columns: &ColumnMap) -> Vec<ParsedStudentRow> {
    let (total_rows, _) = range.get_size();
    let mut rows = Vec::new();

    for row in (header_row + 1)..total_rows as u32 {
        let is_student_row = matches!(
            range.get_value((row, columns.serial)),
            Some(Data::Int(_)) | Some(Data::Float(_))
        );
        if !is_student_row {
            break;
        }

        let student_no = range
            .get_value((row, columns.student_no))
            .and_then(numeric_cell_to_id);
        let branch = columns.branch.map(|col| cell_text(range, row, col)).unwrap_or_default();

        rows.push(ParsedStudentRow {
            student_no,
            first_name: cell_text(range, row, columns.first_name),
            last_name: cell_text(range, row, columns.last_name),
            branch,
        });
    }

    rows
}

/// e-Okul sınıf listesi `.XLS` içeriğini ayrıştırır. `bytes` frontend'den
/// olduğu gibi (dosya sistemi izni gerekmeden) gelen ham içeriktir.
pub fn parse_student_list_xls(bytes: &[u8]) -> AppResult<ParsedClassList> {
    let mut workbook: Xls<_> = open_workbook_from_rs(Cursor::new(bytes.to_vec()))
        .map_err(|_| invalid_file())?;

    let range = workbook
        .worksheet_range_at(0)
        .ok_or_else(invalid_file)?
        .map_err(|_| invalid_file())?;

    let title = cell_text(&range, 0, 0);
    let (grade, field_name) = parse_class_title(&title)?;

    let (header_row, columns) = find_header(&range)?;
    let rows = read_student_rows(&range, header_row, &columns);

    Ok(ParsedClassList { grade, field_name, rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gerçek e-Okul dosyaları öğrenci kişisel verisi içerdiği için depoya
    /// commit edilemez; bunun yerine `scripts/make_eokul_fixtures.py`'nin
    /// ürettiği, aynı BIFF8 yapısını taşıyan TAMAMEN KURGUSAL `.xls`
    /// dosyaları `src/services/fixtures/eokul/` altında commit'lidir — bu
    /// yüzden testler ATLAMADAN her ortamda (CI dahil) koşar.
    fn fixture(name_fragment: &str) -> Vec<u8> {
        let fixtures_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/services/fixtures/eokul");
        let path = std::fs::read_dir(&fixtures_dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xls"))
                    && p.file_name().unwrap().to_string_lossy().contains(name_fragment)
            })
            .unwrap_or_else(|| panic!("fixture bulunamadı: {name_fragment}"));
        std::fs::read(path).unwrap()
    }

    #[test]
    fn parse_class_title_extracts_grade_and_field_name() {
        let raw = "AMP - 12. Sınıf / C Şubesi (ELEKTRİK-ELEKTRONİK TEKNOLOJİSİ ALANI) Sınıf Listesi";
        let (grade, field) = parse_class_title(raw).unwrap();
        assert_eq!(grade, "12/C");
        assert_eq!(field, "ELEKTRİK-ELEKTRONİK TEKNOLOJİSİ ALANI");
    }

    #[test]
    fn parse_class_title_rejects_unrelated_text() {
        assert!(parse_class_title("rastgele bir metin").is_err());
    }

    /// r076_12d.xls (dal sütunlu, 12/D, kurgusal): 17 öğrenci, iki dal (8 + 9).
    #[test]
    fn r076_class_d_has_seventeen_rows_split_across_two_branches() {
        let parsed = parse_student_list_xls(&fixture("r076_12d")).unwrap();
        assert_eq!(parsed.grade, "12/D");
        assert_eq!(parsed.field_name, "ELEKTRİK-ELEKTRONİK TEKNOLOJİSİ ALANI");
        assert_eq!(parsed.rows.len(), 17);

        let dagitim = parsed.rows.iter().filter(|r| r.branch == "Elektrik Tesisatları ve Dağıtımı").count();
        let bakim = parsed.rows.iter().filter(|r| r.branch == "Endüstriyel Bakım Onarım").count();
        assert_eq!(dagitim, 8);
        assert_eq!(bakim, 9);
    }

    /// r076_12c.xls (dal sütunlu, 12/C, kurgusal): 17 öğrenci, hepsi aynı dal.
    #[test]
    fn r076_class_c_has_seventeen_rows_all_same_branch() {
        let parsed = parse_student_list_xls(&fixture("r076_12c")).unwrap();
        assert_eq!(parsed.grade, "12/C");
        assert_eq!(parsed.rows.len(), 17);
        assert!(parsed.rows.iter().all(|r| r.branch == "Elektronik ve Haberleşme"));

        let first = &parsed.rows[0];
        assert_eq!(first.student_no.as_deref(), Some("9001"));
        assert_eq!(first.first_name, "ALVAR");
        assert_eq!(first.last_name, "QUENNET");
    }

    /// R020 dosyalarında Dalı sütunu yok (Cinsiyeti/Pansiyon Durum var) ve
    /// sütun konumları R076'dan FARKLI (Soyadı 6 değil 7); yine de aynı
    /// öğrenciler doğru ayrışmalı — bu, sütunların İSİMDEN eşlendiğinin kanıtı.
    #[test]
    fn r020_files_parse_despite_missing_branch_column_and_different_offsets() {
        let class_c = parse_student_list_xls(&fixture("r020_12c")).unwrap();
        let class_d = parse_student_list_xls(&fixture("r020_12d")).unwrap();

        assert_eq!(class_c.grade, "12/C");
        assert_eq!(class_d.grade, "12/D");
        assert_eq!(class_c.rows.len(), 17);
        assert_eq!(class_d.rows.len(), 17);
        assert!(class_c.rows.iter().all(|r| r.branch.is_empty()), "R020'de dal sütunu yok");

        let first = &class_c.rows[0];
        assert_eq!(first.student_no.as_deref(), Some("9001"));
        assert_eq!(first.first_name, "ALVAR");
        assert_eq!(first.last_name, "QUENNET");
    }

    /// e-Okul olmayan bir dosya (rastgele bayt) Validation hatası vermeli.
    #[test]
    fn rejects_a_file_that_is_not_a_real_xls() {
        let err = parse_student_list_xls(b"bu bir xls dosyasi degil").unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }
}
