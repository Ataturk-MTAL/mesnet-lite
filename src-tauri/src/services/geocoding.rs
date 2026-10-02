use crate::db::companies;
use crate::domain::address::parse_district_province;
use crate::domain::models::{geocode_precision, Company};
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::time::Duration;
use tokio::time::sleep;

/// OSM Nominatim kullanım politikası saniyede en fazla 1 istek izin verir.
/// Bu sınırın aşılması IP'nin engellenmesine yol açabileceği için istekler
/// arasına bilinçli olarak bu kadar gecikme konur.
const RATE_LIMIT_DELAY: Duration = Duration::from_secs(1);

/// Nominatim kullanım politikasının zorunlu kıldığı User-Agent başlığı.
/// Boş veya jenerik bir değer istekleri reddettirebilir.
const USER_AGENT: &str = "MESNET.Lite/0.1 (okul koordinatorluk uygulamasi)";

const NOMINATIM_SEARCH_URL: &str = "https://nominatim.openstreetmap.org/search";

/// Bir işletmenin coğrafi kodlama sonucunun türü.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GeocodeOutcome {
    Resolved { latitude: f64, longitude: f64 },
    Failed,
    Skipped,
}

/// Tek bir işletme için coğrafi kodlama sonucu.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeocodeResult {
    pub company_id: i64,
    pub company_name: String,
    pub status: GeocodeOutcome,
}

/// Toplu coğrafi kodlama çalıştırmasının özeti.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GeocodeSummary {
    /// Tam adresten VE mahalle düzeyinden çözülenlerin toplamı.
    pub resolved: i64,
    /// `resolved`ın içinden yalnız mahalle düzeyinde (yaklaşık) çözülenler.
    pub approximate: i64,
    pub failed: i64,
    pub skipped: i64,
    pub results: Vec<GeocodeResult>,
    /// Kullanıcıya gösterilecek Türkçe uyarılar (ör. ağ hatası nedeniyle
    /// başarısız olan aramalar).
    pub warnings: Vec<String>,
}

impl GeocodeSummary {
    /// Çözülen bir işletmeyi sayar; mahalle düzeyindeki sonuç hem `resolved`
    /// hem `approximate` içinde sayılır.
    fn record_resolved(&mut self, precision: &str) {
        self.resolved += 1;
        if precision == geocode_precision::NEIGHBORHOOD {
            self.approximate += 1;
        }
    }
}

/// Nominatim `/search` yanıtındaki tek sonuç satırı. Nominatim `lat`/`lon`
/// değerlerini STRING olarak döner; doğrudan `f64` bekleyip deserialize
/// etmeye çalışmak klasik bir hatadır, bu yüzden burada `String` olarak
/// okunup ardından ayrıca sayıya çevrilir.
#[derive(Debug, Deserialize)]
struct NominatimEntry {
    lat: String,
    lon: String,
}

/// Nominatim yanıt gövdesini ayrıştırır. Saf fonksiyondur, ağa hiç dokunmaz;
/// bu sayede ağ çağrısı yapılmadan test edilebilir. Boş dizi (`[]`) veya
/// bozuk JSON durumunda panik atmadan `None` döner.
fn parse_response(body: &str) -> Option<(f64, f64)> {
    let entries: Vec<NominatimEntry> = serde_json::from_str(body).ok()?;
    let first = entries.into_iter().next()?;
    let latitude = first.lat.parse::<f64>().ok()?;
    let longitude = first.lon.parse::<f64>().ok()?;
    Some((latitude, longitude))
}

/// Bir işletmenin coğrafi kodlamaya ihtiyacı olup olmadığını belirler.
/// Enlem VE boylamı zaten kayıtlıysa işletme yeniden kodlanmaz.
fn needs_geocoding(company: &Company) -> bool {
    company.latitude.is_none() || company.longitude.is_none()
}

/// Tek bir adresi Nominatim üzerinden çözer. Ağ hatası (`Err`) ile "sonuç
/// bulunamadı" (`Ok(None)`) ayrı durumlardır: ilki bağlantı/istek sorunudur,
/// ikincisi verilen adresin Nominatim'de eşleşmediği anlamına gelir.
pub async fn geocode_address(
    client: &reqwest::Client,
    address: &str,
) -> AppResult<Option<(f64, f64)>> {
    let response = client
        .get(NOMINATIM_SEARCH_URL)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .query(&[("format", "json"), ("limit", "1"), ("q", address)])
        .send()
        .await
        .map_err(|err| AppError::Geocoding(format!("Nominatim isteği başarısız: {err}")))?;

    let body = response
        .text()
        .await
        .map_err(|err| AppError::Geocoding(format!("Nominatim yanıtı okunamadı: {err}")))?;

    Ok(parse_response(&body))
}

