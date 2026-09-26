// Rust tarafındaki serde camelCase çıktısının birebir karşılığı.

/** Harita üzerinde bir nokta. */
export interface LatLng {
  latitude: number
  longitude: number
}

export type GeocodeStatus = 'pending' | 'resolved' | 'failed' | 'manual'
export type ChiefType = 'none' | 'workshop_lab' | 'department'
export type EmploymentType = 'tenured' | 'contracted'

export interface Company {
  id: number
  name: string
  contactFirstName: string
  contactLastName: string
  phone: string
  email: string
  addressText: string
  /** Adresten türetilen veya elle girilen ilçe. Ayrıştırılamamışsa boş dize. */
  district: string
  latitude: number | null
  longitude: number | null
  geocodeStatus: GeocodeStatus
  /** TEK YÖN yol mesafesi. Saat tavanı kuralları bunun iki katını kullanır. */
  oneWayDistanceKm: number | null
  notes: string
  createdAt: string
  updatedAt: string
}

export interface NewCompany {
  name: string
  contactFirstName: string
  contactLastName: string
  phone: string
  email: string
  addressText: string
  /** Boş gönderilirse arka uç adresten türetir; doluysa aynen korunur. */
  district: string
  latitude: number | null
  longitude: number | null
  oneWayDistanceKm: number | null
  notes: string
}

/** Saat tavanı kurallarında kullanılan gidiş-dönüş mesafesi. */
export function roundTripDistanceKm(company: Company): number | null {
  return company.oneWayDistanceKm === null ? null : company.oneWayDistanceKm * 2
}

/**
 * `delete_company` yanıtı. Geçmişi olan işletme gerçekten silinmez, pasife
 * alınır ve listeden düşer; `softDeleted` bu iki durumu ayırt eder.
 */
export interface CompanyRemoval {
  softDeleted: boolean
}

export interface Teacher {
  id: number
  firstName: string
  lastName: string
  registryNo: string
  field: string
  /** JSON dizi metni olarak saklanır; okumak için parseBranches kullanın. */
  branches: string
  employmentType: EmploymentType
  baseHours: number
  maxExtraHours: number
  otherExtraHours: number
  chiefType: ChiefType
  /** SQLite'ta boolean yoktur; 0 veya 1. */
  isActive: number
}

/** Öğretmen + mevzuattan türetilen kapasite. Kapasite Rust tarafında hesaplanır. */
export interface TeacherWithCapacity extends Teacher {
  chiefHours: number
  statutoryCap: number
  capacity: number
}

export interface NewTeacher {
  firstName: string
  lastName: string
  registryNo: string
  field: string
  branches: string[]
  employmentType: EmploymentType
  baseHours: number
  maxExtraHours: number
  otherExtraHours: number
  chiefType: ChiefType
  isActive: boolean
}

// ---------------------------------------------------------------------------
// PIN'li kullanıcı girişi — kayıt tutma amaçlı, yetkilendirme YOK. 2-3 kişilik
// ekibin tamamı tam yetkilidir; amaç yalnızca "değişikliği kim yaptı" bilgisini
// tarihçeye düşürmektir.
// ---------------------------------------------------------------------------

/** Rust tarafındaki `UserSummary` ile birebir; PIN hiçbir zaman arayüze gelmez. */
export interface User {
  id: number
  name: string
  isActive: boolean
}

// ---------------------------------------------------------------------------
// Yedekleme — uygulama her açılışta günlük otomatik yedek alır; Ayarlar
// ekranındaki "Yedekleme" kartı bu durumu gösterir ve elle yedek/geri yükleme
// akışlarını tetikler.
// ---------------------------------------------------------------------------

/** `backup_status` komutunun döndürdüğü durum; `lastBackupAt` 'YYYY-MM-DD' biçiminde. */
export interface BackupStatus {
  backupDir: string
  lastBackupAt: string | null
  backupCount: number
}

export interface NewStudent {
  firstName: string
  lastName: string
  studentNo: string | null
  grade: string
  branch: string
  submittedAt: string | null
  /** Boş bırakılırsa backend aktif dönemi yazar. */
  term: string
}

export interface Student {
  id: number
  firstName: string
  lastName: string
  studentNo: string | null
  grade: string
  branch: string
  companyId: number | null
  submittedAt: string | null
  term: string
}

/** `branches` bozuk JSON içerse bile okuma yolu düşmemeli. */
export function parseBranches(raw: string): string[] {
  try {
    const parsed: unknown = JSON.parse(raw)
    return Array.isArray(parsed) ? parsed.filter((b): b is string => typeof b === 'string') : []
  } catch {
    return []
  }
}

