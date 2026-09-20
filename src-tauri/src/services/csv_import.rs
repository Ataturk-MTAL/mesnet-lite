use crate::domain::models::{NewCompany, NewStudent};
use crate::error::{AppError, AppResult};

/// JotForm dışa aktarımının sütun sırası. Başlıklar tekrar ettiği için
/// ("Ad" ve "Soyad" hem öğrenci hem yetkili için kullanılıyor) sütunlara
/// ada göre değil KONUMA göre erişilir.
mod column {
    pub const SUBMISSION_DATE: usize = 0;
    pub const STUDENT_FIRST_NAME: usize = 1;
    pub const STUDENT_LAST_NAME: usize = 2;
    pub const COMPANY_NAME: usize = 3;
    pub const CONTACT_FIRST_NAME: usize = 4;
    pub const CONTACT_LAST_NAME: usize = 5;
    pub const PHONE: usize = 6;
    pub const LOCATOR: usize = 7;
    pub const EMAIL: usize = 8;
    pub const BRANCH: usize = 9;
    pub const STUDENT_NO: usize = 10;
    pub const GRADE: usize = 11;
    pub const EXPECTED_COUNT: usize = 12;
}

/// `İşletmenizi bulun` alanının ayrıştırılmış hâli.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LocatorParts {
    /// Okuldan işletmeye TEK YÖN yol mesafesi (km).
    pub one_way_distance_km: Option<f64>,
    /// İşletmenin gerçek adresi (`Address:` satırı).
    pub address: Option<String>,
}

/// Bir CSV satırından çıkan işletme + öğrenci çifti.
#[derive(Debug, Clone)]
pub struct ImportRow {
    pub company: NewCompany,
    pub student: NewStudent,
}

/// Ayrıştırma sonucu. Bozuk satırlar sessizce atılmaz, `errors` içinde raporlanır.
#[derive(Debug, Default)]
pub struct ParsedCsv {
    pub rows: Vec<ImportRow>,
    pub errors: Vec<String>,
}

/// `İşletmenizi bulun` alanını ayrıştırır. Alan üç satırlıdır:
///
/// ```text
/// Result: <OKULUN adı ve adresi>
/// Distance: 6.8 km
/// Address: <İŞLETMENİN gerçek adresi>
/// ```
///
/// `Result:` satırı OKULUN adresidir, işletmenin değil — yok sayılır.
/// Coğrafi kodlamaya giden metin `Address:` satırıdır.
pub fn parse_locator_field(raw: &str) -> LocatorParts {
    let mut parts = LocatorParts::default();

    for line in raw.lines() {
        let line = line.trim();

        if let Some(rest) = line.strip_prefix("Distance:") {
            // "6.8 km" veya "6,8 km" biçimleri
            let number = rest.trim().trim_end_matches("km").trim().replace(',', ".");
            parts.one_way_distance_km = number.parse::<f64>().ok();
        } else if let Some(rest) = line.strip_prefix("Address:") {
            let address = rest.trim();
            if !address.is_empty() {
                parts.address = Some(address.to_string());
            }
        }
    }

    parts
}

/// JotForm tarihini (`Sep 11, 2026`) ISO-8601'e (`2026-09-11`) çevirir.
/// Biçim tanınmazsa None döner; tarih zorunlu alan değildir.
pub fn parse_submission_date(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    chrono::NaiveDate::parse_from_str(trimmed, "%b %d, %Y")
        .ok()
        .map(|d| d.format("%Y-%m-%d").to_string())
}

fn cell(record: &csv::StringRecord, index: usize) -> String {
    record.get(index).unwrap_or_default().trim().to_string()
}

