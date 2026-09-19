import { describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import HistoryEntryCard from './HistoryEntryCard.vue'
import { labels } from '../../i18n/labels'
import type { HistoryChangeSetEntry } from '../../types/models'

// `useChange` bu testlerde hiç çağrılmıyor (Geri Al akışı ayrı bir görevde
// canlı denenir), ama `api/history.ts` yine de taklit edilir; gerçek hâli
// Tauri komutuna iner ve jsdom'da çöker.
vi.mock('../../api/history', () => ({
  previewChange: vi.fn(),
  commitChange: vi.fn(),
  listHistory: vi.fn(),
  getSubjectHistory: vi.fn(),
}))

function mountCard(entry: HistoryChangeSetEntry) {
  return mount(HistoryEntryCard, {
    props: { entry, term: '2026-2027/1' },
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }]],
    },
  })
}

function baseEntry(overrides: Partial<HistoryChangeSetEntry> = {}): HistoryChangeSetEntry {
  return {
    changeSetId: 1,
    recordedAt: '2026-09-19T10:00:00.000Z',
    kind: 'transferStudent',
    reason: 'Nakil talebi',
    actor: 'okul.mudur.yrd',
    effectiveDate: '2026-09-20',
    documentDate: null,
    revokedByChangeSetId: null,
    revokesChangeSetId: null,
    isRevocable: true,
    warnings: [],
    events: [],
    ...overrides,
  }
}

describe('HistoryEntryCard', () => {
  it('nests caused events under their cause', () => {
    const entry = baseEntry({
      events: [
        {
          eventId: 10,
          stream: 'placement',
          subjectId: 1,
          subjectLabel: 'Ahmet Yılmaz',
          kind: 'student_transferred',
          effectiveDate: '2026-09-20',
          before: 'A İşletmesi',
          after: 'B İşletmesi',
          causedByEventId: null,
          isRevoked: false,
        },
        {
          eventId: 11,
          stream: 'company_hours',
          subjectId: 2,
          subjectLabel: 'B İşletmesi',
          kind: 'hours_capped',
          effectiveDate: '2026-09-20',
          before: '10',
          after: '8',
          causedByEventId: 10,
          isRevoked: false,
        },
      ],
    })

    const wrapper = mountCard(entry)
    const causeRow = wrapper.get('[data-testid="history-event-10"]')
    const childRow = wrapper.get('[data-testid="history-event-11"]')

    expect(causeRow.attributes('data-depth')).toBe('0')
    expect(childRow.attributes('data-depth')).toBe('1')

    // Neden satırı, doğurduğu otomatik olaydan önce gelir.
    const rows = wrapper.findAll('.history-event-row')
    const causeIndex = rows.findIndex((row) => row.attributes('data-testid') === 'history-event-10')
    const childIndex = rows.findIndex((row) => row.attributes('data-testid') === 'history-event-11')
    expect(causeIndex).toBeLessThan(childIndex)
    expect(childRow.text()).toContain(labels.changeHistory.causedBy)
  })

  it('strikes a revoked change set', () => {
    const entry = baseEntry({ revokedByChangeSetId: 42, isRevocable: false })
    const wrapper = mountCard(entry)

    const card = wrapper.get('[data-testid="history-entry-card"]')
    expect(card.classes()).toContain('history-entry--revoked')
    expect(wrapper.text()).toContain(labels.changeHistory.revokedBadge)
  })

  it('hides revoke when not revocable', () => {
    const entry = baseEntry({ isRevocable: false })
    const wrapper = mountCard(entry)
    expect(wrapper.find('[data-testid="revoke-button"]').exists()).toBe(false)
  })

  it('shows revoke when revocable', () => {
    const entry = baseEntry({ isRevocable: true })
    const wrapper = mountCard(entry)
    expect(wrapper.find('[data-testid="revoke-button"]').exists()).toBe(true)
  })
})