/// Tek bir sorguyu çözen arama. Üretimde Nominatim (`NominatimGeocoder`),
/// testlerde ağa çıkmayan sahte bir uygulama kullanılır; basamak mantığı
/// (`resolve_location`) böylece ağsız sınanır.
trait Geocoder {
    async fn search(&mut self, query: &str) -> AppResult<Option<(f64, f64)>>;
}

/// Her isteği saniyede-bir sınırına uyarak yollayan Nominatim istemcisi.
/// Sınır BURADA uygulanır, çünkü bir işletme için tek yerine iki istek
/// atılabilir (tam adres + mahalle yedeği) ve her istek sayılır.
struct NominatimGeocoder {
    client: reqwest::Client,
    is_first_request: bool,
}

impl NominatimGeocoder {
    fn new() -> Self {
        Self { client: reqwest::Client::new(), is_first_request: true }
    }
}

impl Geocoder for NominatimGeocoder {
    async fn search(&mut self, query: &str) -> AppResult<Option<(f64, f64)>> {
        // İlk istekten önce beklemeye gerek yok, sonraki her istekten önce bekleriz.
        if self.is_first_request {
            self.is_first_request = false;
        } else {
            sleep(RATE_LIMIT_DELAY).await;
        }
        geocode_address(&self.client, query).await
    }
}

/// Çözülen konum ve kesinliği (`geocode_precision::*`).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Located {
    latitude: f64,
    longitude: f64,
    precision: &'static str,
}

/// Mahalle düzeyindeki yedek sorgu: `"{mahalle}, {ilçe}, {il}, Türkiye"`.
/// Uzun sokak/kapı numaralı adresler Nominatim'de sonuç vermezken bu kısa
/// biçim mahallelerin büyük çoğunluğunu `suburb` düzeyinde çözer (Issue #42).
/// Mahalle, ilçe ya da il bilinmiyorsa `None` — yedek basamak atlanır. İl,
/// ilçeyle aynı eşleşmeden (`parse_district_province`) gelir.
fn neighborhood_query(company: &Company) -> Option<String> {
    let neighborhood = company.neighborhood.trim();
    let district = company.district.trim();
    let (_, province) = parse_district_province(&company.address_text)?;
    if neighborhood.is_empty() || district.is_empty() {
        return None;
    }
    Some(format!("{neighborhood}, {district}, {province}, Türkiye"))
}

/// Basamaklı adres çözümü: (1) tam adres, (2) bulunamazsa mahalle düzeyi.
/// Yalnız "sonuç yok" (`Ok(None)`) 2. basamağa düşer; ağ hatası (`Err`)
/// düşmez, çünkü bağlantı yokken ikinci istek anlamsızdır.
async fn resolve_location(
    geocoder: &mut impl Geocoder,
    company: &Company,
) -> AppResult<Option<Located>> {
    if let Some((latitude, longitude)) = geocoder.search(&company.address_text).await? {
        return Ok(Some(Located { latitude, longitude, precision: geocode_precision::ADDRESS }));
    }
    let Some(query) = neighborhood_query(company) else {
        return Ok(None);
    };
    let found = geocoder.search(&query).await?;
    Ok(found.map(|(latitude, longitude)| Located {
        latitude,
        longitude,
        precision: geocode_precision::NEIGHBORHOOD,
    }))
}

/// Konumu olmayan (enlem veya boylamı eksik) tüm işletmeleri sırayla çözer.
/// OSM kullanım politikası saniyede en fazla 1 istek gerektirdiği için art
/// arda gelen istekler arasına `RATE_LIMIT_DELAY` kadar bekleme konur
/// (`NominatimGeocoder`). Başarısız bir arama toplu işlemi durdurmaz: kayıt
/// 'failed' olarak işaretlenip sıradaki işletmeye geçilir.
pub async fn geocode_pending(pool: &SqlitePool) -> AppResult<GeocodeSummary> {
    let mut geocoder = NominatimGeocoder::new();
    // `list` (süzülmüş) BİLEREK kullanılır: pasif bir işletme hiçbir yönetim
    // ekranında (harita, atama havuzu) görünmez, onun için Nominatim'e istek
    // atıp saniyede-bir sınırını harcamanın anlamı yok.
    let all_companies = companies::list(pool).await?;

    let mut summary = GeocodeSummary::default();
    for company in &all_companies {
        let (status, warning) = if needs_geocoding(company) {
            geocode_company(pool, &mut geocoder, company, &mut summary).await?
        } else {
            summary.skipped += 1;
            (GeocodeOutcome::Skipped, None)
        };
        if let Some(warning) = warning {
            summary.warnings.push(warning);
        }
        summary.results.push(GeocodeResult {
            company_id: company.id,
            company_name: company.name.clone(),
            status,
        });
    }

    Ok(summary)
}

