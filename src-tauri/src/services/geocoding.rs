use crate::db::companies;
use crate::domain::models::Company;
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
    pub resolved: i64,
    pub failed: i64,
    pub skipped: i64,
    pub results: Vec<GeocodeResult>,
    /// Kullanıcıya gösterilecek Türkçe uyarılar (ör. ağ hatası nedeniyle
    /// başarısız olan aramalar).
    pub warnings: Vec<String>,
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

/// Konumu olmayan (enlem veya boylamı eksik) tüm işletmeleri sırayla çözer.
/// OSM kullanım politikası saniyede en fazla 1 istek gerektirdiği için art
/// arda gelen istekler arasına `RATE_LIMIT_DELAY` kadar bekleme konur.
/// Başarısız bir arama toplu işlemi durdurmaz: kayıt 'failed' olarak
/// işaretlenip sıradaki işletmeye geçilir.
pub async fn geocode_pending(pool: &SqlitePool) -> AppResult<GeocodeSummary> {
    let client = reqwest::Client::new();
    let all_companies = companies::list(pool).await?;

    let mut summary = GeocodeSummary::default();
    let mut is_first_request = true;

    for company in &all_companies {
        if !needs_geocoding(company) {
            summary.skipped += 1;
            summary.results.push(GeocodeResult {
                company_id: company.id,
                company_name: company.name.clone(),
                status: GeocodeOutcome::Skipped,
            });
            continue;
        }

        // Saniyede bir istek: ilk istekten önce beklemeye gerek yok,
        // sonraki her istekten önce bekleriz.
        if is_first_request {
            is_first_request = false;
        } else {
            sleep(RATE_LIMIT_DELAY).await;
        }

        match geocode_address(&client, &company.address_text).await {
            Ok(Some((latitude, longitude))) => {
                companies::set_location(pool, company.id, latitude, longitude, "resolved")
                    .await?;
                summary.resolved += 1;
                summary.results.push(GeocodeResult {
                    company_id: company.id,
                    company_name: company.name.clone(),
                    status: GeocodeOutcome::Resolved { latitude, longitude },
                });
            }
            Ok(None) => {
                companies::mark_geocode_failed(pool, company.id).await?;
                summary.failed += 1;
                summary.results.push(GeocodeResult {
                    company_id: company.id,
                    company_name: company.name.clone(),
                    status: GeocodeOutcome::Failed,
                });
            }
            Err(err) => {
                companies::mark_geocode_failed(pool, company.id).await?;
                summary.failed += 1;
                summary
                    .warnings
                    .push(format!("{}: konum çözümlenemedi ({err})", company.name));
                summary.results.push(GeocodeResult {
                    company_id: company.id,
                    company_name: company.name.clone(),
                    status: GeocodeOutcome::Failed,
                });
            }
        }
    }

    Ok(summary)
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

        let located = companies::set_location(&pool, created.id, 36.8, 34.6, "manual")
            .await
            .unwrap();
        assert!(!needs_geocoding(&located));
    }
}