fn optional_cell(record: &csv::StringRecord, index: usize) -> Option<String> {
    let value = cell(record, index);
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// JotForm CSV içeriğini ayrıştırır. Girdi UTF-8 BOM'lu olabilir.
pub fn parse_jotform_csv(content: &str) -> AppResult<ParsedCsv> {
    // BOM varsa kaldır; yoksa csv crate ilk başlığı "\u{feff}Submission Date" okur.
    let content = content.trim_start_matches('\u{feff}');

    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(content.as_bytes());

    let mut parsed = ParsedCsv::default();

    for (index, result) in reader.records().enumerate() {
        // Kullanıcıya gösterilecek satır numarası: başlık 1. satır, veri 2'den başlar.
        let line_number = index + 2;

        let record = match result {
            Ok(record) => record,
            Err(err) => {
                parsed
                    .errors
                    .push(format!("{line_number}. satır okunamadı: {err}"));
                continue;
            }
        };

        if record.len() < column::EXPECTED_COUNT {
            parsed.errors.push(format!(
                "{line_number}. satırda {} sütun var, {} bekleniyordu",
                record.len(),
                column::EXPECTED_COUNT
            ));
            continue;
        }

        let company_name = cell(&record, column::COMPANY_NAME);
        if company_name.is_empty() {
            parsed
                .errors
                .push(format!("{line_number}. satırda işletme adı boş"));
            continue;
        }

        let locator = parse_locator_field(record.get(column::LOCATOR).unwrap_or_default());
        let address = match locator.address {
            Some(address) => address,
            None => {
                parsed.errors.push(format!(
                    "{line_number}. satırda adres bulunamadı ({company_name})"
                ));
                continue;
            }
        };

        let company = NewCompany {
            name: company_name,
            contact_first_name: cell(&record, column::CONTACT_FIRST_NAME),
            contact_last_name: cell(&record, column::CONTACT_LAST_NAME),
            phone: cell(&record, column::PHONE),
            email: cell(&record, column::EMAIL),
            address_text: address,
            latitude: None,
            longitude: None,
            one_way_distance_km: locator.one_way_distance_km,
            notes: String::new(),
        };

        let student = NewStudent {
            first_name: cell(&record, column::STUDENT_FIRST_NAME),
            last_name: cell(&record, column::STUDENT_LAST_NAME),
            student_no: optional_cell(&record, column::STUDENT_NO),
            grade: cell(&record, column::GRADE),
            branch: cell(&record, column::BRANCH),
            company_id: None,
            submitted_at: parse_submission_date(&cell(&record, column::SUBMISSION_DATE)),
            // Dönem CSV'de yoktur; içe aktarma sırasında aktif dönemle damgalanır.
            term: String::new(),
        };

        parsed.rows.push(ImportRow { company, student });
    }

    if parsed.rows.is_empty() && parsed.errors.is_empty() {
        return Err(AppError::CsvParse("Dosyada veri satırı yok".into()));
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCATOR_SAMPLE: &str = "Result: Test Lisesi, Test Mahallesi, Toroslar/Mersin, Türkiye\n\
         Distance: 6.8 km\n\
         Address: Test Mahallesi, 6214. Sk. No:40, 33020 Akdeniz/Mersin, Türkiye";

    #[test]
    fn locator_field_extracts_distance_and_address() {
        let parts = parse_locator_field(LOCATOR_SAMPLE);
        assert_eq!(parts.one_way_distance_km, Some(6.8));
        assert_eq!(
            parts.address.as_deref(),
            Some("Test Mahallesi, 6214. Sk. No:40, 33020 Akdeniz/Mersin, Türkiye")
        );
    }

    /// `Result:` satırı OKULUN adresidir; adres olarak alınmamalı.
    #[test]
    fn locator_field_ignores_result_line() {
        let parts = parse_locator_field(LOCATOR_SAMPLE);
        assert!(!parts.address.unwrap().contains("Test Lisesi"));
    }

    #[test]
    fn locator_field_accepts_comma_decimal_separator() {
        let parts = parse_locator_field("Distance: 12,5 km\nAddress: Test adres");
        assert_eq!(parts.one_way_distance_km, Some(12.5));
    }

    #[test]
    fn locator_field_returns_empty_parts_when_lines_missing() {
        let parts = parse_locator_field("Result: sadece okul");
        assert_eq!(parts.one_way_distance_km, None);
        assert_eq!(parts.address, None);
    }

    #[test]
    fn submission_date_converts_jotform_format_to_iso() {
        assert_eq!(
            parse_submission_date("Sep 11, 2026").as_deref(),
            Some("2026-09-11")
        );
    }

    #[test]
    fn submission_date_returns_none_for_unknown_format() {
        assert_eq!(parse_submission_date("11.09.2026"), None);
        assert_eq!(parse_submission_date("   "), None);
    }

    fn csv_with_rows(rows: &str) -> String {
        let header = "\"Submission Date\",Ad,Soyad,\"İşletme Adı\",Ad,Soyad,\
             \"Telefon Numarası\",\"İşletmenizi bulun\",E-posta,\
             \"DAL Bİlgisi Seçiniz\",\"Öğrenci No\",\"Sınıf Seçiniz\"\n";
        format!("\u{feff}{header}{rows}")
    }

    #[test]
    fn parses_a_complete_row_into_company_and_student() {
        let row = format!(
            "\"Sep 11, 2026\",AHMET,YILMAZ,\"TEST TEKNIK\",MEHMET,DEMIR,\
             \"(532) 000-0000\",\"{LOCATOR_SAMPLE}\",,\"Elektronik Haberleşme\",,12/C\n"
        );
        let parsed = parse_jotform_csv(&csv_with_rows(&row)).unwrap();

        assert!(parsed.errors.is_empty(), "hata: {:?}", parsed.errors);
        assert_eq!(parsed.rows.len(), 1);

        let entry = &parsed.rows[0];
        assert_eq!(entry.company.name, "TEST TEKNIK");
        assert_eq!(entry.company.contact_first_name, "MEHMET");
        assert_eq!(entry.company.phone, "(532) 000-0000");
        assert_eq!(entry.company.one_way_distance_km, Some(6.8));
        assert!(entry.company.address_text.contains("6214. Sk."));

        assert_eq!(entry.student.first_name, "AHMET");
        assert_eq!(entry.student.grade, "12/C");
        assert_eq!(entry.student.branch, "Elektronik Haberleşme");
        // CSV'de öğrenci no boş olabilir; boş metin değil None olmalı.
        assert_eq!(entry.student.student_no, None);
        assert_eq!(entry.student.submitted_at.as_deref(), Some("2026-09-11"));
    }

    /// BOM ilk sütun başlığına yapışmamalı ve veri satırı normal okunmalı.
    #[test]
    fn handles_utf8_bom() {
        let row = format!(
            "\"Sep 11, 2026\",A,B,\"BOM TEST\",C,D,\"\",\"{LOCATOR_SAMPLE}\",,DAL,,12/D\n"
        );
        let parsed = parse_jotform_csv(&csv_with_rows(&row)).unwrap();
        assert_eq!(parsed.rows.len(), 1);
        assert_eq!(parsed.rows[0].company.name, "BOM TEST");
    }

    /// Adres yoksa satır sessizce atlanmaz, hata listesine girer.
    #[test]
    fn reports_row_without_address_instead_of_dropping_it() {
        let row = "\"Sep 11, 2026\",A,B,\"ADRESSIZ\",C,D,\"\",\"Result: sadece okul\",,DAL,,12/C\n";
        let parsed = parse_jotform_csv(&csv_with_rows(row)).unwrap();

        assert!(parsed.rows.is_empty());
        assert_eq!(parsed.errors.len(), 1);
        assert!(parsed.errors[0].contains("ADRESSIZ"));
        assert!(parsed.errors[0].starts_with("2. satır"));
    }

    #[test]
    fn reports_row_with_blank_company_name() {
        let row = format!("\"Sep 11, 2026\",A,B,\"\",C,D,\"\",\"{LOCATOR_SAMPLE}\",,DAL,,12/C\n");
        let parsed = parse_jotform_csv(&csv_with_rows(&row)).unwrap();

        assert!(parsed.rows.is_empty());
        assert_eq!(parsed.errors.len(), 1);
        assert!(parsed.errors[0].contains("işletme adı boş"));
    }

    /// Gerçek JotForm dışa aktarımına karşı doğrulama. Dosya `data/`
    /// klasöründe yoksa test atlanır (depoya commit edilmez, bkz.
    /// `.gitignore`: `/data`), böylece CSV olmadan da `cargo test` yeşil
    /// kalır — ama atlama `real_export_fixture::find_real_export` içinde
    /// `eprintln!` ile açıkça bildirilir.
    #[test]
    fn parses_the_real_jotform_export_when_present() {
        let Some(csv_path) = crate::services::real_export_fixture::find_real_export(
            "csv",
            "ATLANDI: data/ klasöründe gerçek JotForm CSV'si yok",
        ) else {
            return;
        };

        let content = std::fs::read_to_string(&csv_path).unwrap();
        let parsed = parse_jotform_csv(&content).unwrap();

        assert!(parsed.errors.is_empty(), "ayrıştırma hataları: {:?}", parsed.errors);
        assert_eq!(parsed.rows.len(), 32, "beklenen 32 öğrenci satırı");

        // Her satırda adres ve mesafe okunabilmeli.
        assert!(parsed.rows.iter().all(|r| !r.company.address_text.is_empty()));
        assert!(parsed.rows.iter().all(|r| r.company.one_way_distance_km.is_some()));

        // 28 tekil işletme; 4 işletme iki öğrencili.
        let unique: std::collections::HashSet<String> = parsed
            .rows
            .iter()
            .map(|r| crate::db::companies::normalize_name(&r.company.name))
            .collect();
        assert_eq!(unique.len(), 28, "beklenen 28 tekil işletme");
    }

    #[test]
    fn errors_when_file_has_no_data_rows() {
        let err = parse_jotform_csv(&csv_with_rows("")).unwrap_err();
        assert!(matches!(err, AppError::CsvParse(_)));
    }
}
