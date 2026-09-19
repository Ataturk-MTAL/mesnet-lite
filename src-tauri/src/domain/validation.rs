use crate::domain::scheduling::{days_over_daily_cap, Block, Slot, MAX_HOURS_PER_DAY};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// OÖKY MADDE 88: "Aynı işletmede aynı alanda mesleki eğitim gören
/// 15 öğrenciye kadar bir koordinatör öğretmen görevlendirilir"
pub const MAX_STUDENTS_PER_COORDINATOR: i64 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ViolationCode {
    AwardedExceedsMax,
    WeeklyCapExceeded,
    DailyCapExceeded,
    SlotNotAvailable,
    SlotNotWorkplaceDay,
    NotPlaced,
    TooManyStudentsPerCoordinator,
    PoolExceeded,
    FieldMismatch,
    RuleNotFound,
    MissingCoordinates,
    PartialStudentCoverage,
    /// Blok, öğretmenin aynı dönemdeki başka bir bloğuyla aynı hücrede duruyor.
    BlockOverlap,
    /// Blok, ızgaranın gün sonunu (`day_end_hour`, HARİÇ) aşıyor.
    BlockExceedsDayEnd,
}

impl ViolationCode {
    /// Kuralın doğal ağırlığı. `is_forced` olan atamalarda `Error` `Warning`'e düşer.
    pub fn natural_severity(self) -> Severity {
        match self {
            ViolationCode::AwardedExceedsMax
            | ViolationCode::WeeklyCapExceeded
            | ViolationCode::DailyCapExceeded
            | ViolationCode::SlotNotAvailable
            | ViolationCode::SlotNotWorkplaceDay
            | ViolationCode::NotPlaced
            | ViolationCode::TooManyStudentsPerCoordinator
            | ViolationCode::BlockOverlap
            | ViolationCode::BlockExceedsDayEnd => Severity::Error,

            ViolationCode::PoolExceeded
            | ViolationCode::FieldMismatch
            | ViolationCode::RuleNotFound
            | ViolationCode::MissingCoordinates => Severity::Warning,

            ViolationCode::PartialStudentCoverage => Severity::Info,
        }
    }

