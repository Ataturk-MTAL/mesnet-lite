//! Dönem tarihleri ve yürürlük tarihi çözümü — spec §5.1.
//!
//! Ek ders puantajı ay sonunda mutemede hazırlanır ve en geç ayın 3'ünde
//! ilçeye gönderilir (spec §2 madde 3). Bu yüzden içinde bulunulan aydan
//! önceki bir tarihe geçmişe dönük giriş yapılamaz: puantaj çoktan gitmiştir.

use chrono::{Datelike, FixedOffset, NaiveDate, NaiveDateTime, Utc};

use super::history::rejection::{Rejection, RejectionCode};
use crate::error::{AppError, AppResult};

/// Türkiye 2016'dan beri tek ve sabit UTC+3 dilimindedir; `chrono-tz` gibi
/// bir bağımlılık gerekmez (plan "Genel Kısıtlar").
const TURKEY_UTC_OFFSET_SECONDS: i32 = 3 * 3600;

/// Bir eğitim-öğretim dönemi (ör. `"2026-2027/1"`) için başlangıç/bitiş tarihi.
#[derive(Debug, Clone, PartialEq)]
pub struct TermDates {
    pub term: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// Kullanıcı tarafından onaylanmadıysa `false`. Göçten (0006) gelen
    /// varsayılan tarihler onaylanmamış sayılır (spec §9 adım 1).
    pub dates_confirmed: bool,
}

impl TermDates {
    /// `term` dize biçiminden varsayılan dönem tarihlerini türetir.
    ///
    /// - `"YYYY-YYYY/1"` → `YYYY-09-01`–`(YYYY+1)-01-31` (güz dönemi).
    /// - `"YYYY-YYYY/2"` → `(YYYY+1)-02-01`–`(YYYY+1)-06-30` (bahar dönemi).
    /// - Biçimi bozuk bir dönem adı hiçbir tarihi dışlamamak için en geniş
    ///   aralığı alır: `2000-01-01`–`2099-12-31` (spec §9 adım 1, "biçimi
    ///   bozuk eski dönemler").
    pub fn default_for(term: &str) -> TermDates {
        let (start, end) = match parse_term_year_and_half(term) {
            Some((year, 1)) => (ymd(year, 9, 1), ymd(year + 1, 1, 31)),
            Some((year, 2)) => (ymd(year + 1, 2, 1), ymd(year + 1, 6, 30)),
            _ => (ymd(2000, 1, 1), ymd(2099, 12, 31)),
        };
        TermDates {
            term: term.to_string(),
            start,
            end,
            dates_confirmed: false,
        }
    }

    /// Dönem henüz başlamadıysa planlama evresindeyizdir; tarih zorunlu
    /// değildir ve ay penceresi uygulanmaz (spec §5.1).
    pub fn is_planning(&self, today: NaiveDate) -> bool {
        today < self.start
    }

    /// Varsayılan "tarihteki durum" günü: bugün, dönem aralığına sıkıştırılır.
    pub fn default_as_of(&self, today: NaiveDate) -> NaiveDate {
        today.clamp(self.start, self.end)
    }

    /// Yürürlük tarihi için izin verilen en erken gün.
    ///
    /// Planlamada dönem başlangıcından önceye gidilemez. Dönem başladıysa
    /// önceki ayın puantajı kapandığı için bu ayın 1'inden önceye
    /// gidilemez — ikisinin büyüğü bağlayıcıdır.
    pub fn earliest_allowed(&self, today: NaiveDate) -> NaiveDate {
        if self.is_planning(today) {
            self.start
        } else {
            self.start.max(first_of_month(today))
        }
    }

