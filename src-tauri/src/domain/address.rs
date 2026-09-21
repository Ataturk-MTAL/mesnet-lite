//! Geocoding servisinden gelen adres metninden ilçe (ve yol üstü olarak il)
//! ayrıştırma.
//!
//! Kaynak adresler şu biçimde gelir: `"..., <posta kodu> <İlçe>/<İl>,
//! Türkiye"`. `Cargo.toml`'da `regex` bağımlılığı olmadığı için (yeni
//! bağımlılık eklenmedi) bu modül aynı deseni ELLE tarar:
//!
//!   `\b\d{5}\s+([^/,]+?)\s*/\s*([^,]+)`
//!
//! SON eşleşme alınır (`find_iter().last()` eşdeğeri): bazı adreslerde
//! işletmenin kendi yazdığı serbest metin, geocoding'in eklediği "posta kodu
//! İlçe/İl" ekinden ÖNCE de benzer bir il/ilçe adı geçirebilir; doğru olan
//! her zaman EN SONDAKİ (geocoding'in ürettiği) eşleşmedir.

/// Adresten ilçeyi ayrıştırır, Türkçe kurallarıyla normalize edip döner.
/// Kalıp bulunamazsa (adres eksik, kısa ya da beklenen biçimde değilse)
/// `None` döner — bu bir hata değildir, yalnızca ilçe alanı boş kalır.
pub fn parse_district(address: &str) -> Option<String> {
    find_last_district_province(address).map(|(district, _)| normalize_district_name(&district))
}

/// Adres içindeki TÜM `\b\d{5}\s+([^/,]+?)\s*/\s*([^,]+)` eşleşmelerini
/// tarar, SON bulunanı (ilçe, il) çifti olarak döner.
fn find_last_district_province(address: &str) -> Option<(String, String)> {
    let chars: Vec<char> = address.chars().collect();
    let mut last_match = None;
    let mut cursor = 0;

    while cursor < chars.len() {
        match try_match_at(&chars, cursor) {
            Some((district, province, next_cursor)) => {
                last_match = Some((district, province));
                // Regex motorlarının `find_iter`'ı gibi bir sonraki taramaya
                // bu eşleşmenin BİTTİĞİ yerden devam edilir.
                cursor = next_cursor;
            }
            None => cursor += 1,
        }
    }

    last_match
}

/// `start` konumundan itibaren `\d{5}\s+([^/,]+?)\s*/\s*([^,]+)` kalıbını
/// dener. Başarılıysa `(ilçe, il, eşleşmenin bittiği konum)` döner.
fn try_match_at(chars: &[char], start: usize) -> Option<(String, String, usize)> {
    // \b: posta kodundan hemen önceki karakter bir "kelime karakteri"
    // olmamalı (aksi halde 5 haneli dilim daha uzun bir belirtecin parçasıdır).
    if start > 0 && is_word_char(chars[start - 1]) {
        return None;
    }

    // \d{5}: TAM olarak 5 rakam; hemen ardından 6. bir rakam gelmemeli
    // (aksi halde bu posta kodu değil, daha uzun bir sayının başıdır).
    let digits_end = start + 5;
    if digits_end > chars.len() || !chars[start..digits_end].iter().all(char::is_ascii_digit) {
        return None;
    }
    if chars.get(digits_end).is_some_and(char::is_ascii_digit) {
        return None;
    }

    // \s+: en az bir boşluk.
    let mut pos = digits_end;
    let whitespace_start = pos;
    while chars.get(pos).is_some_and(|c| c.is_whitespace()) {
        pos += 1;
    }
    if pos == whitespace_start {
        return None;
    }

    let (district, pos_after_district) = scan_district(chars, pos)?;
    let (province, pos_after_province) = scan_province(chars, pos_after_district + 1);

    Some((district, province?, pos_after_province))
}

/// `([^/,]+?)\s*/`: ilçe adını `/` görülene kadar okur. Araya bir `,`
/// girerse (posta kodu ile `/` arasında virgül varsa) bu aday geçersizdir.
fn scan_district(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut pos = start;
    while let Some(&c) = chars.get(pos) {
        if c == '/' {
            let district: String = chars[start..pos].iter().collect::<String>().trim().to_string();
            return if district.is_empty() { None } else { Some((district, pos)) };
        }
        if c == ',' {
            return None;
        }
        pos += 1;
    }
    None
}

