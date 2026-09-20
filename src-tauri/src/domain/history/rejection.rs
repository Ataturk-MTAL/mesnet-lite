use chrono::NaiveDate;
use serde::Serialize;

/// `decide` ve tarih çözümlemesinin döndürdüğü red kodu.
///
/// Arayüze `ChangeOutcome::Rejected.code` alanı olarak camelCase serileşir
/// (spec §8, "rejected.code değerleri" tablosu). Değerler o tabloyla birebir
/// aynı sırada değil ama aynı adlarla tutulur.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RejectionCode {
    /// Yürürlük tarihi önceki aya düşüyor. Ek ders puantajı ay sonunda
    /// mutemede verilip en geç ayın 3'ünde ilçeye gittiği için önceki ayın
    /// puantajı değişemez (spec §2 madde 3). `suggested_date` bu ayın 1'idir.
    PreviousMonthClosed,
    /// Dönem başladı ama yürürlük tarihi girilmedi.
    EffectiveDateRequired,
    /// Yürürlük tarihi dönem aralığının (`[start, end]`) dışında.
    OutOfTerm,
    /// Ör. "öğrenci o tarihte o işletmede değil" — önceki durum tutmuyor.
    FactNotTrueAtDate,
    /// Aday olay, `from_*` bekleyen sonraki bir kayıtla çelişiyor.
    /// `conflicting_change_set_ids` çelişen kümeyi taşır.
    ConflictsWithLaterChange,
    /// Elle girilen saat artışı, sunucunun hesapladığı tavanı aşıyor.
    AboveCap,
    /// Aynı öğretmende tarih aralıkları örtüşen bir blok var.
    BlockOverlap,
    /// Açılış kümesi, geri alma kümesi ya da önceki aya düşen küme
    /// geri alınamaz.
    NotRevocable,
    /// Hedefe bağlı sonraki bir kayıt önce geri alınmalı.
    /// `conflicting_change_set_ids` bağlı kümeyi taşır.
    HasDependents,
    /// Kaydın açılış dışında geçmişi var; silinemez, pasif yapılabilir.
    HasHistory,
    /// Bu işlem yalnız planlama evresinde (dönem başlamadan) yapılabilir.
    PlanningOnly,
    /// Sınırda doğrulama: komuttaki bir kimlik (`studentId`/`companyId`/
    /// `teacherId`/`changeSetId`) bağlamda yok, yerinde oluşturulması
    /// gereken bir kayıt oluşturulamadı, ya da hedef pasif bir işletme
    /// (R2b brief madde 3). `0`'a düşen sahte varsayılan ya da yer tutucu
    /// etiketle sessizce devam etmek yerine burada reddedilir.
    InvalidRequest,
    /// Okulda aynı anda en fazla bir öğretmen alan şefi (`department`)
    /// olabilir; komut, başka bir öğretmenin şeflik aralığıyla çakışan
    /// yeni bir `department` günü yaratıyor. `conflicting_change_set_ids`,
    /// o öğretmenin şefliğini kuran değişiklik kümesini taşır.
    ChiefAlreadyAssigned,
}

/// `decide` ve `resolve_effective_date`'in alan kuralı reddi.
///
/// `AppError` DEĞİLDİR: bu bir I/O ya da programlama hatası değil, iş
/// kuralının bilinçli sonucudur — esrs deseninde `Result<Decision, Rejection>`
/// ile `AppResult<..>`'ın iki katmanlı ayrımı budur (spec §3).
#[derive(Debug, Clone, PartialEq)]
pub struct Rejection {
    pub code: RejectionCode,
    pub message: String,
    pub conflicting_change_set_ids: Vec<i64>,
    pub suggested_date: Option<NaiveDate>,
}

impl Rejection {
    /// Ek alanı olmayan basit bir red oluşturur.
    pub fn new(code: RejectionCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            conflicting_change_set_ids: Vec::new(),
            suggested_date: None,
        }
    }

    /// `suggested_date` dolu bir red (ör. `PreviousMonthClosed`).
    pub fn with_suggested_date(mut self, date: NaiveDate) -> Self {
        self.suggested_date = Some(date);
        self
    }

    /// `conflicting_change_set_ids` dolu bir red (ör. `ConflictsWithLaterChange`).
    pub fn with_conflicts(mut self, ids: Vec<i64>) -> Self {
        self.conflicting_change_set_ids = ids;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Arayüz `code` alanını bu birebir metinle karşılaştırır (spec §8).
    #[test]
    fn chief_already_assigned_serializes_as_camel_case() {
        let json = serde_json::to_string(&RejectionCode::ChiefAlreadyAssigned).unwrap();
        assert_eq!(json, "\"chiefAlreadyAssigned\"");
    }
}
