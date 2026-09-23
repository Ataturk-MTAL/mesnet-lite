//! Tarihçeden ETKİSİZ kayıt silme kuralı (kullanıcı kararı 2026-09-23).
//!
//! "Silinebilir mi?" `decide`'ın bir komutu DEĞİLDİR — geri almadan (bkz.
//! `history_service.rs::is_revocable`) AYRI bir ikinci sorudur: geri alma
//! AKTİF bir kaydı iptal eder, silme ise ZATEN etkisiz kalmış (tamamen geri
//! alınmış) bir kaydı günlükten fiziksel olarak temizler. Tek doğruluk
//! kaynağı burasıdır; hem `services/history_service.rs` (görüntülemede
//! `isDeletable` bayrağı) hem `services/history_delete.rs` (silmeden HEMEN
//! önceki zorunlu yeniden doğrulama) AYNI fonksiyonu çağırır — kural iki
//! yere kopyalanmaz.

use std::collections::BTreeSet;

use super::events::{EventPayload, StoredEvent};

/// Bir değişiklik kümesi (S) silinebilir ⇔
///
/// 1. `S.kind != "opening"` — açılış kümesi asla silinemez, dönemin
///    başlangıç durumu onsuz tanımsız kalırdı.
/// 2. S'nin HİÇ işaret (`Revoked` yükü) olayı yoktur — S bir başkasını geri
///    almışsa, S'yi silmek o geri almayı da kaybettirir ve bugünkü durum
///    değişirdi.
/// 3. S'nin işaret-olmayan HER olayı başka bir işaretin hedefidir, yani
///    ZATEN geri alınmıştır — S'nin gerçek etkisinden bugün hiçbir şey
///    kalmamıştır (öğretmen ek ders SAATİ dahil).
///
/// `events` dönemin TÜM olaylarıdır (`db::change_log::load_term_events`);
/// yalnız S'nin öz olayları değil, S'yi hedefleyen işaretleri bulmak için de
/// tüm günlük gerekir.
pub fn is_deletable(events: &[StoredEvent], change_set_id: i64, kind: &str) -> bool {
    if kind == "opening" {
        return false;
    }

    let own: Vec<&StoredEvent> = events.iter().filter(|e| e.change_set_id == change_set_id).collect();
    if own.iter().any(|e| matches!(e.payload, EventPayload::Revoked)) {
        return false;
    }

    let revoked_target_ids: BTreeSet<i64> = events.iter().filter_map(|e| e.revokes).collect();
    own.iter().all(|e| revoked_target_ids.contains(&e.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::history::events::{Labels, Stream};
    use chrono::NaiveDate;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn labels() -> Labels {
        Labels(Default::default())
    }

    fn effect_event(id: i64, change_set_id: i64) -> StoredEvent {
        StoredEvent {
            id,
            change_set_id,
            stream: Stream::CompanyHours,
            subject_id: 1,
            term: "2026-2027/1".into(),
            effective_date: ymd(2026, 11, 3),
            payload: EventPayload::HoursCapped { cap: 4, student_count: 1, labels: labels() },
            caused_by: None,
            revokes: None,
            is_opening: false,
        }
    }

    fn marker_event(id: i64, change_set_id: i64, revokes: i64) -> StoredEvent {
        StoredEvent {
            id,
            change_set_id,
            stream: Stream::CompanyHours,
            subject_id: 1,
            term: "2026-2027/1".into(),
            effective_date: ymd(2026, 11, 5),
            payload: EventPayload::Revoked,
            caused_by: None,
            revokes: Some(revokes),
            is_opening: false,
        }
    }

    /// Açılış hiçbir koşulda silinemez — diğer koşullar sağlansa bile.
    #[test]
    fn opening_is_never_deletable() {
        let events = vec![effect_event(1, 10)];
        assert!(!is_deletable(&events, 10, "opening"));
    }

    /// Hiçbir olayı geri alınmamış bir küme silinemez: bugünkü durumu hâlâ
    /// o belirliyor.
    #[test]
    fn a_set_whose_event_is_still_live_is_not_deletable() {
        let events = vec![effect_event(1, 10)];
        assert!(!is_deletable(&events, 10, "set_company_hours"));
    }

    /// TÜM olayları başka kümelerdeki işaretlerce geri alınmış bir küme
    /// silinebilir.
    #[test]
    fn a_fully_revoked_set_is_deletable() {
        let events = vec![effect_event(1, 10), marker_event(2, 20, 1)];
        assert!(is_deletable(&events, 10, "set_company_hours"));
        // Markörü taşıyan küme (20) kendisi silinebilir DEĞİLDİR (kural 2).
        assert!(!is_deletable(&events, 20, "revoke"));
    }

    /// Bir kümenin İKİ olayından yalnız BİRİ geri alınmışsa küme SİLİNEMEZ —
    /// kural 3 kümenin HER olayını ister.
    #[test]
    fn a_partially_revoked_set_is_not_deletable() {
        let events = vec![effect_event(1, 10), effect_event(2, 10), marker_event(3, 20, 1)];
        assert!(!is_deletable(&events, 10, "set_company_hours"));
    }

    /// Salt geri-alma kümesi (yalnız işaret olayı taşır) silinebilir
    /// DEĞİLDİR — kural 2, kendi işareti yüzünden.
    #[test]
    fn a_pure_revoke_set_is_not_deletable() {
        let events = vec![effect_event(1, 10), marker_event(2, 20, 1)];
        assert!(!is_deletable(&events, 20, "revoke"));
    }
}
