use serde::{Deserialize, Serialize};

/// İşletmenin konum bilgisinin durumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GeocodeStatus {
    Pending,
    Resolved,
    Failed,
    Manual,
}

/// Şeflik görevi. Saat değeri buradan türetilir, veritabanında saklanmaz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChiefType {
    None,
    WorkshopLab,
    Department,
}

impl ChiefType {
    /// MADDE 6/4: bölüm şefleri için haftada 10, atölye ve laboratuvar şefleri
    /// için haftada 6 saat. Bu saatler azamî ek ders tavanının İÇİNDE verilir,
    /// üstüne eklenmez.
    pub fn weekly_hours(self) -> i64 {
        match self {
            ChiefType::None => 0,
            ChiefType::WorkshopLab => 6,
            ChiefType::Department => 10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EmploymentType {
    Tenured,
    Contracted,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Company {
    pub id: i64,
    pub name: String,
    pub contact_first_name: String,
    pub contact_last_name: String,
    pub phone: String,
    pub email: String,
    pub address_text: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub geocode_status: String,
    /// TEK YÖN yol mesafesi. Koordinatlardan hesaplanmaz; CSV'den gelir veya
    /// elle girilir. Saat tavanı kuralları iki katını kullanır.
    pub one_way_distance_km: Option<f64>,
    /// İşletmenin bulunduğu ilçe. Atama tahtasında atanmamış işletmelerin
    /// ilçe bazlı gruplanabilmesi için `address_text`ten ayrı bir sütunda
    /// tutulur (bkz. `domain::address::parse_district`, migration 0010).
    /// Adresten türetilemezse boş kalır; bu bir hata değildir.
    pub district: String,
    pub notes: String,
    pub created_at: String,
    pub updated_at: String,
}

impl Company {
    /// Saat tavanı kurallarında kullanılan gidiş-dönüş mesafesi.
    /// Mesafe bilinmiyorsa None döner; sıfır varsayılmaz, çünkü sıfır işletmeyi
    /// en düşük tavanlı aralığa düşürürdü.
    pub fn round_trip_distance_km(&self) -> Option<f64> {
        self.one_way_distance_km.map(|km| km * 2.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCompany {
    pub name: String,
    pub contact_first_name: String,
    pub contact_last_name: String,
    pub phone: String,
    pub email: String,
    pub address_text: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub one_way_distance_km: Option<f64>,
    /// Kullanıcı ilçeyi elle girdiyse (boş olmayan bir değer gönderdiyse) bu
    /// değer KORUNUR; boşsa `db::companies` adresten türetir (bkz.
    /// `domain::address::parse_district`). `#[serde(default)]`: Vue tarafı
    /// bu alanı henüz göndermiyorsa (geçiş sürecinde) istek yine de kabul
    /// edilir.
    #[serde(default)]
    pub district: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Student {
    pub id: i64,
    pub first_name: String,
    pub last_name: String,
    pub student_no: Option<String>,
    pub grade: String,
    pub branch: String,
    pub company_id: Option<i64>,
    pub submitted_at: Option<String>,
    /// Eğitim-öğretim yılı. Öğrenci listesi her yıl yenilenir.
    pub term: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewStudent {
    pub first_name: String,
    pub last_name: String,
    pub student_no: Option<String>,
    pub grade: String,
    pub branch: String,
    pub company_id: Option<i64>,
    pub submitted_at: Option<String>,
    pub term: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Teacher {
    pub id: i64,
    pub first_name: String,
    pub last_name: String,
    pub registry_no: String,
    pub field: String,
    /// JSON dizi olarak saklanır, ör. ["Elektronik Haberleşme"]
    pub branches: String,
    pub employment_type: String,
    pub base_hours: i64,
    pub max_extra_hours: i64,
    pub other_extra_hours: i64,
    pub chief_type: String,
    pub is_active: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTeacher {
    pub first_name: String,
    pub last_name: String,
    pub registry_no: String,
    pub field: String,
    pub branches: Vec<String>,
    pub employment_type: String,
    pub base_hours: i64,
    pub max_extra_hours: i64,
    pub other_extra_hours: i64,
    pub chief_type: String,
    pub is_active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_company(one_way_distance_km: Option<f64>) -> Company {
        Company {
            id: 1,
            name: "Test İşletme A".into(),
            contact_first_name: String::new(),
            contact_last_name: String::new(),
            phone: String::new(),
            email: String::new(),
            address_text: "Test adres".into(),
            latitude: None,
            longitude: None,
            geocode_status: "pending".into(),
            one_way_distance_km,
            district: String::new(),
            notes: String::new(),
            created_at: "2026-09-18T00:00:00Z".into(),
            updated_at: "2026-09-18T00:00:00Z".into(),
        }
    }

    /// MADDE 6/4: bölüm şefi haftada 10, atölye ve laboratuvar şefi haftada 6 saat.
    #[test]
    fn chief_type_weekly_hours_follows_madde_6_4() {
        assert_eq!(ChiefType::Department.weekly_hours(), 10);
        assert_eq!(ChiefType::WorkshopLab.weekly_hours(), 6);
        assert_eq!(ChiefType::None.weekly_hours(), 0);
    }

    /// Saat tavanı kuralları gidiş-dönüş mesafe kullanır: tek yönün iki katı.
    #[test]
    fn round_trip_distance_doubles_one_way() {
        assert_eq!(
            sample_company(Some(6.8)).round_trip_distance_km(),
            Some(13.6)
        );
    }

    /// Mesafe bilinmiyorsa gidiş-dönüş de bilinmez; sıfır varsayılmaz.
    #[test]
    fn round_trip_distance_is_none_when_one_way_missing() {
        assert_eq!(sample_company(None).round_trip_distance_km(), None);
    }
}