    /// Mevzuat dayanağı; arayüzde ve çizelgede gösterilir.
    pub fn legal_basis(self) -> Option<&'static str> {
        match self {
            ViolationCode::WeeklyCapExceeded => Some("MADDE 15/2 + MADDE 6/1-c"),
            ViolationCode::DailyCapExceeded
            | ViolationCode::TooManyStudentsPerCoordinator
            | ViolationCode::FieldMismatch => Some("OÖKY MADDE 88"),
            ViolationCode::PoolExceeded => Some("MADDE 15/2"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Violation {
    pub code: ViolationCode,
    pub severity: Severity,
    /// İlgili atamanın işletme id'si; havuz gibi genel ihlallerde None.
    pub company_id: Option<i64>,
    pub teacher_id: Option<i64>,
    /// Kullanıcıya gösterilecek Türkçe açıklama.
    pub message: String,
    pub legal_basis: Option<&'static str>,
}

impl Violation {
    fn new(code: ViolationCode, is_forced: bool, message: String) -> Self {
        // Zorlama açıksa kullanıcı kuralı bilerek çiğniyor: engelleme uyarıya döner.
        let severity = match (code.natural_severity(), is_forced) {
            (Severity::Error, true) => Severity::Warning,
            (other, _) => other,
        };
        Self {
            code,
            severity,
            company_id: None,
            teacher_id: None,
            message,
            legal_basis: code.legal_basis(),
        }
    }

    fn for_company(mut self, company_id: i64) -> Self {
        self.company_id = Some(company_id);
        self
    }

    fn for_teacher(mut self, teacher_id: i64) -> Self {
        self.teacher_id = Some(teacher_id);
        self
    }
}

/// Tek bir atamanın denetlenmesi için gereken bilgiler.
#[derive(Debug, Clone)]
pub struct AssignmentCheck {
    pub company_id: i64,
    pub company_name: String,
    pub teacher_id: i64,
    pub teacher_name: String,
    pub awarded_hours: i64,
    /// Kural tablosundan gelen tavan; kural bulunamadıysa None.
    pub max_hours: Option<i64>,
    pub is_forced: bool,
    /// Ziyaretin yeri — TEK bir hücre. Atanmamışsa None.
    pub visit_slot: Option<Slot>,
    /// Öğretmenin bu dönemdeki boş saatleri.
    pub teacher_free_slots: BTreeSet<Slot>,
    /// İşletmedeki öğrencilerin sınıflarının işletme günleri birleşimi.
    pub workplace_days: BTreeSet<i64>,
    pub student_count: i64,
    /// Seçilen günlerin kapsadığı öğrenci sayısı; tümünü kapsamıyorsa bilgi notu.
    pub covered_student_count: i64,
    /// İşletmenin dalı öğretmenin dalları arasında mı?
    pub branch_matches: bool,
    pub has_coordinates: bool,
    /// Izgaranın bitişi (ayarlardaki `day_end_hour`), HARİÇ.
    pub day_end_hour: i64,
    /// Aynı öğretmenin bloğuyla çakışan başka bir işletme varsa adı; yoksa None.
    /// Çakışma denetimi burada YAPILMAZ — çağıran taraf (repository katmanı)
    /// tüm atamaları görebildiği için hesaplayıp buraya bilgi olarak geçirir.
    pub overlapping_with: Option<String>,
}

/// Tek bir atamayı denetler.
pub fn check_assignment(check: &AssignmentCheck) -> Vec<Violation> {
    let mut violations = Vec::new();
    let forced = check.is_forced;

    match check.max_hours {
        None => violations.push(
            Violation::new(
                ViolationCode::RuleNotFound,
                forced,
                format!(
                    "{}: mesafe ve öğrenci sayısına uyan saat kuralı yok, tavan hesaplanamadı",
                    check.company_name
                ),
            )
            .for_company(check.company_id),
        ),
        Some(max_hours) if check.awarded_hours > max_hours => violations.push(
            Violation::new(
                ViolationCode::AwardedExceedsMax,
                forced,
                format!(
                    "{}: takdir edilen {} saat, tavan {} saati aşıyor",
                    check.company_name, check.awarded_hours, max_hours
                ),
            )
            .for_company(check.company_id),
        ),
        Some(_) => {}
    }

    match check.visit_slot {
        None => violations.push(
            Violation::new(
                ViolationCode::NotPlaced,
                forced,
                format!("{}: ziyaret günü ve saati belirlenmemiş", check.company_name),
            )
            .for_company(check.company_id),
        ),
        Some(slot) => {
            if !check.teacher_free_slots.contains(&slot) {
                violations.push(
                    Violation::new(
                        ViolationCode::SlotNotAvailable,
                        forced,
                        format!(
                            "{}: seçilen saat öğretmenin boş saatleri arasında değil",
                            check.company_name
                        ),
                    )
                    .for_company(check.company_id)
                    .for_teacher(check.teacher_id),
                );
            }

            if !check.workplace_days.contains(&slot.day_of_week) {
                violations.push(
                    Violation::new(
                        ViolationCode::SlotNotWorkplaceDay,
                        forced,
                        format!(
                            "{}: seçilen gün öğrencilerin işletmede bulunmadığı bir gün",
                            check.company_name
                        ),
                    )
                    .for_company(check.company_id),
                );
            }

            let block = Block::from_start(slot.day_of_week, slot.hour, check.awarded_hours);
            if block.exceeds_day_end(check.day_end_hour) {
                violations.push(
                    Violation::new(
                        ViolationCode::BlockExceedsDayEnd,
                        forced,
                        format!(
                            "{}: {}. saatte başlayan {} saatlik blok, ızgaranın {}. saatte biten \
                             gün sonunu aşıyor",
                            check.company_name,
                            block.start_hour,
                            block.end_hour - block.start_hour + 1,
                            check.day_end_hour
                        ),
                    )
                    .for_company(check.company_id)
                    .for_teacher(check.teacher_id),
                );
            }
        }
    }

    if let Some(other_company) = &check.overlapping_with {
        violations.push(
            Violation::new(
                ViolationCode::BlockOverlap,
                forced,
                format!(
                    "{}: ziyaret bloğu, aynı öğretmenin {} işletmesindeki bloğuyla çakışıyor",
                    check.company_name, other_company
                ),
            )
            .for_company(check.company_id)
            .for_teacher(check.teacher_id),
        );
    }

    if check.student_count > MAX_STUDENTS_PER_COORDINATOR {
        violations.push(
            Violation::new(
                ViolationCode::TooManyStudentsPerCoordinator,
                forced,
                format!(
                    "{}: {} öğrenci tek koordinatöre bağlı, sınır {}",
                    check.company_name, check.student_count, MAX_STUDENTS_PER_COORDINATOR
                ),
            )
            .for_company(check.company_id)
            .for_teacher(check.teacher_id),
        );
    }

    if !check.branch_matches {
        violations.push(
            Violation::new(
                ViolationCode::FieldMismatch,
                forced,
                format!(
                    "{}: işletmenin dalı {} öğretmeninin dalları arasında yok",
                    check.company_name, check.teacher_name
                ),
            )
            .for_company(check.company_id)
            .for_teacher(check.teacher_id),
        );
    }

    if !check.has_coordinates {
        violations.push(
            Violation::new(
                ViolationCode::MissingCoordinates,
                forced,
                format!("{}: harita konumu yok", check.company_name),
            )
            .for_company(check.company_id),
        );
    }

    if check.covered_student_count < check.student_count {
        violations.push(
            Violation::new(
                ViolationCode::PartialStudentCoverage,
                forced,
                format!(
                    "{}: seçilen günler {} öğrencinin {}'ini kapsıyor",
                    check.company_name, check.student_count, check.covered_student_count
                ),
            )
            .for_company(check.company_id),
        );
    }

    violations
}

/// Bir öğretmenin tüm atamalarının toplu denetimi.
pub fn check_teacher_totals(
    teacher_id: i64,
    teacher_name: &str,
    capacity: i64,
    awarded_total: i64,
    hours_by_day: &std::collections::BTreeMap<i64, i64>,
    is_forced: bool,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    if awarded_total > capacity {
        violations.push(
            Violation::new(
                ViolationCode::WeeklyCapExceeded,
                is_forced,
                format!(
                    "{teacher_name}: toplam {awarded_total} saat, kapasite {capacity} saati aşıyor"
                ),
            )
            .for_teacher(teacher_id),
        );
    }

    // Günlük sınır SAAT toplamına bakar; bir hücrede 8 saatlik işletme durabilir.
    // Birden çok gün aşılmışsa TEK bir ihlalde, gün adları Türkçe liste olarak
    // birleştirilir (ham gün numarası kullanıcıya asla gösterilmez).
    let exceeded_days = days_over_daily_cap(hours_by_day);
    if !exceeded_days.is_empty() {
        let day_list = exceeded_days
            .iter()
            .map(|day| day_name(*day))
            .collect::<Vec<_>>()
            .join(", ");
        let day_word = if exceeded_days.len() == 1 {
            "gününde"
        } else {
            "günlerinde"
        };
        violations.push(
            Violation::new(
                ViolationCode::DailyCapExceeded,
                is_forced,
                format!(
                    "{teacher_name}: {day_list} {day_word} günlük {MAX_HOURS_PER_DAY} saat sınırı aşıldı"
                ),
            )
            .for_teacher(teacher_id),
        );
    }

    violations
}

/// Gün numarasını (1 = Pazartesi … 5 = Cuma) Türkçe gün adına çevirir.
/// Tanınmayan bir numara sessizce yutulmaz, "Gün {n}" olarak görünür kalır.
///
/// Bu, projedeki TEK gün-adı eşlemesi olmalı: aynı eşlemeyi başka bir dosyada
/// (ör. `pdf_report.rs`) yeniden yazmak yerine buradan çağırın.
pub fn day_name(day: i64) -> String {
    match day {
        1 => "Pazartesi".to_string(),
        2 => "Salı".to_string(),
        3 => "Çarşamba".to_string(),
        4 => "Perşembe".to_string(),
        5 => "Cuma".to_string(),
        other => format!("Gün {other}"),
    }
}

/// Okulun toplam havuzu aşıldı mı (MADDE 15/2).
pub fn check_pool(total_awarded: i64, total_pool: i64) -> Option<Violation> {
    if total_pool > 0 && total_awarded > total_pool {
        return Some(Violation::new(
            ViolationCode::PoolExceeded,
            false,
            format!("Takdir toplamı {total_awarded} saat, okul havuzu {total_pool} saati aşıyor"),
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(pairs: &[(i64, i64)]) -> BTreeSet<Slot> {
        pairs.iter().map(|(d, h)| Slot::new(*d, *h)).collect()
    }

    fn valid_check() -> AssignmentCheck {
        AssignmentCheck {
            company_id: 1,
            company_name: "Test İşletme A".into(),
            teacher_id: 2,
            teacher_name: "Test Öğretmen".into(),
            awarded_hours: 2,
            max_hours: Some(8),
            is_forced: false,
            visit_slot: Some(Slot::new(1, 9)),
            teacher_free_slots: slots(&[(1, 9), (1, 10), (1, 11)]),
            workplace_days: BTreeSet::from([1]),
            student_count: 1,
            covered_student_count: 1,
            branch_matches: true,
            has_coordinates: true,
            day_end_hour: 17,
            overlapping_with: None,
        }
    }

    fn codes(violations: &[Violation]) -> Vec<ViolationCode> {
        violations.iter().map(|v| v.code).collect()
    }

    #[test]
    fn valid_assignment_produces_no_violations() {
        assert!(check_assignment(&valid_check()).is_empty());
    }

    #[test]
    fn awarded_above_max_is_flagged() {
        let mut check = valid_check();
        check.awarded_hours = 9;

        let found = codes(&check_assignment(&check));
        assert!(found.contains(&ViolationCode::AwardedExceedsMax));
    }

    #[test]
    fn missing_rule_is_flagged_instead_of_assuming_a_default() {
        let mut check = valid_check();
        check.max_hours = None;

        assert!(codes(&check_assignment(&check)).contains(&ViolationCode::RuleNotFound));
    }

    /// Ziyaret hücresi seçilmemiş atama eksiktir.
    #[test]
    fn assignment_without_a_visit_slot_is_flagged() {
        let mut check = valid_check();
        check.visit_slot = None;

        assert!(codes(&check_assignment(&check)).contains(&ViolationCode::NotPlaced));
    }

    /// Saat, hücre sayısına bağlı DEĞİL: 6 saatlik işletme tek hücrede durur.
    #[test]
    fn a_single_cell_can_carry_many_hours() {
        let mut check = valid_check();
        check.awarded_hours = 6;
        check.max_hours = Some(8);

        assert!(check_assignment(&check).is_empty());
    }

    #[test]
    fn slot_outside_teacher_availability_is_flagged() {
        let mut check = valid_check();
        check.visit_slot = Some(Slot::new(1, 15));

        assert!(codes(&check_assignment(&check)).contains(&ViolationCode::SlotNotAvailable));
    }

    #[test]
    fn slot_on_a_non_workplace_day_is_flagged() {
        let mut check = valid_check();
        check.teacher_free_slots = slots(&[(4, 10)]);
        check.visit_slot = Some(Slot::new(4, 10));

        assert!(codes(&check_assignment(&check)).contains(&ViolationCode::SlotNotWorkplaceDay));
    }

    /// Blok gün sonunu aşarsa ihlal, hücrenin kendisi boş olsa bile bayraklanır.
    #[test]
    fn block_exceeding_the_grid_day_end_is_flagged() {
        let mut check = valid_check();
        check.teacher_free_slots = slots(&[(1, 11), (1, 12), (1, 13), (1, 14), (1, 15), (1, 16)]);
        check.visit_slot = Some(Slot::new(1, 11));
        check.awarded_hours = 6; // 11-16, ızgara 17'de (hariç) bitiyor -> tam sığar
        check.day_end_hour = 16; // ama ızgara burada 16'da bitiyor -> aşıyor

        assert!(codes(&check_assignment(&check)).contains(&ViolationCode::BlockExceedsDayEnd));
    }

    /// Blok tam ızgara sınırından önce bitiyorsa ihlal oluşmaz.
    #[test]
    fn block_ending_exactly_before_day_end_is_not_flagged() {
        let mut check = valid_check();
        check.awarded_hours = 2;
        check.visit_slot = Some(Slot::new(1, 9));
        check.day_end_hour = 11; // 9-10 tam sığar

        assert!(!codes(&check_assignment(&check)).contains(&ViolationCode::BlockExceedsDayEnd));
    }

    /// Repository katmanı çakışan işletmeyi bildirdiğinde ihlal oluşur.
    #[test]
    fn overlapping_block_reported_by_the_caller_is_flagged() {
        let mut check = valid_check();
        check.overlapping_with = Some("Çakışan İşletme".into());

        let found = check_assignment(&check);
        let overlap = found
            .iter()
            .find(|v| v.code == ViolationCode::BlockOverlap)
            .unwrap();
        assert!(overlap.message.contains("Çakışan İşletme"));
    }

    #[test]
    fn no_overlap_reported_when_the_caller_found_none() {
        let check = valid_check();
        assert!(!codes(&check_assignment(&check)).contains(&ViolationCode::BlockOverlap));
    }

    /// Zorlama açıkken blok ihlalleri de uyarıya düşer ama KAYBOLMAZ.
    #[test]
    fn forcing_downgrades_block_violations_to_warnings_without_hiding_them() {
        let mut check = valid_check();
        check.is_forced = true;
        check.overlapping_with = Some("Çakışan İşletme".into());
        check.day_end_hour = 1; // her blok aşar

        let found = check_assignment(&check);
        let overlap = found
            .iter()
            .find(|v| v.code == ViolationCode::BlockOverlap)
            .unwrap();
        let exceeds = found
            .iter()
            .find(|v| v.code == ViolationCode::BlockExceedsDayEnd)
            .unwrap();
        assert_eq!(overlap.severity, Severity::Warning);
        assert_eq!(exceeds.severity, Severity::Warning);
    }

    /// OÖKY MADDE 88: 15 öğrenciye kadar bir koordinatör.
    #[test]
    fn sixteen_students_exceeds_the_coordinator_limit() {
        let mut check = valid_check();
        check.student_count = 16;
        check.covered_student_count = 16;

        assert!(codes(&check_assignment(&check))
            .contains(&ViolationCode::TooManyStudentsPerCoordinator));
    }

    #[test]
    fn exactly_fifteen_students_is_allowed() {
        let mut check = valid_check();
        check.student_count = 15;
        check.covered_student_count = 15;

        assert!(!codes(&check_assignment(&check))
            .contains(&ViolationCode::TooManyStudentsPerCoordinator));
    }

    #[test]
    fn branch_mismatch_is_a_warning_not_an_error() {
        let mut check = valid_check();
        check.branch_matches = false;

        let found = check_assignment(&check);
        let mismatch = found
            .iter()
            .find(|v| v.code == ViolationCode::FieldMismatch)
            .unwrap();
        assert_eq!(mismatch.severity, Severity::Warning);
    }

    #[test]
    fn partial_coverage_is_informational_only() {
        let mut check = valid_check();
        check.student_count = 2;
        check.covered_student_count = 1;

        let found = check_assignment(&check);
        let coverage = found
            .iter()
            .find(|v| v.code == ViolationCode::PartialStudentCoverage)
            .unwrap();
        assert_eq!(coverage.severity, Severity::Info);
    }

    /// Zorlama açıkken engelleyici ihlaller uyarıya düşer ama KAYBOLMAZ.
    #[test]
    fn forcing_downgrades_errors_to_warnings_without_hiding_them() {
        let mut check = valid_check();
        check.awarded_hours = 9;
        check.is_forced = true;

        let found = check_assignment(&check);
        let exceeded = found
            .iter()
            .find(|v| v.code == ViolationCode::AwardedExceedsMax)
            .unwrap();
        assert_eq!(exceeded.severity, Severity::Warning);
    }

    #[test]
    fn weekly_cap_violation_carries_its_legal_basis() {
        let found = check_teacher_totals(1, "Test", 20, 22, &BTreeMap::new(), false);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, ViolationCode::WeeklyCapExceeded);
        assert_eq!(found[0].legal_basis, Some("MADDE 15/2 + MADDE 6/1-c"));
    }

    #[test]
    fn weekly_total_within_capacity_is_clean() {
        assert!(check_teacher_totals(1, "Test", 20, 20, &BTreeMap::new(), false).is_empty());
    }

    /// Günlük sınır o güne düşen SAAT toplamına bakar.
    #[test]
    fn daily_cap_violation_is_reported_per_day() {
        let hours_by_day = BTreeMap::from([(2, 9)]);
        let found = check_teacher_totals(1, "Test", 20, 9, &hours_by_day, false);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, ViolationCode::DailyCapExceeded);
        assert_eq!(found[0].legal_basis, Some("OÖKY MADDE 88"));
    }

    /// Tam 8 saat sınırı aşmaz.
    #[test]
    fn exactly_eight_hours_on_a_day_is_allowed() {
        let hours_by_day = BTreeMap::from([(2, 8)]);
        assert!(check_teacher_totals(1, "Test", 20, 8, &hours_by_day, false).is_empty());
    }

    /// Uyarı metni ham gün numarası DEĞİL, Türkçe gün adı içermeli.
    #[test]
    fn daily_cap_message_uses_the_turkish_day_name_not_the_raw_number() {
        let hours_by_day = BTreeMap::from([(3, 9)]);
        let found = check_teacher_totals(1, "Ahmet YILMAZ", 20, 9, &hours_by_day, false);

        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("Çarşamba"));
        assert!(!found[0].message.contains(" 3 "));
    }

    /// Birden çok gün aşıldığında TEK ihlalde, adlar virgülle birleştirilir.
    #[test]
    fn multiple_days_over_cap_are_combined_into_one_message() {
        let hours_by_day = BTreeMap::from([(2, 9), (3, 10)]);
        let found = check_teacher_totals(1, "Ahmet YILMAZ", 20, 19, &hours_by_day, false);

        let daily = found
            .iter()
            .filter(|v| v.code == ViolationCode::DailyCapExceeded)
            .collect::<Vec<_>>();
        assert_eq!(daily.len(), 1);
        assert!(daily[0].message.contains("Salı, Çarşamba"));
    }

    #[test]
    fn day_name_maps_numbers_to_turkish_day_names() {
        assert_eq!(day_name(1), "Pazartesi");
        assert_eq!(day_name(2), "Salı");
        assert_eq!(day_name(3), "Çarşamba");
        assert_eq!(day_name(4), "Perşembe");
        assert_eq!(day_name(5), "Cuma");
        // Tanınmayan numara sessizce yutulmaz, olduğu gibi görünür kalır.
        assert_eq!(day_name(9), "Gün 9");
    }

    #[test]
    fn pool_check_flags_only_when_exceeded() {
        assert!(check_pool(10, 20).is_none());
        assert!(check_pool(20, 20).is_none());
        assert_eq!(check_pool(21, 20).unwrap().code, ViolationCode::PoolExceeded);
    }

    /// Havuz tanımlı değilse (0) kontrol yapılmaz; sıfır "sınırsız" demek değil,
    /// "ayar girilmemiş" demektir.
    #[test]
    fn pool_check_is_skipped_when_pool_is_unset() {
        assert!(check_pool(100, 0).is_none());
    }
}