// ---------------------------------------------------------------------------
// Dönem içi değişiklik tarihçesi — spec §8 (Tauri sözleşmesi) ile birebir.
// Kaynak: docs/superpowers/specs/2026-09-19-donem-ici-degisiklik-tarihce-design.md
// ---------------------------------------------------------------------------

/** Beş tarihçe akışından biri; `Stream::as_str()` ile birebir aynı metinler. */
export type Stream = 'placement' | 'company_hours' | 'coordination' | 'teacher_load' | 'teacher_schedule'

/**
 * `EventPayload::kind()` çıktısı; 12 tür (11 olay + geri alma işareti).
 * Etiketleri için bkz. `labels.history.eventKind`.
 */
export type EventKind =
  | 'student_placed'
  | 'student_transferred'
  | 'student_left'
  | 'hours_set'
  | 'hours_capped'
  | 'hours_cleared'
  | 'coordinator_assigned'
  | 'coordinator_ended'
  | 'coordinator_ended_by_policy'
  | 'load_set'
  | 'schedule_set'
  | 'revoked'

/** `rejected.code` değerleri (spec §8). */
export type RejectionCode =
  | 'previousMonthClosed'
  | 'effectiveDateRequired'
  | 'outOfTerm'
  | 'factNotTrueAtDate'
  | 'conflictsWithLaterChange'
  | 'aboveCap'
  | 'blockOverlap'
  | 'notRevocable'
  | 'hasDependents'
  | 'hasHistory'
  | 'planningOnly'
  | 'invalidRequest'
  | 'chiefAlreadyAssigned'
  | 'poolExceeded'

/** `ImpactWarning.code` değerleri. */
export type WarningCode =
  | 'lockedAboveCap'
  | 'capacityExceeded'
  | 'dailyCapExceeded'
  | 'blockOutsideFreeSlots'
  | 'capUnknown'

/** `ImpactNotice.code` değerleri. */
export type NoticeCode = 'capIncreased' | 'reducedBelowCap' | 'newCompanyNeedsSetup' | 'futureDated' | 'shadowed'

/** Öğrenci oluşturma komutunun taşıdığı alanlar; `companyId` ve `term` komut zarfında ayrıca yer alır. */
export interface NewStudentInput {
  firstName: string
  lastName: string
  studentNo: string | null
  grade: string
  branch: string
  submittedAt: string | null
}

/** `assignCoordinators.rows` öğesi. */
export interface CoordinatorAssignmentRow {
  companyId: number
  teacherId: number
  visitDay: number
  visitHour: number
  isForced: boolean
  forceReason: string | null
}

/** `setCompanyHours.rows` öğesi. */
export interface CompanyHoursRow {
  companyId: number
  awardedHours: number
  isHonorary: boolean
  isLocked: boolean
  notes: string
}

/**
 * `createTeacher.teacher`: yalnız kimlik bilgisi. Yük alanları tarihe bağlı
 * olduğu için ayrı `load` alanında gelir; burada tekrarlanmaz.
 */
export interface NewTeacherProfile {
  firstName: string
  lastName: string
  registryNo: string
  field: string
  branches: string[]
  isActive: boolean
}

/** `createTeacher.load` ve `setTeacherLoad.load`. */
export interface TeacherLoadInput {
  baseHours: number
  maxExtraHours: number
  otherExtraHours: number
  chiefType: ChiefType
  employmentType: EmploymentType
}

/** `setTeacherSchedule.slots` öğesi. */
export interface ScheduleSlot {
  dayOfWeek: number
  hour: number
}

/** `transferStudent.to`. */
export type TransferTarget = { type: 'existing'; companyId: number } | { type: 'new'; company: NewCompany }

/**
 * Spec §8'deki 16 komut türü, `type` etiketiyle ayırt edilir. `correct.replacement`
 * bu birleşimin tamamına başvurur; spec metni "yukarıdakilerden biri" der ve
 * `revoke` / `correct`'i dışlamaz.
 */
