use serde::{Deserialize, Serialize};

/// Okul/kurum tipi. MADDE 15/2 tavanını belirleyen iki eksenden biri.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstitutionType {
    /// Meslekî eğitim merkezi — MADDE 15/2-a
    VocationalCenter,
    /// Diğer okul ve kurumlar — MADDE 15/2-b
    Other,
}

impl InstitutionType {
    pub fn parse(raw: &str) -> Self {
        match raw {
            "vocational_center" => InstitutionType::VocationalCenter,
            _ => InstitutionType::Other,
        }
    }
}

/// MADDE 15/2 uyarınca haftalık işletmelerde meslek eğitimi tavanı.
///
/// a) Meslekî eğitim merkezlerinde;
///    1) Büyükşehir belediyesi sınırları içindeki ilçelerde 24 saati,
///    2) Diğer il ve ilçelerde 18 saati,
/// b) Diğer okul ve kurumlarda;
///    1) Büyükşehir belediyesi sınırları içindeki ilçelerde 20 saati,
///    2) Diğer il ve ilçelerde 16 saati,
/// geçmemek üzere verilir.
pub fn statutory_cap(institution_type: InstitutionType, is_metropolitan_district: bool) -> i64 {
    match (institution_type, is_metropolitan_district) {
        (InstitutionType::VocationalCenter, true) => 24,
        (InstitutionType::VocationalCenter, false) => 18,
        (InstitutionType::Other, true) => 20,
        (InstitutionType::Other, false) => 16,
    }
}

/// Bir öğretmenin koordinatörlük için kalan haftalık kapasitesi.
///
/// Şeflik saati (MADDE 6/4) ayrı bir bütçe DEĞİLDİR; azamî ek ders tavanının
/// içinde verilir, bu yüzden tavandan önceden düşülür. Sonuç, mevzuat tavanı ile
/// öğretmenin kalan bütçesinin küçüğüdür ve asla negatif olmaz.
pub fn coordinator_capacity(
    max_extra_hours: i64,
    chief_hours: i64,
    other_extra_hours: i64,
    statutory_cap: i64,
) -> i64 {
    let remaining_budget = max_extra_hours - chief_hours - other_extra_hours;
    statutory_cap.min(remaining_budget).max(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::ChiefType;

    #[test]
    fn statutory_cap_covers_all_four_cases_of_madde_15_2() {
        // MADDE 15/2-a-1 ve a-2
        assert_eq!(statutory_cap(InstitutionType::VocationalCenter, true), 24);
        assert_eq!(statutory_cap(InstitutionType::VocationalCenter, false), 18);
        // MADDE 15/2-b-1 ve b-2
        assert_eq!(statutory_cap(InstitutionType::Other, true), 20);
        assert_eq!(statutory_cap(InstitutionType::Other, false), 16);
    }

    #[test]
    fn institution_type_falls_back_to_other_for_unknown_value() {
        assert_eq!(InstitutionType::parse("bilinmeyen"), InstitutionType::Other);
        assert_eq!(
            InstitutionType::parse("vocational_center"),
            InstitutionType::VocationalCenter
        );
    }

    /// Atatürk MTAL, Toroslar/Mersin: diğer okul + büyükşehir ilçesi => tavan 20.
    /// Şeflik saati bu tavanın İÇİNDEN düşülür.
    #[test]
    fn capacity_for_this_school_matches_spec_table() {
        let cap = statutory_cap(InstitutionType::Other, true);
        assert_eq!(cap, 20);

        let department = coordinator_capacity(24, ChiefType::Department.weekly_hours(), 0, cap);
        let workshop = coordinator_capacity(24, ChiefType::WorkshopLab.weekly_hours(), 0, cap);
        let plain = coordinator_capacity(24, ChiefType::None.weekly_hours(), 0, cap);

        assert_eq!(department, 14, "bölüm şefi: min(20, 24-10)");
        assert_eq!(workshop, 18, "atölye/lab şefi: min(20, 24-6)");
        assert_eq!(plain, 20, "şef değil: min(20, 24-0)");
    }

    /// Meslekî eğitim merkezinde tavan 24'tür; şefsiz öğretmende bütçe sınırlar.
    #[test]
    fn vocational_center_capacity_is_bounded_by_remaining_budget() {
        let cap = statutory_cap(InstitutionType::VocationalCenter, true);
        assert_eq!(cap, 24);
        assert_eq!(coordinator_capacity(24, 0, 0, cap), 24);
        assert_eq!(coordinator_capacity(24, 6, 0, cap), 18);
    }

    /// Koordinatörlük dışı ek dersler de aynı bütçeden düşer.
    #[test]
    fn other_extra_hours_reduce_capacity() {
        let cap = statutory_cap(InstitutionType::Other, true);
        assert_eq!(coordinator_capacity(24, 0, 6, cap), 18);
        assert_eq!(coordinator_capacity(24, 10, 4, cap), 10);
    }

    /// Bütçe tükendiğinde kapasite sıfırdır, negatif olmaz.
    #[test]
    fn capacity_never_goes_negative() {
        let cap = statutory_cap(InstitutionType::Other, true);
        assert_eq!(coordinator_capacity(24, 10, 20, cap), 0);
        assert_eq!(coordinator_capacity(6, 10, 0, cap), 0);
    }
}