/// `([^,]+)`: ili `,` görülene ya da metin bitene kadar okur.
fn scan_province(chars: &[char], start: usize) -> (Option<String>, usize) {
    let mut pos = start;
    while chars.get(pos).is_some_and(|&c| c != ',') {
        pos += 1;
    }
    let province: String = chars[start..pos].iter().collect::<String>().trim().to_string();
    (if province.is_empty() { None } else { Some(province) }, pos)
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Türkçe kurallarıyla ilk harf büyük, kalan harfler küçük normalizasyonu.
///
/// Kaynak veri hem `AKDENİZ` hem `Akdeniz` biçiminde gelebiliyor; ilçe
/// alanı Vue tarafında gruplama anahtarı olarak kullanılacağı için aynı
/// ilçenin TEK bir değere inmesi gerekir. Rust'ın `str::to_lowercase()`'i
/// burada kullanılamaz: Unicode varsayılan eşlemesinde 'İ' (nokta) küçük
/// harfe "i̇" (birleşik noktalı, iki karakter) olarak döner, 'I' ise 'i'ye
/// döner — ikisi de Türkçe'de yanlıştır ('İ' -> 'i', 'I' -> 'ı' olmalı).
/// Bu yüzden `services/commission_minutes.rs::turkish_uppercase`'in tersi
/// burada elle yazılmıştır.
pub fn normalize_district_name(raw: &str) -> String {
    raw.split_whitespace()
        .map(capitalize_turkish_word)
        .collect::<Vec<_>>()
        .join(" ")
}

fn capitalize_turkish_word(word: &str) -> String {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let tail: String = chars.map(turkish_lower_char).collect();
    format!("{}{tail}", turkish_upper_char(first))
}

fn turkish_upper_char(c: char) -> char {
    match c {
        'i' => 'İ',
        'ı' => 'I',
        other => other.to_uppercase().next().unwrap_or(other),
    }
}

fn turkish_lower_char(c: char) -> char {
    match c {
        'İ' => 'i',
        'I' => 'ı',
        other => other.to_lowercase().next().unwrap_or(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gerçek veritabanındaki 28 adresten biri (Yenişehir grubu, 6 kayıttan
    /// biri) — standart "posta kodu İlçe/İl" ekiyle biter.
    #[test]
    fn parse_district_extracts_yenisehir_from_a_real_address() {
        let address = "Çiftlikköy, Mersin Ünv., 33110 Yenişehir/Mersin, Türkiye";
        assert_eq!(parse_district(address), Some("Yenişehir".to_string()));
    }

    /// Aynı 28 adresten biri (Akdeniz grubu): adreste YANILTICI bir 5 haneli
    /// sayı da var ("63143 sokak D:8. Blok No:4," — bu bir posta kodu değil,
    /// sokak numarası; hemen ardından '/' gelmeden önce ',' geldiği için
    /// kalıp burada eşleşmez). Doğru eşleşme yalnızca sondaki "33020
    /// Akdeniz/Mersin"dir.
    #[test]
    fn parse_district_extracts_akdeniz_from_a_real_address() {
        let address = "Mega Center, Çilek, 63143 sokak D:8. Blok No:4, 33020 Akdeniz/Mersin, Türkiye";
        assert_eq!(parse_district(address), Some("Akdeniz".to_string()));
    }

    /// Gerçek veritabanındaki KALEKİM AŞ. kaydı (28 adresin biri): işletmenin
    /// kendi yazdığı serbest metinde "AKDENİZ/MERSİN" bir kez daha geçiyor.
    /// NOT: bu belirli adreste posta kodu (`\d{5}`) yalnızca SONDAKİ
    /// "33020 Akdeniz/Mersin" önünde bulunduğu için kalıp fiilen tek kez
    /// eşleşiyor (doğrulandı) — yine de "son eşleşmeyi al" tasarımının
    /// dayandığı gerçek veri budur; ayrım gücünü kanıtlayan asıl test
    /// aşağıdaki `parse_district_prefers_the_last_match_over_the_first`.
    #[test]
    fn parse_district_handles_the_kalekim_address_with_a_repeated_district_name() {
        let address = "KALEKİM AŞ. KARADUVAR MAH. SERBEST BÖLGE 14.CADDE NO:13 AKDENİZ/MERSİN \
             KALEKİM AŞ. KARADUVAR MAH. SERBEST BÖLGE 14.CADDE NO:13, Karaduvar, \
             33020 Akdeniz/Mersin, Türkiye";
        assert_eq!(parse_district(address), Some("Akdeniz".to_string()));
    }

    /// Sondaki eşleşmenin ZORUNLU olduğunu kanıtlayan ayırt edici test:
    /// adreste iki tam eşleşme var (Kadıköy/İstanbul ve Akdeniz/Mersin);
    /// ilk eşleşme alınsaydı yanlış ilçe ("Kadıköy") dönerdi.
    #[test]
    fn parse_district_prefers_the_last_match_over_the_first() {
        let address = "Merkez, 34000 Kadıköy/İstanbul eski kayıt, 33020 Akdeniz/Mersin, Türkiye";
        assert_eq!(parse_district(address), Some("Akdeniz".to_string()));
    }

    /// Posta kodu hiç yoksa ayrıştırma başarısız olmalı; hata değil, `None`.
    #[test]
    fn parse_district_returns_none_without_a_postal_code() {
        let address = "Cumhuriyet Mahallesi, Atatürk Caddesi No:5, Mersin, Türkiye";
        assert_eq!(parse_district(address), None);
    }

    #[test]
    fn parse_district_returns_none_for_an_empty_address() {
        assert_eq!(parse_district(""), None);
    }

    /// Kısa biçim: adresin TAMAMI yalnızca "posta kodu İlçe/İl" olabilir.
    #[test]
    fn parse_district_handles_the_short_form_without_a_trailing_comma() {
        assert_eq!(parse_district("33130 Akdeniz/Mersin"), Some("Akdeniz".to_string()));
    }

    /// Vue tarafına giden değer TEK bir yazımda olmalı: büyük/küçük harf
    /// farkı gruplamayı bozmamalı.
    #[test]
    fn normalize_district_name_unifies_different_casings() {
        assert_eq!(normalize_district_name("AKDENİZ"), normalize_district_name("Akdeniz"));
        assert_eq!(normalize_district_name("akdeniz"), "Akdeniz");
    }

    /// Türkçe İ/I ayrımı: noktalı büyük İ küçükte noktalı i'ye, noktasız
    /// büyük I küçükte noktasız ı'ya döner (ASCII `to_lowercase` bunu yanlış
    /// yapar).
    #[test]
    fn normalize_district_name_respects_turkish_dotted_and_dotless_letters() {
        assert_eq!(normalize_district_name("İSTANBUL"), "İstanbul");
        assert_eq!(normalize_district_name("IĞDIR"), "Iğdır");
    }
}
