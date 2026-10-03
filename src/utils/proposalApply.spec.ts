import { describe, expect, it } from 'vitest'
import { buildApplyProposalCommand, hasNoChanges } from './proposalApply'
import { proposalFixture, proposedAssignmentFixture } from '../components/allocation/proposalFixture'
import type { HoursRow } from '../api/hours'

function hoursRow(overrides: Partial<HoursRow>): HoursRow {
  return {
    companyId: 1,
    companyName: 'Firma A',
    addressText: '',
    oneWayDistanceKm: null,
    roundTripDistanceKm: null,
    studentCount: 1,
    maxHours: 8,
    awardedHours: 4,
    isHonorary: false,
    isLocked: false,
    notes: '',
    isSaved: true,
    ...overrides,
  }
}

describe('buildApplyProposalCommand', () => {
  it('saat satırında mevcut kilit ve notu korur, 0 saati fahri işaretler', () => {
    // Arrange
    const proposal = proposalFixture({
      hourChanges: [
        { companyId: 1, companyName: 'Firma A', oldHours: 4, newHours: 0, reasonCode: { kind: 'poolExhausted' }, reason: 'x' },
        { companyId: 2, companyName: 'Firma B', oldHours: 2, newHours: 3, reasonCode: { kind: 'granted' }, reason: 'y' },
      ],
    })
    const rows = [
      hoursRow({ companyId: 1, isLocked: true, notes: 'Not A' }),
      hoursRow({ companyId: 2, companyName: 'Firma B', isLocked: false, notes: 'Not B' }),
    ]

    // Act
    const result = buildApplyProposalCommand(proposal, rows)

    // Assert
    expect(result).toEqual({
      ok: true,
      command: {
        type: 'applyProposal',
        hours: [
          { companyId: 1, awardedHours: 0, isHonorary: true, isLocked: true, notes: 'Not A' },
          { companyId: 2, awardedHours: 3, isHonorary: false, isLocked: false, notes: 'Not B' },
        ],
        assign: [],
        release: [],
      },
    })
  })

  it('atamayı zorlamasız gönderir, kept satırını atamaya katmaz, bırakılanların kimliklerini verir', () => {
    // Arrange
    const proposal = proposalFixture({
      assignments: [proposedAssignmentFixture({ companyId: 7, teacherId: 3, visitDay: 2, visitHour: 10 })],
      kept: [
        { companyId: 8, companyName: 'Firma H', teacherId: 3, visitDay: 1, visitHour: 9, awardedHours: 2, isLocked: false, isForced: true },
      ],
      released: [
        { companyId: 5, companyName: 'Firma E', previous: { teacherId: 1, visitDay: 1, visitHour: 9, isForced: false, source: 'proposal' } },
        { companyId: 6, companyName: 'Firma F', previous: { teacherId: 1, visitDay: 2, visitHour: 9, isForced: false, source: 'proposal' } },
      ],
    })

    // Act
    const result = buildApplyProposalCommand(proposal, [])

    // Assert
    expect(result).toEqual({
      ok: true,
      command: {
        type: 'applyProposal',
        hours: [],
        assign: [{ companyId: 7, teacherId: 3, visitDay: 2, visitHour: 10, isForced: false, forceReason: null }],
        release: [5, 6],
      },
    })
  })

  it('saat kaydı bulunmayan işletme için komut kurmaz', () => {
    const proposal = proposalFixture({
      hourChanges: [{ companyId: 4, companyName: 'Firma D', oldHours: 1, newHours: 2, reasonCode: { kind: 'granted' }, reason: 'z' }],
    })

    expect(buildApplyProposalCommand(proposal, [])).toEqual({ ok: false, missingHoursFor: ['Firma D'] })
  })
})

describe('hasNoChanges', () => {
  it('yalnız kept, uyarı ya da yerleşemeyen varsa değişiklik yok sayılır', () => {
    expect(hasNoChanges(proposalFixture({ warnings: ['u'] }))).toBe(true)
    expect(hasNoChanges(proposalFixture({ assignments: [proposedAssignmentFixture()] }))).toBe(false)
  })
})
