import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import HistoryEntryCard from './HistoryEntryCard.vue'
import { labels } from '../../i18n/labels'
import type { HistoryChangeSetEntry } from '../../types/models'

// `useChange` bu testlerde hiç çağrılmıyor (Geri Al akışı ayrı bir görevde
// canlı denenir), ama `api/history.ts` yine de taklit edilir; gerçek hâli
// Tauri komutuna iner ve jsdom'da çöker.
const deleteChangeSetMock = vi.fn<(changeSetId: number) => Promise<void>>()
vi.mock('../../api/history', () => ({
  previewChange: vi.fn(),
  commitChange: vi.fn(),
  listHistory: vi.fn(),
  getSubjectHistory: vi.fn(),
  deleteChangeSet: (changeSetId: number) => deleteChangeSetMock(changeSetId),
}))

// `<Toast />` ve `<ConfirmDialog />` App.vue'da yaşar; gerçek diyaloğu çizmek
// yerine `require`e verilen `accept` geri çağrısını yakalayıp elle tetikleriz.
const toastAddMock = vi.fn<(message: { severity: string; detail?: string }) => void>()
vi.mock('openvue/usetoast', () => ({
  useToast: () => ({ add: toastAddMock }),
}))

const confirmRequireMock = vi.fn<(options: { accept?: () => void }) => void>()
vi.mock('openvue/useconfirm', () => ({
  useConfirm: () => ({ require: confirmRequireMock, close: vi.fn() }),
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
    isDeletable: false,
    warnings: [],
    events: [],
    ...overrides,
  }
}

beforeEach(() => {
  deleteChangeSetMock.mockReset()
  toastAddMock.mockReset()
  confirmRequireMock.mockReset()
})

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

  it('hides delete when not deletable', () => {
    const entry = baseEntry({ isDeletable: false })
    const wrapper = mountCard(entry)
    expect(wrapper.find('[data-testid="delete-button"]').exists()).toBe(false)
  })

  it('shows delete when deletable', () => {
    const entry = baseEntry({ isDeletable: true })
    const wrapper = mountCard(entry)
    expect(wrapper.find('[data-testid="delete-button"]').exists()).toBe(true)
  })

  it('deletes the change set and emits deleted when the confirmation is accepted', async () => {
    deleteChangeSetMock.mockResolvedValueOnce(undefined)
    const entry = baseEntry({ changeSetId: 7, isDeletable: true })
    const wrapper = mountCard(entry)

    await wrapper.get('[data-testid="delete-button"]').trigger('click')
    expect(confirmRequireMock).toHaveBeenCalledTimes(1)
    expect(confirmRequireMock.mock.calls[0][0]).toMatchObject({ message: labels.history.delete.confirmMessage })

    await confirmRequireMock.mock.calls[0][0].accept?.()
    await flushPromises()

    expect(deleteChangeSetMock).toHaveBeenCalledWith(7)
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'success', summary: labels.history.delete.success }),
    )
    expect(wrapper.emitted('deleted')).toHaveLength(1)
  })

  it('does not delete when the confirmation is not accepted', async () => {
    const entry = baseEntry({ isDeletable: true })
    const wrapper = mountCard(entry)

    await wrapper.get('[data-testid="delete-button"]').trigger('click')
    expect(confirmRequireMock).toHaveBeenCalledTimes(1)

    expect(deleteChangeSetMock).not.toHaveBeenCalled()
    expect(wrapper.emitted('deleted')).toBeUndefined()
  })

  it('shows an error toast with the real message when deletion fails', async () => {
    deleteChangeSetMock.mockRejectedValueOnce(new Error('delete_change_set: Bu kayıt silinemez'))
    const entry = baseEntry({ isDeletable: true })
    const wrapper = mountCard(entry)

    await wrapper.get('[data-testid="delete-button"]').trigger('click')
    await confirmRequireMock.mock.calls[0][0].accept?.()
    await flushPromises()

    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'error', detail: 'delete_change_set: Bu kayıt silinemez' }),
    )
    expect(wrapper.emitted('deleted')).toBeUndefined()
  })
})
