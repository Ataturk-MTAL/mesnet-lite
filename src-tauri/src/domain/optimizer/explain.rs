//! Karar gerekçeleri: saat değişikliği ve yerleşemeyen işletme nedenleri.
//! Nedenler arama bittikten SONRA nihai durumdan türetilir; arama sırasında
//! tutulan tek bilgi `made_room`'dur.

use super::model::{HourChangeReason, UnassignedReason};
use super::state::{Placement, State};
use crate::domain::scheduling::MAX_HOURS_PER_DAY;
use crate::domain::validation::day_name;

/// Hamlenin kaydı: (hamleden sonraki saat, yer açılan işletme dizini).
pub(super) type RoomRecord = (i64, usize);

pub(super) fn hour_reason(
    state: &mut State,
    c: usize,
    placement: Placement,
    room: Option<&RoomRecord>,
) -> (HourChangeReason, String) {
    let p = state.p;
    let company = p.companies[c];
    let (old, new) = (company.awarded_hours, placement.hours);
    if new > old {
        return (
            HourChangeReason::Granted,
            format!("Saat {old} → {new}: işletmeye saat verildi"),
        );
    }
    if new == p.ceiling[c] {
        return (
            HourChangeReason::CeilingLowered,
            format!("Saat tavanı {new} saat olduğundan {old} → {new}"),
        );
    }
    if let Some((_, other)) = room.filter(|(hours, _)| *hours == new) {
        let other = p.companies[*other];
        return (
            HourChangeReason::MadeRoomFor {
                company_id: other.id,
            },
            format!(
                "{old} → {new}: «{}» işletmesine yer açmak için kısıldı",
                other.name
            ),
        );
    }
    let reason = why_not_more(state, c, placement);
    let text = reason_text(&reason, state, placement.teacher, (old, new));
    (reason, text)
}

/// Nihai durumda saat bir fazla olamaz mıydı? İlk engellenen kontrol:
/// havuz → kapasite → ardışık hücre → günlük sınır. İşletme geçici olarak
/// çıkarılır; kendi saati ve hücreleri engel sayılmasın.
fn why_not_more(state: &mut State, c: usize, placement: Placement) -> HourChangeReason {
    state.unplace(c);
    let (t, more) = (placement.teacher, placement.hours + 1);
    let reason = if state.p.budget.is_some() && more > state.budget_left() {
        if state.p.pool_overrun {
            HourChangeReason::PoolAlreadyOverrun
        } else {
            HourChangeReason::PoolExhausted
        }
    } else if more > state.teachers[t].rem_cap {
        HourChangeReason::TeacherCapacity
    } else if !state.any_block_position(t, c, more, false) {
        HourChangeReason::NoConsecutiveCells
    } else if !state.any_block_position(t, c, more, true) {
        HourChangeReason::DailyCap
    } else {
        // Hiçbir kural engellemiyor: yalnız arama sınırı bunu açıklar
        // (sınır dışında M1 bu artışı zaten uygulardı).
        HourChangeReason::SearchLimit
    };
    state.set(c, Some(placement));
    reason
}

fn reason_text(
    reason: &HourChangeReason,
    state: &State,
    teacher: usize,
    (old, new): (i64, i64),
) -> String {
    let teacher_name = &state.p.teachers[teacher].name;
    match reason {
        HourChangeReason::PoolExhausted => {
            format!("Saat havuzu tükendiği için {old} → {new} (MADDE 15/2)")
        }
        HourChangeReason::PoolAlreadyOverrun => {
            format!("Saat havuzu zaten aşıldığı için {old} → {new}")
        }
        HourChangeReason::TeacherCapacity => {
            format!("{teacher_name} öğretmeninin kapasitesi dolduğu için {old} → {new}")
        }
        HourChangeReason::DailyCap => format!(
            "{teacher_name} için günlük {MAX_HOURS_PER_DAY} saat sınırı (OÖKY MADDE 88) nedeniyle {old} → {new}"
        ),
        HourChangeReason::NoConsecutiveCells => format!(
            "{teacher_name} öğretmeninin {} ardışık boş saati olmadığı için {old} → {new}",
            new + 1
        ),
        _ => format!(
            "Arama sınırına ulaşıldığı için {old} → {new}; yeniden önerip deneyin"
        ),
    }
}

