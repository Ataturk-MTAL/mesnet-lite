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
    parse_district_province(address).map(|(district, _)| district)
}

/// `parse_district`in bulduğu `(ilçe, il)` çiftinin ikisini de normalize
/// ederek döner. İl, adres çözümünün mahalle düzeyindeki yedek sorgusu için
/// gerekir (`"{mahalle}, {ilçe}, {il}, Türkiye"`); ayrı bir ayrıştırma
/// yazılmaz, çünkü ilçe ile il AYNI eşleşmeden gelmelidir.
pub fn parse_district_province(address: &str) -> Option<(String, String)> {
    find_last_district_province(address)
        .map(|(district, province)| (normalize_place_name(&district), normalize_place_name(&province)))
}

/// Mahalle öneki/eki olmayan, sokak ya da numara belirten sözcükler. Harita
/// dışa aktarımında ilk parça mahalle olmak zorunda değildir; işletme bazen
/// adresi doğrudan sokakla başlatır ("Atlas Cad. No:3, 33110 ...") ve bu
/// durumda ilk parçayı mahalle sanmak yanlış mahalle yazdırırdı.
const STREET_WORDS: [&str; 10] =
    ["sk", "sok", "sokak", "cad", "cd", "caddesi", "blv", "bulv", "bulvarı", "no"];

/// Bu ya da daha çok haneli bir sayı kapı/posta numarası sayılır ("1234 Sk.").
const MIN_STREET_NUMBER_DIGITS: usize = 3;

/// Mahalle adının sonundaki "Mahallesi" / "Mah." / "Mah" ekleri.
const NEIGHBORHOOD_SUFFIXES: [&str; 3] = ["mahallesi", "mah.", "mah"];

/// Adresin ilk virgül parçasını mahalle olarak ayrıştırır.
///
/// Yalnız adreste `posta kodu İlçe/İl` parçası VAR ve ilk parça o parça
/// DEĞİLSE, ve ilk parça sokak/numara içermiyorsa mahalle döner. Aksi hâlde
/// `None` — hata değil, mahalle alanı boş kalır. Ad, ilçeyle AYNI Türkçe
/// kuralla (`normalize_place_name`) normalize edilir.
pub fn parse_neighborhood(address: &str) -> Option<String> {
    find_last_district_province(address)?;
    let first_part = address.split(',').next()?.trim();
    // Eşleşme tek bir virgül parçasının içinde kalır (ilçe taraması virgülde
    // durur); ilk parçada eşleşme varsa ilk parça posta parçasıdır.
    if find_last_district_province(first_part).is_some() || looks_like_street(first_part) {
        return None;
    }
    let name = strip_neighborhood_suffix(first_part);
    if name.is_empty() {
        return None;
    }
    Some(normalize_place_name(&name))
}

fn looks_like_street(part: &str) -> bool {
    part.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).any(|word| {
        let is_long_number =
            word.len() >= MIN_STREET_NUMBER_DIGITS && word.chars().all(|c| c.is_ascii_digit());
        is_long_number || STREET_WORDS.contains(&turkish_lower_str(word).as_str())
    })
}

/// Sondaki mahalle ekini (tek sözcük) atar; kalan sözcükleri boşlukla birleştirir.
fn strip_neighborhood_suffix(part: &str) -> String {
    let mut words: Vec<&str> = part.split_whitespace().collect();
    let has_suffix = words
        .last()
        .is_some_and(|last| NEIGHBORHOOD_SUFFIXES.contains(&turkish_lower_str(last).as_str()));
    if has_suffix {
        words.pop();
    }
    words.join(" ")
}

