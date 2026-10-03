import type { AllocationProposal } from '../api/assignments'
import type { HoursRow } from '../api/hours'
import type { ChangeCommand, CompanyHoursRow, CoordinatorAssignmentRow } from '../types/models'

/** `applyProposal` komutu; `ChangeCommand` birleşiminin ilgili kolu. */
export type ApplyProposalCommand = Extract<ChangeCommand, { type: 'applyProposal' }>

/** Önerinin hiçbir yazılacak değişikliği yoksa true (yeni atama, saat değişikliği, bırakma). */
export function hasNoChanges(proposal: AllocationProposal): boolean {
  return (
    proposal.assignments.length === 0 &&
    proposal.hourChanges.length === 0 &&
    proposal.released.length === 0
  )
}

/** Komut kurulamadıysa: saat satırı bulunamayan işletmelerin adları. */
export type BuildResult =
  | { ok: true; command: ApplyProposalCommand }
  | { ok: false; missingHoursFor: string[] }

/**
 * Öneriden tek değişiklik kümesi komutu üretir. Saat satırlarında `isLocked` ve
 * `notes` işletmenin MEVCUT saat kaydından alınır (kaybedilmez); kaydı bulunmayan
 * bir işletme için kilit/not tahmin edilmez, sonuç `ok: false` olur.
 * `kept` satırları atamaya girmez; yalnız saatleri değiştiyse `hours`'ta yer alırlar.
 */
export function buildApplyProposalCommand(
  proposal: AllocationProposal,
  hoursRows: readonly HoursRow[],
): BuildResult {
  const currentByCompany = new Map(hoursRows.map((row) => [row.companyId, row]))

  const missingHoursFor = proposal.hourChanges
    .filter((change) => !currentByCompany.has(change.companyId))
    .map((change) => change.companyName)
  if (missingHoursFor.length > 0) return { ok: false, missingHoursFor }

  const hours = proposal.hourChanges.flatMap((change): CompanyHoursRow[] => {
    const current = currentByCompany.get(change.companyId)
    if (current === undefined) return []
    return [
      {
        companyId: change.companyId,
        awardedHours: change.newHours,
        isHonorary: change.newHours === 0,
        isLocked: current.isLocked,
        notes: current.notes,
      },
    ]
  })

  const assign = proposal.assignments.map(
    (item): CoordinatorAssignmentRow => ({
      companyId: item.companyId,
      teacherId: item.teacherId,
      visitDay: item.visitDay,
      visitHour: item.visitHour,
      isForced: false,
      forceReason: null,
    }),
  )

  return {
    ok: true,
    command: {
      type: 'applyProposal',
      hours,
      assign,
      release: proposal.released.map((item) => item.companyId),
    },
  }
}
