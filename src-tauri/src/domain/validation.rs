use crate::domain::scheduling::{days_over_daily_cap, Slot, MAX_HOURS_PER_DAY};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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
    UnplacedHours,
    TooManyStudentsPerCoordinator,
    PoolExceeded,
    FieldMismatch,
    RuleNotFound,
    MissingCoordinates,
    PartialStudentCoverage,
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
            | ViolationCode::UnplacedHours
            | ViolationCode::TooManyStudentsPerCoordinator => Severity::Error,

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
    pub slots: BTreeSet<Slot>,
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

    if check.slots.len() as i64 != check.awarded_hours {
        violations.push(
            Violation::new(
                ViolationCode::UnplacedHours,
                forced,
                format!(
                    "{}: {} saat takdir edildi ama {} dilim yerleştirildi",
                    check.company_name,
                    check.awarded_hours,
                    check.slots.len()
                ),
            )
            .for_company(check.company_id),
        );
    }

    let unavailable: Vec<&Slot> = check
        .slots
        .iter()
        .filter(|slot| !check.teacher_free_slots.contains(slot))
        .collect();
    if !unavailable.is_empty() {
        violations.push(
            Violation::new(
                ViolationCode::SlotNotAvailable,
                forced,
                format!(
                    "{}: {} dilim öğretmenin boş saatleri dışında",
                    check.company_name,
                    unavailable.len()
                ),
            )
            .for_company(check.company_id)
            .for_teacher(check.teacher_id),
        );
    }

    let wrong_day: Vec<&Slot> = check
        .slots
        .iter()
        .filter(|slot| !check.workplace_days.contains(&slot.day_of_week))
        .collect();
    if !wrong_day.is_empty() {
        violations.push(
            Violation::new(
                ViolationCode::SlotNotWorkplaceDay,
                forced,
                format!(
                    "{}: {} dilim öğrencilerin işletmede bulunmadığı güne denk geliyor",
                    check.company_name,
                    wrong_day.len()
                ),
            )
            .for_company(check.company_id),
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
    all_slots: &BTreeSet<Slot>,
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

    for day in days_over_daily_cap(all_slots) {
        violations.push(
            Violation::new(
                ViolationCode::DailyCapExceeded,
                is_forced,
                format!(
                    "{teacher_name}: {}. günde {} saat, günlük sınır {MAX_HOURS_PER_DAY}",
                    day,
                    all_slots.iter().filter(|s| s.day_of_week == day).count()
                ),
            )
            .for_teacher(teacher_id),
        );
    }

    violations
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
            slots: slots(&[(1, 9), (1, 10)]),
            teacher_free_slots: slots(&[(1, 9), (1, 10), (1, 11)]),
            workplace_days: BTreeSet::from([1]),
            student_count: 1,
            covered_student_count: 1,
            branch_matches: true,
            has_coordinates: true,
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
        check.slots = slots(&[(1, 9), (1, 10)]);

        let found = codes(&check_assignment(&check));
        assert!(found.contains(&ViolationCode::AwardedExceedsMax));
    }

    #[test]
    fn missing_rule_is_flagged_instead_of_assuming_a_default() {
        let mut check = valid_check();
        check.max_hours = None;

        assert!(codes(&check_assignment(&check)).contains(&ViolationCode::RuleNotFound));
    }

    #[test]
    fn slot_count_must_equal_awarded_hours() {
        let mut check = valid_check();
        check.awarded_hours = 3;

        assert!(codes(&check_assignment(&check)).contains(&ViolationCode::UnplacedHours));
    }

    #[test]
    fn slot_outside_teacher_availability_is_flagged() {
        let mut check = valid_check();
        check.slots = slots(&[(1, 9), (1, 15)]);

        assert!(codes(&check_assignment(&check)).contains(&ViolationCode::SlotNotAvailable));
    }

    #[test]
    fn slot_on_a_non_workplace_day_is_flagged() {
        let mut check = valid_check();
        check.teacher_free_slots = slots(&[(1, 9), (4, 10)]);
        check.slots = slots(&[(1, 9), (4, 10)]);

        assert!(codes(&check_assignment(&check)).contains(&ViolationCode::SlotNotWorkplaceDay));
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
        let found = check_teacher_totals(1, "Test", 20, 22, &BTreeSet::new(), false);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, ViolationCode::WeeklyCapExceeded);
        assert_eq!(found[0].legal_basis, Some("MADDE 15/2 + MADDE 6/1-c"));
    }

    #[test]
    fn weekly_total_within_capacity_is_clean() {
        assert!(check_teacher_totals(1, "Test", 20, 20, &BTreeSet::new(), false).is_empty());
    }

    #[test]
    fn daily_cap_violation_is_reported_per_day() {
        let nine_hours: Vec<(i64, i64)> = (8..17).map(|h| (2, h)).collect();
        let found = check_teacher_totals(1, "Test", 20, 9, &slots(&nine_hours), false);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, ViolationCode::DailyCapExceeded);
        assert_eq!(found[0].legal_basis, Some("OÖKY MADDE 88"));
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