fn turkish_lower_str(text: &str) -> String {
    text.chars().map(turkish_lower_char).collect()
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
/// ilçenin TEK bir değere inmesi gerekir. İl ve mahalle adı da aynı kuralla
/// normalize edilir (ikinci bir normalize yazılmaz). Rust'ın `str::to_lowercase()`'i
/// burada kullanılamaz: Unicode varsayılan eşlemesinde 'İ' (nokta) küçük
/// harfe "i̇" (birleşik noktalı, iki karakter) olarak döner, 'I' ise 'i'ye
/// döner — ikisi de Türkçe'de yanlıştır ('İ' -> 'i', 'I' -> 'ı' olmalı).
/// Bu yüzden `services/commission_minutes.rs::turkish_uppercase`'in tersi
/// burada elle yazılmıştır.
pub fn normalize_place_name(raw: &str) -> String {
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

    /// Kurgusal adres (Yenişehir örneği) — standart "posta kodu İlçe/İl"
    /// ekiyle biter.
    #[test]
    fn parse_district_extracts_yenisehir_from_a_standard_address() {
        let address = "Kurgu Mahallesi, Örnek Kampüsü, 33000 Yenişehir/Mersin, Türkiye";
        assert_eq!(parse_district(address), Some("Yenişehir".to_string()));
    }

    /// Kurgusal adres (Akdeniz örneği): adreste YANILTICI bir 5 haneli
    /// sayı da var ("98765 sokak D:2. Blok No:7," — bu bir posta kodu değil,
    /// sokak numarası; hemen ardından '/' gelmeden önce ',' geldiği için
    /// kalıp burada eşleşmez). Doğru eşleşme yalnızca sondaki "33000
    /// Akdeniz/Mersin"dir.
    #[test]
    fn parse_district_extracts_akdeniz_despite_a_misleading_five_digit_street_number() {
        let address = "Örnek İş Merkezi, Deneme, 98765 sokak D:2. Blok No:7, 33000 Akdeniz/Mersin, Türkiye";
        assert_eq!(parse_district(address), Some("Akdeniz".to_string()));
    }

    /// Kurgusal "ÖRNEK SANAYİ AŞ." kaydı: işletmenin kendi yazdığı serbest
    /// metinde "AKDENİZ/MERSİN" bir kez daha geçiyor.
    /// NOT: bu belirli adreste posta kodu (`\d{5}`) yalnızca SONDAKİ
    /// "33000 Akdeniz/Mersin" önünde bulunduğu için kalıp fiilen tek kez
    /// eşleşiyor — yine de "son eşleşmeyi al" tasarımının dayandığı veri
    /// biçimi budur; ayrım gücünü kanıtlayan asıl test
    /// aşağıdaki `parse_district_prefers_the_last_match_over_the_first`.
    #[test]
    fn parse_district_handles_an_address_with_a_repeated_district_name() {
        let address = "ÖRNEK SANAYİ AŞ. NUMUNE MAH. DENEME BÖLGE 7.CADDE NO:9 AKDENİZ/MERSİN \
             ÖRNEK SANAYİ AŞ. NUMUNE MAH. DENEME BÖLGE 7.CADDE NO:9, Numune, \
             33000 Akdeniz/Mersin, Türkiye";
        assert_eq!(parse_district(address), Some("Akdeniz".to_string()));
    }

    /// Sondaki eşleşmenin ZORUNLU olduğunu kanıtlayan ayırt edici test:
    /// adreste iki tam eşleşme var (Kadıköy/İstanbul ve Akdeniz/Mersin);
    /// ilk eşleşme alınsaydı yanlış ilçe ("Kadıköy") dönerdi.
    #[test]
    fn parse_district_prefers_the_last_match_over_the_first() {
        let address = "Merkez, 34000 Kadıköy/İstanbul eski kayıt, 33000 Akdeniz/Mersin, Türkiye";
        assert_eq!(parse_district(address), Some("Akdeniz".to_string()));
    }

    /// Posta kodu hiç yoksa ayrıştırma başarısız olmalı; hata değil, `None`.
    #[test]
    fn parse_district_returns_none_without_a_postal_code() {
        let address = "Örnek Mahallesi, Deneme Caddesi No:5, Mersin, Türkiye";
        assert_eq!(parse_district(address), None);
    }

    #[test]
    fn parse_district_returns_none_for_an_empty_address() {
        assert_eq!(parse_district(""), None);
    }

    /// Kısa biçim: adresin TAMAMI yalnızca "posta kodu İlçe/İl" olabilir.
    #[test]
    fn parse_district_handles_the_short_form_without_a_trailing_comma() {
        assert_eq!(parse_district("33000 Akdeniz/Mersin"), Some("Akdeniz".to_string()));
    }

    /// Vue tarafına giden değer TEK bir yazımda olmalı: büyük/küçük harf
    /// farkı gruplamayı bozmamalı.
    #[test]
    fn normalize_place_name_unifies_different_casings() {
        assert_eq!(normalize_place_name("AKDENİZ"), normalize_place_name("Akdeniz"));
        assert_eq!(normalize_place_name("akdeniz"), "Akdeniz");
    }

    /// Türkçe İ/I ayrımı: noktalı büyük İ küçükte noktalı i'ye, noktasız
    /// büyük I küçükte noktasız ı'ya döner (ASCII `to_lowercase` bunu yanlış
    /// yapar).
    #[test]
    fn normalize_place_name_respects_turkish_dotted_and_dotless_letters() {
        assert_eq!(normalize_place_name("İSTANBUL"), "İstanbul");
        assert_eq!(normalize_place_name("IĞDIR"), "Iğdır");
    }

    // --- parse_neighborhood ---

    /// Harita dışa aktarımının standart biçimi: ilk parça mahalle, "Mahallesi"
    /// eki atılır.
    #[test]
    fn parse_neighborhood_takes_the_first_part_and_drops_the_suffix() {
        let address = "Deneme Mahallesi, Örnek Sokak No:5, 33110 Yenişehir/Mersin, Türkiye";
        assert_eq!(parse_neighborhood(address), Some("Deneme".to_string()));
    }

    #[test]
    fn parse_neighborhood_drops_abbreviated_suffixes_case_insensitively() {
        assert_eq!(
            parse_neighborhood("ÖRNEK MAH., Test Cad. 7, 33110 Yenişehir/Mersin, Türkiye"),
            Some("Örnek".to_string())
        );
        assert_eq!(
            parse_neighborhood("örnek mah, Test Cad. 7, 33110 Yenişehir/Mersin, Türkiye"),
            Some("Örnek".to_string())
        );
    }

    /// İlçeyle AYNI Türkçe normalizasyon: 'I' -> 'ı', 'İ' -> 'i'.
    #[test]
    fn parse_neighborhood_normalizes_with_turkish_casing() {
        let address = "IŞIKLAR MAHALLESİ, Test Sk. 3, 33110 Yenişehir/Mersin, Türkiye";
        assert_eq!(parse_neighborhood(address), Some("Işıklar".to_string()));
    }

    #[test]
    fn parse_neighborhood_keeps_multi_word_names() {
        let address = "Yeni Deneme Mah., Test Sokak, 33110 Yenişehir/Mersin, Türkiye";
        assert_eq!(parse_neighborhood(address), Some("Yeni Deneme".to_string()));
    }

    #[test]
    fn parse_neighborhood_works_without_a_street_part() {
        let address = "Deneme, 33110 Yenişehir/Mersin, Türkiye";
        assert_eq!(parse_neighborhood(address), Some("Deneme".to_string()));
    }

    #[test]
    fn parse_neighborhood_returns_none_when_the_first_part_is_the_postal_part() {
        assert_eq!(parse_neighborhood("33110 Yenişehir/Mersin, Türkiye"), None);
        assert_eq!(parse_neighborhood("33130 Akdeniz/Mersin"), None);
    }

    #[test]
    fn parse_neighborhood_returns_none_without_a_postal_district_part() {
        assert_eq!(parse_neighborhood("Deneme Mahallesi, Test Sokak No:1, Mersin"), None);
        assert_eq!(parse_neighborhood(""), None);
    }

    /// İlk parça sokak/cadde/numara ise mahalle sayılmaz.
    #[test]
    fn parse_neighborhood_returns_none_when_the_first_part_is_a_street_or_number() {
        let tail = ", 33110 Yenişehir/Mersin, Türkiye";
        for first in [
            "Test Sk. 4",
            "Test SOK",
            "Test Sokak",
            "Atlas Cad.",
            "Atlas Cd",
            "Atlas Caddesi",
            "Atlas Blv.",
            "Atlas Bulv.",
            "Atlas Bulvarı",
            "ATLAS BULVARI",
            "No:5",
            "Test No 5",
            "1234 Test",
        ] {
            assert_eq!(parse_neighborhood(&format!("{first}{tail}")), None, "{first}");
        }
    }

    /// 1-2 haneli sayı sokak/numara sayılmaz (ör. "2. Etap"); 3+ hane sayılır.
    #[test]
    fn parse_neighborhood_allows_short_numbers_but_not_three_digit_ones() {
        let tail = ", 33110 Yenişehir/Mersin, Türkiye";
        assert_eq!(parse_neighborhood(&format!("Deneme 2{tail}")), Some("Deneme 2".to_string()));
        assert_eq!(parse_neighborhood(&format!("Deneme 200{tail}")), None);
    }

    /// Yalnız "Mahallesi" yazan parçadan ad kalmaz.
    #[test]
    fn parse_neighborhood_returns_none_when_only_the_suffix_remains() {
        assert_eq!(
            parse_neighborhood("Mahallesi, Test Sokak, 33110 Yenişehir/Mersin, Türkiye"),
            None
        );
    }

    #[test]
    fn parse_district_province_returns_both_normalized() {
        assert_eq!(
            parse_district_province("Deneme, 33110 YENİŞEHİR/MERSİN, Türkiye"),
            Some(("Yenişehir".to_string(), "Mersin".to_string()))
        );
        assert_eq!(parse_district_province("Mersin"), None);
    }
}