export type ChangeCommand =
  | { type: 'createStudent'; student: NewStudentInput; companyId: number | null }
  | { type: 'placeStudent'; studentId: number; to: TransferTarget }
  | { type: 'transferStudent'; studentId: number; fromCompanyId: number; to: TransferTarget }
  | { type: 'studentLeaves'; studentId: number; fromCompanyId: number }
  | { type: 'deleteStudent'; studentId: number }
  | { type: 'setCompanyHours'; rows: CompanyHoursRow[] }
  | { type: 'assignCoordinators'; rows: CoordinatorAssignmentRow[] }
  | { type: 'endCoordination'; companyId: number }
  | { type: 'clearCoordination' }
  | { type: 'createTeacher'; teacher: NewTeacherProfile; load: TeacherLoadInput }
  | { type: 'setTeacherLoad'; teacherId: number; load: TeacherLoadInput }
  | { type: 'setTeacherSchedule'; teacherId: number; slots: ScheduleSlot[] }
  | { type: 'copySchedulesFromTerm'; fromTerm: string }
  | { type: 'deleteTeacher'; teacherId: number }
  | { type: 'revoke'; changeSetId: number }
  | { type: 'correct'; changeSetId: number; replacement: ChangeCommand }

/**
 * Doğrudan yazım komutlarına (saat kaydı, atama/çıkarma, toplu silme) eklenen
 * isteğe bağlı yürürlük tarihi ve gerekçe. Bu komutlar `preview_change` /
 * `commit_change` akışından GEÇMEZ; arka uç kaydı kendi içinde tarihçeye
 * yazar. Dönem planlama evresindeyse (`isPlanning === true`) ikisi de `null`
 * gönderilir.
 */
export interface EffectiveChangeInput {
  effectiveDate: string | null
  reason: string | null
}

/** `preview_change` / `commit_change` isteği. */
export interface ChangeRequest {
  term: string
  effectiveDate: string | null
  documentDate: string | null
  reason: string
  command: ChangeCommand
}

/** Etki penceresinde birincil ya da otomatik bir satır. */
export interface ImpactLine {
  kind: EventKind
  stream: Stream
  subjectId: number
  subjectLabel: string
  effectiveDate: string
  before: string | null
  after: string | null
}

export interface ImpactWarning {
  code: WarningCode
  message: string
  subjectLabel: string
  fromDate: string
  toDate: string | null
}

export interface ImpactNotice {
  code: NoticeCode
  message: string
  subjectLabel: string
  date: string
}

export interface ImpactSummary {
  effectiveDate: string
  isPlanning: boolean
  shadowedUntil: string | null
  primary: ImpactLine[]
  automatic: ImpactLine[]
  warnings: ImpactWarning[]
  notices: ImpactNotice[]
}

/** `preview_change` / `commit_change` yanıtı; `status` ile ayırt edilir. */
export type ChangeOutcome =
  | {
      status: 'rejected'
      code: RejectionCode
      reason: string
      conflictingChangeSetIds: number[]
      suggestedDate: string | null
    }
  | { status: 'stale'; message: string }
  | { status: 'preview'; impact: ImpactSummary; highWater: number }
  | { status: 'committed'; changeSetId: number; impact: ImpactSummary }

/** `list_history` isteği. */
export interface HistoryFilter {
  term: string
  stream: Stream | null
  subjectId: number | null
  companyId: number | null
  teacherId: number | null
  includeOpening: boolean
  beforeChangeSetId: number | null
  limit: number
}

/** Bir değişiklik kümesi içindeki tek bir olay; denetim satırı. */
export interface HistoryEventEntry {
  eventId: number
  stream: Stream
  subjectId: number
  subjectLabel: string
  kind: EventKind
  effectiveDate: string
  before: string | null
  after: string | null
  causedByEventId: number | null
  isRevoked: boolean
}

/** Tarihçe listesindeki bir değişiklik kümesi. */
export interface HistoryChangeSetEntry {
  changeSetId: number
  recordedAt: string
  kind: string
  reason: string
  actor: string
  effectiveDate: string
  documentDate: string | null
  revokedByChangeSetId: number | null
  revokesChangeSetId: number | null
  isRevocable: boolean
  /** Yalnızca bugünkü durumu değiştirmeyen (tüm olayları geri alınmış, opening olmayan) kayıtlarda true. */
  isDeletable: boolean
  warnings: ImpactWarning[]
  events: HistoryEventEntry[]
}

/** `list_history` yanıtı; sayfalama `nextBeforeChangeSetId` ile ilerler. */
export interface HistoryResponse {
  entries: HistoryChangeSetEntry[]
  nextBeforeChangeSetId: number | null
}

/** `list_terms_with_dates` yanıtındaki tek dönem. */
export interface TermWithDates {
  term: string
  startDate: string
  endDate: string
  datesConfirmed: boolean
  isPlanning: boolean
  defaultAsOf: string
  earliestAllowedDate: string
}

/** `update_term_dates` isteği. */
export interface UpdateTermDatesInput {
  term: string
  startDate: string
  endDate: string
  confirm: boolean
}
