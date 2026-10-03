import type { AllocationProposal, ProposedAssignment } from '../../api/assignments'

/** Yalnız testlerde kullanılan KURGUSAL öneri verisi. */
export function proposedAssignmentFixture(overrides: Partial<ProposedAssignment> = {}): ProposedAssignment {
  return {
    companyId: 1,
    companyName: 'Firma A',
    teacherId: 1,
    teacherName: 'Ahmet Yılmaz',
    awardedHours: 2,
    previousHours: 2,
    visitDay: 1,
    visitHour: 9,
    visitEndHour: 10,
    exactBranchMatch: true,
    groupKey: 'district:Akdeniz',
    previous: null,
    ...overrides,
  }
}

export function proposalFixture(overrides: Partial<AllocationProposal> = {}): AllocationProposal {
  return {
    mode: 'fillGaps',
    assignments: [],
    kept: [],
    released: [],
    hourChanges: [],
    groupSplits: [],
    unassigned: [],
    teacherLoads: [
      {
        teacherId: 1,
        teacherName: 'Ahmet Yılmaz',
        hours: 4,
        capacity: 20,
        companyCount: 2,
        distinctGroups: 1,
      },
    ],
    placedCount: 0,
    totalHours: 0,
    poolHours: 100,
    poolRemaining: 100,
    warnings: [],
    ...overrides,
  }
}

/** Her bölümü dolduran öneri: yeni, taşınan, saat değişikliği (0'a inen dahil), bırakma, yerleşemeyen, bölünme, uyarı. */
export function fullProposalFixture(): AllocationProposal {
  return proposalFixture({
    assignments: [
      proposedAssignmentFixture(),
      proposedAssignmentFixture({
        companyId: 2,
        companyName: 'Firma B',
        visitDay: 3,
        visitHour: 11,
        visitEndHour: 13,
        awardedHours: 3,
        previousHours: 2,
        previous: { teacherId: 2, visitDay: 2, visitHour: 10, isForced: false, source: 'proposal' },
        exactBranchMatch: false,
      }),
    ],
    released: [
      {
        companyId: 4,
        companyName: 'Firma D',
        previous: { teacherId: 2, visitDay: 4, visitHour: 9, isForced: false, source: 'proposal' },
      },
    ],
    hourChanges: [
      {
        companyId: 2,
        companyName: 'Firma B',
        oldHours: 2,
        newHours: 3,
        reasonCode: { kind: 'granted' },
        reason: 'Havuzda saat kaldığı için artırıldı.',
      },
      {
        companyId: 5,
        companyName: 'Firma E',
        oldHours: 4,
        newHours: 0,
        reasonCode: { kind: 'poolExhausted' },
        reason: 'Havuz tükendiği için fahriye düşürüldü.',
      },
    ],
    groupSplits: [
      {
        teacherId: 1,
        teacherName: 'Ahmet Yılmaz',
        groups: [
          { groupKey: 'district:Akdeniz', groupLabel: 'Akdeniz', companyNames: ['Firma A'] },
          { groupKey: 'district:Toroslar', groupLabel: 'Toroslar', companyNames: ['Firma B'] },
        ],
      },
    ],
    unassigned: [
      {
        companyId: 6,
        companyName: 'Firma F',
        reasonCode: 'noEligibleCell',
        reason: 'Salı günü için uygun boş saat yok.',
        wasAssigned: true,
      },
    ],
    teacherLoads: [
      { teacherId: 1, teacherName: 'Ahmet Yılmaz', hours: 18, capacity: 20, companyCount: 5, distinctGroups: 2 },
      { teacherId: 2, teacherName: 'Ayşe Demir', hours: 12, capacity: 20, companyCount: 3, distinctGroups: 1 },
    ],
    placedCount: 6,
    totalHours: 30,
    poolHours: 100,
    poolRemaining: -2,
    warnings: ['Havuz zaten aşılmıştı.'],
  })
}