    /// İstenen yürürlük tarihini spec §5.1 kurallarına göre çözer.
    ///
    /// Planlamada tarih boş bırakılabilir; boşsa dönem başlangıcı kullanılır.
    /// Dönem başladıysa tarih zorunludur ve `[earliest_allowed, end]`
    /// aralığında olmalıdır; önceki aya düşerse bu ayın 1'i önerilir.
    pub fn resolve_effective_date(
        &self,
        requested: Option<NaiveDate>,
        today: NaiveDate,
    ) -> Result<NaiveDate, Rejection> {
        if self.is_planning(today) {
            let date = requested.unwrap_or(self.start);
            return self.require_within_term(date);
        }

        let date = requested.ok_or_else(|| {
            Rejection::new(
                RejectionCode::EffectiveDateRequired,
                "Dönem başladı; bu işlem için bir geçerlilik tarihi girilmelidir.".to_string(),
            )
        })?;

        if date < self.earliest_allowed(today) {
            return Err(Rejection::new(
                RejectionCode::PreviousMonthClosed,
                "Önceki ayın ek ders puantajı ilçeye gönderildi; bu tarih artık \
                 değiştirilemez. Bu ayın 1'inden itibaren bir tarih girin."
                    .to_string(),
            )
            .with_suggested_date(first_of_month(today)));
        }

        self.require_within_term(date)
    }

    /// Tarihin `[start, end]` dışında kaldığı ortak kontrol.
    fn require_within_term(&self, date: NaiveDate) -> Result<NaiveDate, Rejection> {
        if date < self.start || date > self.end {
            return Err(Rejection::new(
                RejectionCode::OutOfTerm,
                format!(
                    "Tarih dönem aralığının ({} – {}) dışında.",
                    self.start, self.end
                ),
            ));
        }
        Ok(date)
    }
}

/// Verilen günün bulunduğu ayın 1'i.
pub fn first_of_month(d: NaiveDate) -> NaiveDate {
    ymd(d.year(), d.month(), 1)
}

/// Sistem saatinden Türkiye yerel takvim gününü hesaplar (sabit UTC+3).
/// `recorded_at` bunun aksine UTC olarak saklanır (plan "Genel Kısıtlar").
pub fn today_local() -> NaiveDate {
    let offset = FixedOffset::east_opt(TURKEY_UTC_OFFSET_SECONDS)
        .expect("sabit +03:00 ofseti her zaman geçerlidir");
    Utc::now().with_timezone(&offset).date_naive()
}

/// `today_local`in saat bileşenli hâli: Türkiye yerel tarih-saati (sabit
/// UTC+3). `services::versions::create_version`in `created_at` damgası için
/// kullanılır — sürüm listesi kullanıcıya günün yalnızca tarihini değil saatini
/// de göstermeli, çünkü aynı günde birden çok sürüm alınabilir.
pub fn now_local() -> NaiveDateTime {
    let offset = FixedOffset::east_opt(TURKEY_UTC_OFFSET_SECONDS)
        .expect("sabit +03:00 ofseti her zaman geçerlidir");
    Utc::now().with_timezone(&offset).naive_local()
}

/// `'YYYY-MM-DD'` dışındaki her girdiyi sınırda reddeder (dış veri asla
/// doğrulanmadan kullanılmaz).
pub fn parse_date(raw: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .map_err(|_| AppError::Validation(format!("Geçersiz tarih: '{raw}'. Biçim YYYY-MM-DD olmalıdır.")))
}

fn ymd(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("sabit takvim değerleri her zaman geçerlidir")
}