/// Yerleşemeyen işletmenin nedeni. Oynak işletme için kapasite ve havuz asla
/// "yerleşemedi" üretmez (fahri ziyaret onlardan muaf); yalnız hücre yokluğu
/// üretir. KİLİTLİ işletmenin saati sabit olduğundan fahriye düşemez; o
/// yüzden kapasite, ardışık blok ve günlük sınır da gerçek neden olabilir.
pub(super) fn unassigned_reason(state: &State, c: usize) -> (UnassignedReason, String) {
    let p = state.p;
    let company = p.companies[c];
    if p.teachers.is_empty() {
        return (
            UnassignedReason::NoTeachers,
            "Aktif öğretmen yok; önce öğretmen ekleyin".into(),
        );
    }
    if company.workplace_days.is_empty() {
        return (
            UnassignedReason::NoWorkplaceDays,
            "İşletmenin öğrencilerinin işletmede bulunduğu gün yok; sınıf programını kontrol edin"
                .into(),
        );
    }
    let eligible: Vec<usize> = (0..p.teachers.len())
        .filter(|t| p.eligible(*t, c) != 0)
        .collect();
    if eligible.is_empty() {
        let days: Vec<String> = company
            .workplace_days
            .iter()
            .map(|d| day_name(*d))
            .collect();
        return (
            UnassignedReason::NoEligibleCell,
            format!(
                "Hiçbir öğretmenin boş saati işletmenin gün(ler)iyle ({}) kesişmiyor; \
                 öğretmen boş saatlerini ya da işletme günlerini gözden geçirin",
                days.join(", ")
            ),
        );
    }
    if company.is_locked && p.awarded(c) > 0 {
        if let Some(found) = locked_reason(state, c, &eligible) {
            return found;
        }
    }
    (
        UnassignedReason::AllEligibleCellsOccupied,
        "Uygun boş saatlerin hepsi başka işletmelerce dolu; boş saat ekleyin ya da başka bir atamayı kaldırın"
            .into(),
    )
}

/// Kilitli ve yerleşemeyen işletmenin gerçek nedeni; sırayla elenir:
/// saat uzunluğu → kapasite → ardışık blok → günlük sınır.
fn locked_reason(
    state: &State,
    c: usize,
    eligible: &[usize],
) -> Option<(UnassignedReason, String)> {
    let p = state.p;
    let hours = p.awarded(c);
    let name = &p.companies[c].name;
    if hours > MAX_HOURS_PER_DAY || hours > p.day_end - p.day_start {
        return Some((
            UnassignedReason::LockedHoursTooLong,
            format!(
                "{name} işletmesinin kilitli saati ({hours}) günlük {MAX_HOURS_PER_DAY} saati \
                 ya da ders saati ızgarasını aşıyor; kilitli saati düzeltin"
            ),
        ));
    }
    let with_capacity: Vec<usize> = eligible
        .iter()
        .copied()
        .filter(|t| state.teachers[*t].rem_cap >= hours)
        .collect();
    if with_capacity.is_empty() {
        return Some((
            UnassignedReason::NoTeacherCapacity,
            format!(
                "Kilitli {hours} saat, uygun hiçbir öğretmenin kalan kapasitesine sığmıyor; \
                 kapasiteyi artırın ya da başka bir atamayı kaldırın"
            ),
        ));
    }
    if !with_capacity
        .iter()
        .any(|t| state.any_block_position(*t, c, hours, false))
    {
        return Some((
            UnassignedReason::NoConsecutiveBlock,
            format!("Kilitli {hours} saat için ardışık boş saat bulunamadı; öğretmen boş saatlerini gözden geçirin"),
        ));
    }
    if !with_capacity
        .iter()
        .any(|t| state.any_block_position(*t, c, hours, true))
    {
        return Some((
            UnassignedReason::DailyCapReached,
            format!("Kilitli {hours} saat, boş bloğun düştüğü günün {MAX_HOURS_PER_DAY} saatlik sınırını (OÖKY MADDE 88) aşıyor"),
        ));
    }
    None
}
