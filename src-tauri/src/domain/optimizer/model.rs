//! Dağıtım motorunun girdi ve çıktı tipleri.
//!
//! Girdiler yalnızca motorun okuduğu veridir; çıktılar ekrana ve değişiklik
//! kümesine gider, bu yüzden `camelCase` serileştirilir.

use crate::domain::scheduling::Slot;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Motorun çalışma kipi.
///
/// İkisi aynı kod yolundan geçer; yalnızca hangi işletmelerin SABİT sayıldığı
/// değişir (bkz. `problem::is_fixed_company`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProposalMode {
    /// Yalnızca henüz ataması olmayan işletmeleri yerleştirir.
    FillGaps,
    /// Önerilmiş (elle yapılmamış) atamaları yeniden düzenleyebilir.
    Redistribute,
}

/// Bir atamanın kaynağı. Elle yapılan karar Redistribute'ta ASLA taşınmaz;
/// yalnız uygulanmış bir öneriden gelen atama yeniden düzenlenebilir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlacementSource {
    Manual,
    Proposal,
}

/// İşletmenin şu an yürürlükteki ataması.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentPlacement {
    pub teacher_id: i64,
    pub visit_day: i64,
    pub visit_hour: i64,
    pub is_forced: bool,
    pub source: PlacementSource,
}

#[derive(Debug, Clone)]
pub struct CompanyInput {
    pub id: i64,
    pub name: String,
    /// İşletmedeki öğrencilerin dalları.
    pub branches: Vec<String>,
    pub student_count: i64,
    /// Öğrencilerin sınıflarının işletmede bulunduğu günler.
    pub workplace_days: BTreeSet<i64>,
    /// ŞU AN yürürlükteki saat (saat değişikliği kaydında "eski değer").
    pub awarded_hours: i64,
    /// Kullanıcı fahri işaretledi mi? Motor bunu 0'a çakmaz; kilitli değilse
    /// havuz ve kapasite izin verdiğinde saat alabilir.
    pub is_honorary: bool,
    /// Saat tavanı: komut katmanı anlık tavan ile anlık görüntü tavanının
    /// küçüğünü verir.
    pub max_hours: i64,
    pub is_locked: bool,
    /// Bölge anahtarı; `None` = gruplanmamış. Yalnız bölünme sayımında kullanılır.
    pub group_key: Option<String>,
    /// Mesajlarda gösterilen bölge adı; grupsuzda boş.
    pub group_label: String,
    pub current: Option<CurrentPlacement>,
}

#[derive(Debug, Clone)]
pub struct TeacherInput {
    pub id: i64,
    pub name: String,
    pub branches: Vec<String>,
    /// Koordinatörlük kapasitesi (MADDE 15/2 tavanı eksi şeflik ve diğer ek dersler).
    pub capacity: i64,
    pub free_slots: BTreeSet<Slot>,
    /// `companies` içinde OLMAYAN işletmelerden gelen yük; çifte sayım yok.
    pub base_assigned_hours: i64,
    pub base_used_slots: BTreeSet<Slot>,
    pub base_hours_by_day: BTreeMap<i64, i64>,
}

#[derive(Debug, Clone)]
pub struct EngineInput<'a> {
    pub companies: &'a [CompanyInput],
    pub teachers: &'a [TeacherInput],
    pub day_start_hour: i64,
    /// HARİÇ üst sınır.
    pub day_end_hour: i64,
    /// Dağıtılabilir toplam saat. `<= 0` → tanımsız, kısıtsız.
    pub pool_hours: i64,
    pub mode: ProposalMode,
    /// Öğretmenler arası saat farkı bu eşiği aşarsa eşitlik, bölge
    /// bütünlüğünün önüne geçer (varsayılan 4, komut katmanında).
    pub balance_gap_hours: i64,
}

/// Yalnızca DEĞİŞEN ya da YENİ yerleşimler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedAssignment {
    pub company_id: i64,
    pub company_name: String,
    pub teacher_id: i64,
    pub teacher_name: String,
    pub awarded_hours: i64,
    pub previous_hours: i64,
    pub visit_day: i64,
    pub visit_hour: i64,
    /// DAHİL son ders saati.
    pub visit_end_hour: i64,
    pub exact_branch_match: bool,
    pub group_key: Option<String>,
    pub previous: Option<CurrentPlacement>,
}