/// `"YYYY-YYYY/N"` biçimini ayrıştırır; uymuyorsa `None` döner (çağıran bunu
/// "biçimi bozuk" olarak yorumlar).
fn parse_term_year_and_half(term: &str) -> Option<(i32, u8)> {
    let (years, half_str) = term.split_once('/')?;
    let half: u8 = half_str.parse().ok()?;
    if half != 1 && half != 2 {
        return None;
    }
    let (start_year_str, end_year_str) = years.split_once('-')?;
    let start_year: i32 = start_year_str.parse().ok()?;
    let end_year: i32 = end_year_str.parse().ok()?;
    if end_year != start_year + 1 {
        return None;
    }
    Some((start_year, half))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_for_first_term_spans_september_to_january() {
        let dates = TermDates::default_for("2026-2027/1");
        assert_eq!(dates.start, ymd(2026, 9, 1));
        assert_eq!(dates.end, ymd(2027, 1, 31));
        assert!(!dates.dates_confirmed);
    }

    #[test]
    fn default_for_second_term_spans_february_to_june() {
        let dates = TermDates::default_for("2026-2027/2");
        assert_eq!(dates.start, ymd(2027, 2, 1));
        assert_eq!(dates.end, ymd(2027, 6, 30));
    }

    #[test]
    fn default_for_malformed_term_is_wide() {
        let dates = TermDates::default_for("bozuk-dönem-adı");
        assert_eq!(dates.start, ymd(2000, 1, 1));
        assert_eq!(dates.end, ymd(2099, 12, 31));
    }

    #[test]
    fn planning_defaults_missing_date_to_start() {
        let dates = TermDates::default_for("2026-2027/1");
        let today = ymd(2026, 8, 15); // dönem henüz başlamadı
        assert!(dates.is_planning(today));
        assert_eq!(dates.resolve_effective_date(None, today).unwrap(), dates.start);
    }

    #[test]
    fn running_term_requires_a_date() {
        let dates = TermDates::default_for("2026-2027/1");
        let today = ymd(2026, 10, 10);
        let rejection = dates.resolve_effective_date(None, today).unwrap_err();
        assert_eq!(rejection.code, RejectionCode::EffectiveDateRequired);
    }

    /// today 2026-11-10, istek 2026-10-28 → önceki ayın puantajı zaten
    /// gönderilmiş, `suggested_date` bu ayın 1'idir (2026-11-01).
    #[test]
    fn date_before_first_of_current_month_is_rejected_with_suggestion() {
        let dates = TermDates::default_for("2026-2027/1");
        let today = ymd(2026, 11, 10);
        let rejection = dates
            .resolve_effective_date(Some(ymd(2026, 10, 28)), today)
            .unwrap_err();
        assert_eq!(rejection.code, RejectionCode::PreviousMonthClosed);
        assert_eq!(rejection.suggested_date, Some(ymd(2026, 11, 1)));
    }

    /// today 2026-10-25, istek 2026-10-10 → aynı ay içinde geçmişe dönük
    /// giriş serbesttir (spec §2 madde 3 örneği).
    #[test]
    fn date_in_current_month_before_today_is_accepted() {
        let dates = TermDates::default_for("2026-2027/1");
        let today = ymd(2026, 10, 25);
        let resolved = dates
            .resolve_effective_date(Some(ymd(2026, 10, 10)), today)
            .unwrap();
        assert_eq!(resolved, ymd(2026, 10, 10));
    }

    #[test]
    fn future_date_inside_term_is_accepted() {
        let dates = TermDates::default_for("2026-2027/1");
        let today = ymd(2026, 10, 25);
        let resolved = dates
            .resolve_effective_date(Some(ymd(2026, 12, 1)), today)
            .unwrap();
        assert_eq!(resolved, ymd(2026, 12, 1));
    }

    #[test]
    fn date_outside_term_is_rejected() {
        let dates = TermDates::default_for("2026-2027/1");
        let today = ymd(2026, 10, 25);
        let rejection = dates
            .resolve_effective_date(Some(ymd(2027, 3, 1)), today)
            .unwrap_err();
        assert_eq!(rejection.code, RejectionCode::OutOfTerm);
    }

    /// Dönem başlangıcı ayın ortasındaysa (`2026-09-15`) ve bugün de aynı
    /// ayda ama başlangıçtan önce bir günse, en erken tarih yine başlangıçtır.
    #[test]
    fn earliest_allowed_never_precedes_term_start() {
        let dates = TermDates {
            term: "2026-2027/1".into(),
            start: ymd(2026, 9, 15),
            end: ymd(2027, 1, 31),
            dates_confirmed: true,
        };
        let today = ymd(2026, 9, 20);
        assert_eq!(dates.earliest_allowed(today), ymd(2026, 9, 15));
    }

    #[test]
    fn parse_date_rejects_malformed_string() {
        assert!(parse_date("28.10.2026").is_err());
        assert!(parse_date("garbage").is_err());
        assert_eq!(parse_date("2026-10-28").unwrap(), ymd(2026, 10, 28));
    }
}