/// Tek işletmeyi çözer, sonucu veritabanına yazar ve sayaçları günceller.
/// Ağ hatasında uyarı metnini de döner.
async fn geocode_company(
    pool: &SqlitePool,
    geocoder: &mut impl Geocoder,
    company: &Company,
    summary: &mut GeocodeSummary,
) -> AppResult<(GeocodeOutcome, Option<String>)> {
    match resolve_location(geocoder, company).await {
        Ok(Some(located)) => {
            companies::set_location(
                pool,
                company.id,
                located.latitude,
                located.longitude,
                "resolved",
                located.precision,
            )
            .await?;
            summary.record_resolved(located.precision);
            let outcome =
                GeocodeOutcome::Resolved { latitude: located.latitude, longitude: located.longitude };
            Ok((outcome, None))
        }
        Ok(None) => {
            companies::mark_geocode_failed(pool, company.id).await?;
            summary.failed += 1;
            Ok((GeocodeOutcome::Failed, None))
        }
        Err(err) => {
            companies::mark_geocode_failed(pool, company.id).await?;
            summary.failed += 1;
            let warning = format!("{}: konum çözümlenemedi ({err})", company.name);
            Ok((GeocodeOutcome::Failed, Some(warning)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;
    use crate::domain::models::NewCompany;

    const SAMPLE_RESPONSE: &str =
        r#"[{"place_id":123,"lat":"36.8121","lon":"34.6415","display_name":"Test"}]"#;

    /// Klasik hata: Nominatim `lat`/`lon` alanlarını STRING döner. Bu test
    /// bu alanların doğrudan sayı gibi değil, string olarak parse edildiğini
    /// doğrular.
    #[test]
    fn parse_response_extracts_lat_lon_from_string_fields() {
        assert_eq!(parse_response(SAMPLE_RESPONSE), Some((36.8121, 34.6415)));
    }

    #[test]
    fn parse_response_returns_none_for_empty_array() {
        assert_eq!(parse_response("[]"), None);
    }

    #[test]
    fn parse_response_returns_none_for_malformed_json() {
        assert_eq!(parse_response("bu gecerli json degil"), None);
        assert_eq!(parse_response(""), None);
    }

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    fn sample_input(name: &str) -> NewCompany {
        NewCompany {
            name: name.into(),
            contact_first_name: "Test".into(),
            contact_last_name: "Yetkili".into(),
            phone: "(500) 000-0000".into(),
            email: String::new(),
            address_text: "Test Mahallesi, Test Sokak No:1, Mersin".into(),
            latitude: None,
            longitude: None,
            one_way_distance_km: Some(6.8),
            district: String::new(),
            neighborhood: String::new(),
            notes: String::new(),
        }
    }

    /// Konumu olmayan işletme kodlamaya ihtiyaç duyar; `set_location`
    /// çağrıldıktan sonra artık ihtiyaç duymamalıdır (Skipped sınıflandırması
    /// bu koşula dayanır).
    #[tokio::test]
    async fn needs_geocoding_is_false_once_location_is_set() {
        let (_dir, pool) = test_pool().await;
        let created = companies::create(&pool, &sample_input("Test İşletme A"))
            .await
            .unwrap();
        assert!(needs_geocoding(&created));

        let located = companies::set_location(&pool, created.id, 36.8, 34.6, "manual", geocode_precision::MANUAL)
            .await
            .unwrap();
        assert!(!needs_geocoding(&located));
    }

    /// Sıralı yanıtları veren, gelen sorguları kaydeden sahte arama; ağa çıkmaz.
    struct FakeGeocoder {
        replies: Vec<AppResult<Option<(f64, f64)>>>,
        queries: Vec<String>,
    }

    impl FakeGeocoder {
        fn new(replies: Vec<AppResult<Option<(f64, f64)>>>) -> Self {
            Self { replies, queries: Vec::new() }
        }
    }

    impl Geocoder for FakeGeocoder {
        async fn search(&mut self, query: &str) -> AppResult<Option<(f64, f64)>> {
            self.queries.push(query.to_string());
            self.replies.remove(0)
        }
    }

    fn company_with(address: &str, neighborhood: &str, district: &str) -> Company {
        Company {
            id: 1,
            name: "Test İşletme A".into(),
            contact_first_name: String::new(),
            contact_last_name: String::new(),
            phone: String::new(),
            email: String::new(),
            address_text: address.into(),
            latitude: None,
            longitude: None,
            geocode_status: "pending".into(),
            geocode_precision: String::new(),
            one_way_distance_km: None,
            district: district.into(),
            neighborhood: neighborhood.into(),
            notes: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    const LONG_ADDRESS: &str =
        "Deneme Mahallesi, Örnek Sokak No:5, 33110 Yenişehir/Mersin, Türkiye";

    #[test]
    fn neighborhood_query_joins_neighborhood_district_and_province() {
        let company = company_with(LONG_ADDRESS, "Deneme", "Yenişehir");
        assert_eq!(
            neighborhood_query(&company),
            Some("Deneme, Yenişehir, Mersin, Türkiye".to_string())
        );
    }

    #[test]
    fn neighborhood_query_is_none_when_any_part_is_unknown() {
        // Mahalle yok.
        assert_eq!(neighborhood_query(&company_with(LONG_ADDRESS, "", "Yenişehir")), None);
        // İlçe yok.
        assert_eq!(neighborhood_query(&company_with(LONG_ADDRESS, "Deneme", "  ")), None);
        // İl adresten çıkarılamıyor (posta kodu parçası yok).
        assert_eq!(
            neighborhood_query(&company_with("Deneme Mah., Mersin", "Deneme", "Yenişehir")),
            None
        );
    }

    #[tokio::test]
    async fn resolve_location_uses_the_full_address_first_and_stops_when_found() {
        let company = company_with(LONG_ADDRESS, "Deneme", "Yenişehir");
        let mut geocoder = FakeGeocoder::new(vec![Ok(Some((36.8, 34.6)))]);

        let located = resolve_location(&mut geocoder, &company).await.unwrap().unwrap();

        assert_eq!(geocoder.queries, vec![LONG_ADDRESS.to_string()]);
        assert_eq!(located.precision, "address");
        assert_eq!((located.latitude, located.longitude), (36.8, 34.6));
    }

    #[tokio::test]
    async fn resolve_location_falls_back_to_the_neighborhood_when_the_address_has_no_result() {
        let company = company_with(LONG_ADDRESS, "Deneme", "Yenişehir");
        let mut geocoder = FakeGeocoder::new(vec![Ok(None), Ok(Some((36.7, 34.5)))]);

        let located = resolve_location(&mut geocoder, &company).await.unwrap().unwrap();

        assert_eq!(
            geocoder.queries,
            vec![LONG_ADDRESS.to_string(), "Deneme, Yenişehir, Mersin, Türkiye".to_string()]
        );
        assert_eq!(located.precision, "neighborhood");
        assert_eq!((located.latitude, located.longitude), (36.7, 34.5));
    }

    #[tokio::test]
    async fn resolve_location_returns_none_when_both_steps_find_nothing() {
        let company = company_with(LONG_ADDRESS, "Deneme", "Yenişehir");
        let mut geocoder = FakeGeocoder::new(vec![Ok(None), Ok(None)]);

        assert!(resolve_location(&mut geocoder, &company).await.unwrap().is_none());
        assert_eq!(geocoder.queries.len(), 2);
    }

    /// Ağ hatası (`Err`) ikinci basamağa DÜŞMEZ: bağlantı yokken ikinci bir
    /// istek atmak anlamsız ve saniyede-bir sınırını boşa harcar.
    #[tokio::test]
    async fn resolve_location_does_not_fall_back_after_a_network_error() {
        let company = company_with(LONG_ADDRESS, "Deneme", "Yenişehir");
        let mut geocoder =
            FakeGeocoder::new(vec![Err(AppError::Geocoding("bağlantı yok".into()))]);

        assert!(resolve_location(&mut geocoder, &company).await.is_err());
        assert_eq!(geocoder.queries.len(), 1);
    }

    #[tokio::test]
    async fn resolve_location_propagates_an_error_from_the_fallback_step() {
        let company = company_with(LONG_ADDRESS, "Deneme", "Yenişehir");
        let mut geocoder =
            FakeGeocoder::new(vec![Ok(None), Err(AppError::Geocoding("bağlantı yok".into()))]);

        assert!(resolve_location(&mut geocoder, &company).await.is_err());
    }

    #[tokio::test]
    async fn resolve_location_makes_a_single_request_without_a_neighborhood() {
        let company = company_with(LONG_ADDRESS, "", "Yenişehir");
        let mut geocoder = FakeGeocoder::new(vec![Ok(None)]);

        assert!(resolve_location(&mut geocoder, &company).await.unwrap().is_none());
        assert_eq!(geocoder.queries.len(), 1);
    }

    #[test]
    fn summary_tracks_approximate_inside_resolved() {
        let mut summary = GeocodeSummary::default();
        summary.record_resolved(geocode_precision::ADDRESS);
        summary.record_resolved(geocode_precision::NEIGHBORHOOD);
        assert_eq!((summary.resolved, summary.approximate), (2, 1));
    }
}
