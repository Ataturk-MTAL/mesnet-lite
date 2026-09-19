use serde::{Deserialize, Serialize};

/// İşletme saat tavanı kuralı.
/// Mesafe eşikleri GİDİŞ-DÖNÜŞ km cinsindendir (tek yönün iki katı).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct HourRule {
    pub id: i64,
    /// Dahil
    pub min_distance_km: f64,
    /// Hariç; None = üst sınırsız
    pub max_distance_km: Option<f64>,
    /// Dahil
    pub min_students: i64,
    /// Dahil; None = üst sınırsız
    pub max_students: Option<i64>,
    pub max_hours: i64,
}

impl HourRule {
    fn matches(&self, round_trip_km: f64, student_count: i64) -> bool {
        let above_min_distance = round_trip_km >= self.min_distance_km;
        let below_max_distance = match self.max_distance_km {
            Some(max) => round_trip_km < max,
            None => true,
        };
        let above_min_students = student_count >= self.min_students;
        let below_max_students = match self.max_students {
            Some(max) => student_count <= max,
            None => true,
        };

        above_min_distance && below_max_distance && above_min_students && below_max_students
    }

    /// Mesafe aralığının genişliği. Üst sınırsız aralık sonsuz sayılır.
    fn distance_span(&self) -> f64 {
        match self.max_distance_km {
            Some(max) => max - self.min_distance_km,
            None => f64::INFINITY,
        }
    }

    /// Öğrenci aralığının genişliği. Üst sınırsız aralık sonsuz sayılır.
    fn student_span(&self) -> f64 {
        match self.max_students {
            Some(max) => (max - self.min_students) as f64,
            None => f64::INFINITY,
        }
    }
}

/// Bir işletmeye uyan kurallar arasından EN DAR olanı seçer.
///
/// Sıralama kesindir (spec §6):
/// 1. En küçük mesafe aralığı genişliği
/// 2. Eşitlikte en küçük öğrenci aralığı genişliği
/// 3. Hâlâ eşitse en küçük `id`
///
/// Hiçbir kural uymazsa None döner; varsayılan uydurulmaz.
pub fn select_narrowest(
    rules: &[HourRule],
    round_trip_km: f64,
    student_count: i64,
) -> Option<&HourRule> {
    rules
        .iter()
        .filter(|rule| rule.matches(round_trip_km, student_count))
        .min_by(|a, b| {
            a.distance_span()
                .total_cmp(&b.distance_span())
                .then(a.student_span().total_cmp(&b.student_span()))
                .then(a.id.cmp(&b.id))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(
        id: i64,
        min_distance_km: f64,
        max_distance_km: Option<f64>,
        min_students: i64,
        max_students: Option<i64>,
        max_hours: i64,
    ) -> HourRule {
        HourRule {
            id,
            min_distance_km,
            max_distance_km,
            min_students,
            max_students,
            max_hours,
        }
    }

    /// Öğrenci sayısı 6'da 5-6 ve 6+ sütunları çakışır; en dar olan kazanır.
    #[test]
    fn six_students_falls_into_the_narrower_five_to_six_bracket() {
        let rules = vec![
            rule(3, 0.0, Some(1.0), 5, Some(6), 4),
            rule(4, 0.0, Some(1.0), 6, None, 5),
        ];
        let chosen = select_narrowest(&rules, 0.5, 6).unwrap();
        assert_eq!(chosen.id, 3, "5-6 aralığı 6+ aralığından dardır");
        assert_eq!(chosen.max_hours, 4);
    }

    /// Mesafe aralığı genişliği öğrenci aralığından önce gelir.
    #[test]
    fn distance_span_outranks_student_span() {
        let rules = vec![
            // Dar mesafe, geniş öğrenci
            rule(1, 0.0, Some(1.0), 1, None, 2),
            // Geniş mesafe, dar öğrenci
            rule(2, 0.0, Some(100.0), 1, Some(2), 9),
        ];
        assert_eq!(select_narrowest(&rules, 0.5, 1).unwrap().id, 1);
    }

    /// Her şey eşitse en küçük id kazanır; sonuç belirlenimcidir.
    #[test]
    fn identical_spans_resolve_by_lowest_id() {
        let rules = vec![
            rule(7, 0.0, Some(1.0), 1, Some(2), 3),
            rule(2, 0.0, Some(1.0), 1, Some(2), 9),
        ];
        assert_eq!(select_narrowest(&rules, 0.5, 1).unwrap().id, 2);
    }

    #[test]
    fn returns_none_when_no_rule_matches() {
        let rules = vec![rule(1, 0.0, Some(1.0), 1, Some(2), 2)];
        // Mesafe uyuyor ama öğrenci sayısı aralık dışında
        assert!(select_narrowest(&rules, 0.5, 9).is_none());
        // Öğrenci uyuyor ama mesafe aralık dışında
        assert!(select_narrowest(&rules, 50.0, 1).is_none());
    }
}
