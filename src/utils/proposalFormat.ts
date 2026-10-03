import { labels } from '../i18n/labels'
import type { AllocationProposal, CurrentPlacement } from '../api/assignments'

/** "Salı 10–12" ya da tek saatlik blokta "Salı 10". `endHour` uç DAHİL. */
export function slotLabel(day: number, startHour: number, endHour: number): string {
  const dayName = labels.allocation.days[day] ?? ''
  return endHour > startHour ? `${dayName} ${startHour}–${endHour}` : `${dayName} ${startHour}`
}

/** Önceki yerleşimin etiketi; bitiş saati önceki saatten (fahri = 1 hücre) türetilir. */
export function previousSlotLabel(previous: CurrentPlacement, previousHours: number): string {
  const endHour = previous.visitHour + Math.max(1, previousHours) - 1
  return slotLabel(previous.visitDay, previous.visitHour, endHour)
}

/** Öğretmen kimliğinden ad; öneri yükleri tüm öğretmenleri taşır, bulunamazsa boş dize. */
export function teacherNameMap(proposal: AllocationProposal): Map<number, string> {
  return new Map(proposal.teacherLoads.map((load) => [load.teacherId, load.teacherName]))
}