/// Yerinde kalan atama (sabit ya da motorun dokunmadığı).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeptAssignment {
    pub company_id: i64,
    pub company_name: String,
    pub teacher_id: i64,
    pub visit_day: i64,
    pub visit_hour: i64,
    pub awarded_hours: i64,
    pub is_locked: bool,
    pub is_forced: bool,
}

/// Saat değişikliğinin nedeni. `kind` alanıyla etiketlenir; `MadeRoomFor`
/// ayrıca `companyId` taşır.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum HourChangeReason {
    PoolExhausted,
    PoolAlreadyOverrun,
    TeacherCapacity,
    DailyCap,
    NoConsecutiveCells,
    MadeRoomFor {
        company_id: i64,
    },
    CeilingLowered,
    /// Saat ARTTI (ör. kullanıcı-fahri işletmeye saat verildi).
    Granted,
    /// İşletme yerleştirilemedi; saati havuza geri döndü (old → 0).
    Unplaced,
    /// Arama sayaç sınırına takıldı; saat daha da artırılabilirdi.
    SearchLimit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HourChange {
    pub company_id: i64,
    pub company_name: String,
    pub old_hours: i64,
    pub new_hours: i64,
    pub reason_code: HourChangeReason,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupSplitPart {
    pub group_key: String,
    pub group_label: String,
    pub company_names: Vec<String>,
}

/// Birden çok bölgeye yayılmış öğretmen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupSplit {
    pub teacher_id: i64,
    pub teacher_name: String,
    pub groups: Vec<GroupSplitPart>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UnassignedReason {
    NoTeachers,
    NoWorkplaceDays,
    NoEligibleCell,
    AllEligibleCellsOccupied,
    /// Kilitli işletmenin saati hiçbir öğretmenin kalan kapasitesine sığmıyor.
    NoTeacherCapacity,
    /// Kilitli saat kadar ARDIŞIK boş hücre yok.
    NoConsecutiveBlock,
    /// Kilitli saat, boş bloğun düştüğü günün 8 saat sınırını aşıyor.
    DailyCapReached,
    /// Kilitli saat günlük 8 saati ya da ızgara uzunluğunu aşıyor.
    LockedHoursTooLong,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnassignedCompany {
    pub company_id: i64,
    pub company_name: String,
    pub reason_code: UnassignedReason,
    pub reason: String,
    /// Girdide bir ataması var mıydı (Redistribute'ta serbest bırakıldı)?
    pub was_assigned: bool,
}

/// Yalnız Redistribute: ataması sona erdirilecek işletme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleasedAssignment {
    pub company_id: i64,
    pub company_name: String,
    pub previous: CurrentPlacement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeacherLoadSummary {
    pub teacher_id: i64,
    pub teacher_name: String,
    /// Taban yük dahil toplam saat.
    pub hours: i64,
    pub capacity: i64,
    pub company_count: i64,
    pub distinct_groups: i64,
}

/// Uygulama sözleşmesi: önce `hour_changes`, sonra `assignments`, sonra
/// `released` — tek değişiklik kümesi olarak.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationProposal {
    pub mode: ProposalMode,
    pub assignments: Vec<ProposedAssignment>,
    pub kept: Vec<KeptAssignment>,
    pub released: Vec<ReleasedAssignment>,
    pub hour_changes: Vec<HourChange>,
    pub group_splits: Vec<GroupSplit>,
    pub unassigned: Vec<UnassignedCompany>,
    pub teacher_loads: Vec<TeacherLoadSummary>,
    pub placed_count: i64,
    /// Yürürlükteki saatlerin toplamı: yerleşen işletmeler (sabit ve oynak)
    /// artı yerleşemese bile kayıtta kalan KİLİTLİ işletmelerin saati.
    pub total_hours: i64,
    pub pool_hours: i64,
    /// Havuz tanımsızsa `None`; aşıldıysa negatif.
    pub pool_remaining: Option<i64>,
    pub warnings: Vec<String>,
}
