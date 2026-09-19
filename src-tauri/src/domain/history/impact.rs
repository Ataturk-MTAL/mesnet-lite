//! Etki özeti — bir kararın kullanıcıya önizlemede gösterilecek özeti
//! (spec §8, "ImpactSummary"). Saf veri taşır; karar mantığı `policy.rs` ve
//! `decide/`'dadır.

use chrono::NaiveDate;
use serde::Serialize;

/// `preview_change`/`commit_change` yanıtının etki bölümü. Dört bölüm halinde
/// gösterilir: birincil değişiklik, otomatik zincir, uyarılar, bildirimler.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactSummary {
    pub effective_date: NaiveDate,
    pub is_planning: bool,
    /// Anlık görüntü akışlarında (saat, yük, program) bu değişiklik, öznenin
    /// bir SONRAKİ kaydına kadar mı geçerli? Doluysa o tarih (spec §5.2).
    pub shadowed_until: Option<NaiveDate>,
    pub primary: Vec<ImpactLine>,
    pub automatic: Vec<ImpactLine>,
    pub warnings: Vec<ImpactWarning>,
    pub notices: Vec<ImpactNotice>,
}

impl ImpactSummary {
    /// Boş bir özetle başlar; `decide` alt fonksiyonları satırları doldurur.
    pub fn empty(effective_date: NaiveDate, is_planning: bool) -> Self {
        Self {
            effective_date,
            is_planning,
            shadowed_until: None,
            primary: Vec::new(),
            automatic: Vec::new(),
            warnings: Vec::new(),
            notices: Vec::new(),
        }
    }
}

/// Etki özetindeki tek bir olay satırı (birincil ya da otomatik).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactLine {
    pub kind: String,
    pub stream: String,
    pub subject_id: i64,
    pub subject_label: String,
    pub effective_date: NaiveDate,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// Engelleyici olmayan ama dikkat çekilmesi gereken durum kodu (spec §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WarningCode {
    /// Kilitli satır, katlama anında sınırlanmadığı için tavanın üstünde kaldı.
    LockedAboveCap,
    /// Öğretmenin haftalık ek ders kapasitesi aşıldı (MADDE 15/2 + 6/1-c).
    CapacityExceeded,
    /// Bir günde 8 saatlik sınır aşıldı (OÖKY MADDE 88).
    DailyCapExceeded,
    /// Program değişince zorlanmamış bir blok artık boş saatlerin dışında.
    BlockOutsideFreeSlots,
    /// Mesafe ya da kural eksik olduğu için tavan hesaplanamadı.
    CapUnknown,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactWarning {
    pub code: WarningCode,
    pub message: String,
    pub subject_label: String,
    pub from_date: NaiveDate,
    pub to_date: Option<NaiveDate>,
}

/// Bilgilendirme amaçlı, engelleyici olmayan not kodu (spec §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NoticeCode {
    /// Tavan yükseldi; saat kendiliğinden artmaz, elle yükseltilebilir.
    CapIncreased,
    /// Saat, otomatik düşürüldükten sonra şimdi tavanın altında kaldı.
    ReducedBelowCap,
    /// Yerinde oluşturulan işletmenin saati ve ataması henüz girilmedi.
    NewCompanyNeedsSetup,
    /// Yürürlük tarihi gelecekte; değişiklik henüz etkili değil.
    FutureDated,
    /// Bu değişiklik, öznenin bir sonraki kaydına kadar geçerli (gölgeli).
    Shadowed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactNotice {
    pub code: NoticeCode,
    pub message: String,
    pub subject_label: String,
    pub date: NaiveDate,
}
