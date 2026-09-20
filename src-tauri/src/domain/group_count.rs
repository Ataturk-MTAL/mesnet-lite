//! Öğrenci sayısından grup sayısı (Norm Kadro Yönetmeliği MADDE 22/1-ç tablosu).
//!
//! Saf kural modülüdür: veritabanına dokunmaz. Havuz hesabı (MADDE 15/2) ve
//! ders yükü ekranı bu tek noktadan geçer; tablo başka yerde tekrarlanmaz.

/// Bir şubede açılabilecek en fazla grup sayısı (MADDE 22/1-ç tablosunun
/// son satırı: 33 ve üzeri öğrenci ⇒ 4 grup).
pub const MAX_GROUPS_PER_BRANCH: i64 = 4;

// Tablonun alt sınırları (9. sınıfta 10, diğerlerinde 8) sabit olarak
// tutulmaz: alt sınırın altı da 1 grup sayıldığı için kuralda kullanılmazlar
// (bkz. `groups_for`).

/// 9. sınıf: bu sayıya kadar (dahil) 1 grup.
const NINTH_GRADE_ONE_GROUP_MAX: i64 = 20;
/// 9. sınıf: bu sayıya kadar (dahil) 2 grup; üstü 3 grup.
const NINTH_GRADE_TWO_GROUPS_MAX: i64 = 31;

/// 10., 11. ve 12. sınıf: bu sayıya kadar (dahil) 1 grup.
const UPPER_GRADE_ONE_GROUP_MAX: i64 = 16;
/// 10–12. sınıf: bu sayıya kadar (dahil) 2 grup.
const UPPER_GRADE_TWO_GROUPS_MAX: i64 = 24;
/// 10–12. sınıf: bu sayıya kadar (dahil) 3 grup; üstü 4 grup.
const UPPER_GRADE_THREE_GROUPS_MAX: i64 = 32;

/// Tabloda 9. sınıf ayrı satır dizisidir; 10–12 aynı diziyi paylaşır.
const NINTH_GRADE: u8 = 9;

/// Sınıf düzeyi ve öğrenci sayısından grup sayısı.
///
/// - Öğrenci yoksa (0 veya negatif) 0 grup: öğrencisiz şube havuza katkı
///   vermez.
/// - Tablonun alt sınırının altındaki öğrenci sayısı (9. sınıfta 10, diğerlerinde
///   8 altı) yine 1 grup sayılır: öğrencisi olan şubede ders yine okutulur.
/// - Özel eğitim (kaynaştırma) istisnası burada YOKTUR; kullanıcı grup
///   sayısını elle girer.
pub fn groups_for(grade_level: u8, student_count: i64) -> i64 {
    if student_count <= 0 {
        return 0;
    }
    let groups = if grade_level == NINTH_GRADE {
        ninth_grade_groups(student_count)
    } else {
        upper_grade_groups(student_count)
    };
    groups.min(MAX_GROUPS_PER_BRANCH)
}

fn ninth_grade_groups(student_count: i64) -> i64 {
    match student_count {
        n if n <= NINTH_GRADE_ONE_GROUP_MAX => 1,
        n if n <= NINTH_GRADE_TWO_GROUPS_MAX => 2,
        _ => 3,
    }
}

fn upper_grade_groups(student_count: i64) -> i64 {
    match student_count {
        n if n <= UPPER_GRADE_ONE_GROUP_MAX => 1,
        n if n <= UPPER_GRADE_TWO_GROUPS_MAX => 2,
        n if n <= UPPER_GRADE_THREE_GROUPS_MAX => 3,
        _ => 4,
    }
}

/// `grade` metninin başındaki sayı ("12/C" ⇒ 12, "9-A" ⇒ 9). Başta sayı yoksa
/// veya sayı `u8`'e sığmıyorsa `None`.
pub fn grade_level(grade: &str) -> Option<u8> {
    let trimmed = grade.trim();
    let digits_end = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(trimmed.len());
    trimmed[..digits_end].parse().ok()
}

