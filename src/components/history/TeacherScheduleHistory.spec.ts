import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import Aura from '@openvue/themes/aura'
import TeacherScheduleHistory from './TeacherScheduleHistory.vue'
import { labels } from '../../i18n/labels'
import type { ChangeOutcome, ChangeRequest, HistoryChangeSetEntry, HistoryFilter, HistoryResponse } from '../../types/models'

// Tauri komutları jsdom'da çalışmaz; `api/history.ts` sarmalayıcısı taklit edilir.
const listHistoryMock = vi.fn<(filter: HistoryFilter) => Promise<HistoryResponse>>()
const previewChangeMock = vi.fn<(request: ChangeRequest) => Promise<ChangeOutcome>>()
const commitChangeMock = vi.fn<(request: ChangeRequest, expectedHighWater: number | null) => Promise<ChangeOutcome>>()
const deleteChangeSetMock = vi.fn<(changeSetId: number) => Promise<void>>()
vi.mock('../../api/history', () => ({
  previewChange: (request: ChangeRequest) => previewChangeMock(request),
  commitChange: (request: ChangeRequest, expectedHighWater: number | null) =>
    commitChangeMock(request, expectedHighWater),
  listHistory: (filter: HistoryFilter) => listHistoryMock(filter),
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

function entry(id: number, overrides: Partial<HistoryChangeSetEntry> = {}): HistoryChangeSetEntry {
  return {
    changeSetId: id,
    recordedAt: `2026-09-1${id}T10:00:00.000Z`,
    kind: 'set_teacher_schedule',
    reason: `gerekçe ${id}`,
    actor: 'okul.mudur.yrd',
    effectiveDate: `2026-10-0${id}`,
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

function respondWith(entries: HistoryChangeSetEntry[], next: number | null = null): void {
  listHistoryMock.mockResolvedValue({ entries, nextBeforeChangeSetId: next })
}

async function mountPanel(refreshToken = 0, readOnly = false) {
  const wrapper = mount(TeacherScheduleHistory, {
    props: { teacherId: 7, term: '2026-2027/1', refreshToken, readOnly },
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }], ToastService],
    },
    attachTo: document.body,
  })
  await flushPromises()
  return wrapper
}

function itemIds(wrapper: Awaited<ReturnType<typeof mountPanel>>): string[] {
  return wrapper.findAll('[data-testid^="schedule-history-item-"]').map((item) => item.attributes('data-testid') ?? '')
}

beforeEach(() => {
  listHistoryMock.mockReset()
  previewChangeMock.mockReset()
  commitChangeMock.mockReset()
  deleteChangeSetMock.mockReset()
  toastAddMock.mockReset()
  confirmRequireMock.mockReset()
  document.body.innerHTML = ''
})

describe('TeacherScheduleHistory', () => {
  it('asks the teacher_schedule stream for the selected teacher without opening rows', async () => {
    respondWith([])
    const wrapper = await mountPanel()

    expect(listHistoryMock).toHaveBeenCalledTimes(1)
    const filter = listHistoryMock.mock.calls[0][0]
    expect(filter.term).toBe('2026-2027/1')
    expect(filter.stream).toBe('teacher_schedule')
    expect(filter.subjectId).toBe(7)
    expect(filter.includeOpening).toBe(false)
    expect(wrapper.find('[data-testid="schedule-history-empty"]').exists()).toBe(true)
    wrapper.unmount()
  })

  it('lists the newest change set first', async () => {
    respondWith([entry(1), entry(3), entry(2)])
    const wrapper = await mountPanel()

    expect(itemIds(wrapper)).toEqual([
      'schedule-history-item-3',
      'schedule-history-item-2',
      'schedule-history-item-1',
    ])
    wrapper.unmount()
  })

  it('marks only the latest valid change as active and strikes revoked ones', async () => {
    // 4 = geri alma kaydı, 3 ona geri alınmış; en son geçerli olan 2'dir.
    respondWith([
      entry(4, { kind: 'revoke', revokesChangeSetId: 3, isRevocable: false }),
      entry(3, { revokedByChangeSetId: 4, isRevocable: false }),
      entry(2),
      entry(1),
    ])
    const wrapper = await mountPanel()

    const badges = wrapper.findAll('[data-testid="schedule-history-active-badge"]')
    expect(badges).toHaveLength(1)
    expect(wrapper.get('[data-testid="schedule-history-item-2"]').text()).toContain(labels.availability.historyActiveBadge)

    const revoked = wrapper.get('[data-testid="schedule-history-item-3"]')
    expect(revoked.classes()).toContain('schedule-history-item--muted')
    expect(revoked.text()).toContain(labels.changeHistory.revokedBadge)

    const record = wrapper.get('[data-testid="schedule-history-item-4"]')
    expect(record.classes()).toContain('schedule-history-item--muted')
    expect(record.text()).toContain(labels.availability.historyRevocationBadge)

    // Eylemler yalnız etkin satırda.
    expect(wrapper.findAll('[data-testid="schedule-history-edit-button"]')).toHaveLength(1)
    expect(wrapper.get('[data-testid="schedule-history-item-2"]').find('[data-testid="schedule-history-edit-button"]').exists()).toBe(true)
    wrapper.unmount()
  })

  it('disables both actions and explains why when the latest change is not revocable', async () => {
    respondWith([entry(2, { isRevocable: false }), entry(1)])
    const wrapper = await mountPanel()

    const edit = wrapper.get('[data-testid="schedule-history-edit-button"]').element as HTMLButtonElement
    const revoke = wrapper.get('[data-testid="schedule-history-revoke-button"]').element as HTMLButtonElement
    expect(edit.disabled).toBe(true)
    expect(revoke.disabled).toBe(true)
    expect(wrapper.get('[data-testid="schedule-history-not-revocable"]').text()).toBe(
      labels.availability.historyNotRevocableHint,
    )
    wrapper.unmount()
  })

  it('emits edit with the active entry', async () => {
    respondWith([entry(2), entry(1)])
    const wrapper = await mountPanel()

    await wrapper.get('[data-testid="schedule-history-edit-button"]').trigger('click')

    const emitted = wrapper.emitted('edit')
    expect(emitted).toHaveLength(1)
    expect((emitted?.[0][0] as HistoryChangeSetEntry).changeSetId).toBe(2)
    wrapper.unmount()
  })

  it('shows "Geri Al" as the active entry\'s revoke button label', async () => {
    respondWith([entry(2), entry(1)])
    const wrapper = await mountPanel()

    const revoke = wrapper.get('[data-testid="schedule-history-revoke-button"]')
    expect(revoke.attributes('aria-label')).toBe(labels.availability.historyRevoke)
    expect(revoke.text()).toContain(labels.availability.historyRevoke)
    wrapper.unmount()
  })

  it('previews a revoke command with the entered reason', async () => {
    respondWith([entry(2), entry(1)])
    previewChangeMock.mockResolvedValue({ status: 'stale', message: 'durdur' })
    const wrapper = await mountPanel()

    await wrapper.get('[data-testid="schedule-history-revoke-button"]').trigger('click')
    await flushPromises()

    const submit = document.body.querySelector<HTMLButtonElement>('[data-testid="schedule-delete-submit-button"]')
    expect(submit?.disabled).toBe(true) // Gerekçe girilmeden gönderilemez.

    const input = document.body.querySelector<HTMLTextAreaElement>('[data-testid="schedule-delete-reason-input"]')
    expect(input).not.toBeNull()
    input!.value = 'yanlışlıkla girildi'
    input!.dispatchEvent(new Event('input', { bubbles: true }))
    await flushPromises()

    document.body
      .querySelector<HTMLButtonElement>('[data-testid="schedule-delete-submit-button"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => expect(previewChangeMock).toHaveBeenCalled())

    expect(previewChangeMock.mock.calls[0][0]).toEqual({
      term: '2026-2027/1',
      effectiveDate: null,
      documentDate: null,
      reason: 'yanlışlıkla girildi',
      command: { type: 'revoke', changeSetId: 2 },
    })
    wrapper.unmount()
  })

  it('reloads the list when the refresh token changes', async () => {
    respondWith([entry(1)])
    const wrapper = await mountPanel(0)
    expect(listHistoryMock).toHaveBeenCalledTimes(1)

    await wrapper.setProps({ refreshToken: 1 })
    await flushPromises()

    expect(listHistoryMock).toHaveBeenCalledTimes(2)
    wrapper.unmount()
  })

  it('mutes a silently invalidated change set, tags it "Geçersiz" and skips it as active', async () => {
    // 2, `revokedByChangeSetId` boş kaldığı halde arka uçta olay düzeyinde
    // geri alındı (`isDeletable: true`); en son geçerli kayıt 1 olmalı.
    respondWith([entry(2, { isDeletable: true }), entry(1)])
    const wrapper = await mountPanel()

    const invalidated = wrapper.get('[data-testid="schedule-history-item-2"]')
    expect(invalidated.classes()).toContain('schedule-history-item--muted')
    expect(invalidated.find('[data-testid="schedule-history-invalid-badge"]').exists()).toBe(true)
    expect(invalidated.find('[data-testid="schedule-history-delete-button"]').exists()).toBe(true)

    expect(wrapper.get('[data-testid="schedule-history-item-1"]').text()).toContain(labels.availability.historyActiveBadge)
    expect(wrapper.find('[data-testid="schedule-history-item-2"] [data-testid="schedule-history-active-badge"]').exists()).toBe(
      false,
    )
    wrapper.unmount()
  })

  it('does not tag an already revoked change set as "Geçersiz" too', async () => {
    respondWith([
      entry(4, { kind: 'revoke', revokesChangeSetId: 3, isRevocable: false, isDeletable: true }),
      entry(3, { revokedByChangeSetId: 4, isRevocable: false, isDeletable: true }),
      entry(2),
    ])
    const wrapper = await mountPanel()

    expect(wrapper.find('[data-testid="schedule-history-item-3"] [data-testid="schedule-history-invalid-badge"]').exists()).toBe(
      false,
    )
    expect(wrapper.find('[data-testid="schedule-history-item-4"] [data-testid="schedule-history-invalid-badge"]').exists()).toBe(
      false,
    )
    wrapper.unmount()
  })

  it('deletes a deletable change set after confirmation and notifies the parent', async () => {
    deleteChangeSetMock.mockResolvedValueOnce(undefined)
    respondWith([entry(2, { isDeletable: true }), entry(1)])
    const wrapper = await mountPanel()

    await wrapper.get('[data-testid="schedule-history-item-2"] [data-testid="schedule-history-delete-button"]').trigger('click')
    expect(confirmRequireMock).toHaveBeenCalledTimes(1)
    expect(confirmRequireMock.mock.calls[0][0]).toMatchObject({ message: labels.history.delete.confirmMessage })

    respondWith([entry(1)])
    await confirmRequireMock.mock.calls[0][0].accept?.()
    await flushPromises()

    expect(deleteChangeSetMock).toHaveBeenCalledWith(2)
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'success', summary: labels.history.delete.success }),
    )
    expect(wrapper.emitted('changed')).toHaveLength(1)
    expect(listHistoryMock).toHaveBeenCalledTimes(2) // ilk yükleme + silme sonrası yeniden yükleme
    wrapper.unmount()
  })

  it('does not delete when the confirmation is not accepted', async () => {
    respondWith([entry(2, { isDeletable: true }), entry(1)])
    const wrapper = await mountPanel()

    await wrapper.get('[data-testid="schedule-history-item-2"] [data-testid="schedule-history-delete-button"]').trigger('click')
    expect(confirmRequireMock).toHaveBeenCalledTimes(1)

    expect(deleteChangeSetMock).not.toHaveBeenCalled()
    expect(wrapper.emitted('changed')).toBeUndefined()
    wrapper.unmount()
  })

  it('shows an error toast with the real backend message when deletion fails', async () => {
    deleteChangeSetMock.mockRejectedValueOnce(new Error('delete_change_set: Bu kayıt silinemez'))
    respondWith([entry(2, { isDeletable: true }), entry(1)])
    const wrapper = await mountPanel()

    await wrapper.get('[data-testid="schedule-history-item-2"] [data-testid="schedule-history-delete-button"]').trigger('click')
    await confirmRequireMock.mock.calls[0][0].accept?.()
    await flushPromises()

    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'error', detail: 'delete_change_set: Bu kayıt silinemez' }),
    )
    expect(wrapper.emitted('changed')).toBeUndefined()
    wrapper.unmount()
  })
})

describe('TeacherScheduleHistory readOnly', () => {
  it('readOnly true iken düzelt, geri al ve sil düğmeleri devre dışı kalır', async () => {
    respondWith([entry(2, { isDeletable: true }), entry(1)])
    const wrapper = await mountPanel(0, true)

    expect(wrapper.find('[data-testid="schedule-history-edit-button"]').attributes('disabled')).toBeDefined()
    expect(wrapper.find('[data-testid="schedule-history-revoke-button"]').attributes('disabled')).toBeDefined()
    expect(wrapper.find('[data-testid="schedule-history-delete-button"]').attributes('disabled')).toBeDefined()
    wrapper.unmount()
  })
})