/// Sınıf metninden grup sayısı. Sınıf düzeyi ayrıştırılamazsa tablo
/// uygulanamaz: öğrenci varsa 1 grup, yoksa 0 döner. Sessizce tahmin
/// yürütmemek için tablo yerine en yalın güvenli değer seçilir; kullanıcı
/// gerekirse satırı elle düzeltir.
pub fn groups_for_grade(grade: &str, student_count: i64) -> i64 {
    match grade_level(grade) {
        Some(level) => groups_for(level, student_count),
        None => i64::from(student_count > 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 9. sınıf: 10–20 → 1, 21–31 → 2, 32+ → 3. Her sınırın iki yanı.
    #[test]
    fn ninth_grade_boundaries() {
        for (students, groups) in [(9, 1), (10, 1), (20, 1), (21, 2), (31, 2), (32, 3), (100, 3)] {
            assert_eq!(groups_for(9, students), groups, "9. sınıf, {students} öğrenci");
        }
    }

    /// 10–12. sınıf: 8–16 → 1, 17–24 → 2, 25–32 → 3, 33+ → 4.
    #[test]
    fn upper_grades_boundaries() {
        for level in [10, 11, 12] {
            for (students, groups) in
                [(7, 1), (8, 1), (16, 1), (17, 2), (24, 2), (25, 3), (32, 3), (33, 4)]
            {
                assert_eq!(groups_for(level, students), groups, "{level}. sınıf, {students} öğrenci");
            }
        }
    }

    /// Alt sınırın altı (9. sınıfta <10, diğerlerinde <8) yine 1 grup.
    #[test]
    fn below_the_table_minimum_is_still_one_group() {
        assert_eq!(groups_for(9, 1), 1);
        assert_eq!(groups_for(9, 9), 1);
        assert_eq!(groups_for(12, 1), 1);
        assert_eq!(groups_for(12, 7), 1);
    }

    #[test]
    fn no_students_means_no_groups() {
        for level in [9, 10, 11, 12] {
            assert_eq!(groups_for(level, 0), 0);
        }
        assert_eq!(groups_for(12, -3), 0, "negatif sayı öğrencisizdir");
    }

    #[test]
    fn a_branch_never_exceeds_four_groups() {
        assert_eq!(groups_for(12, 10_000), MAX_GROUPS_PER_BRANCH);
        assert_eq!(groups_for(11, 33), MAX_GROUPS_PER_BRANCH);
    }

    #[test]
    fn grade_level_reads_the_leading_number() {
        assert_eq!(grade_level("12/C"), Some(12));
        assert_eq!(grade_level("9-A"), Some(9));
        assert_eq!(grade_level(" 10/B "), Some(10));
        assert_eq!(grade_level("11"), Some(11));
    }

    #[test]
    fn grade_level_is_none_when_it_cannot_be_read() {
        assert_eq!(grade_level("Hazırlık"), None);
        assert_eq!(grade_level(""), None);
        assert_eq!(grade_level("/C"), None);
        assert_eq!(grade_level("999/A"), None, "u8'e sığmayan sayı");
    }

    /// Gerçek veri: 12/C Elektronik Haberleşme, 16 öğrenci ⇒ 1 grup.
    #[test]
    fn groups_for_grade_applies_the_table_to_the_parsed_level() {
        assert_eq!(groups_for_grade("12/C", 16), 1);
        assert_eq!(groups_for_grade("12/C", 17), 2);
        assert_eq!(groups_for_grade("9-A", 21), 2);
    }

    /// Sınıf düzeyi okunamazsa öğrenci varsa 1 grup, yoksa 0.
    #[test]
    fn unparsable_grade_gives_one_group_when_there_are_students() {
        assert_eq!(groups_for_grade("Hazırlık", 40), 1);
        assert_eq!(groups_for_grade("Hazırlık", 1), 1);
        assert_eq!(groups_for_grade("Hazırlık", 0), 0);
    }
}
