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
vi.mock('../../api/history', () => ({
  previewChange: (request: ChangeRequest) => previewChangeMock(request),
  commitChange: (request: ChangeRequest, expectedHighWater: number | null) =>
    commitChangeMock(request, expectedHighWater),
  listHistory: (filter: HistoryFilter) => listHistoryMock(filter),
  getSubjectHistory: vi.fn(),
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

async function mountPanel(refreshToken = 0) {
  const wrapper = mount(TeacherScheduleHistory, {
    props: { teacherId: 7, term: '2026-2027/1', refreshToken },
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
    const remove = wrapper.get('[data-testid="schedule-history-delete-button"]').element as HTMLButtonElement
    expect(edit.disabled).toBe(true)
    expect(remove.disabled).toBe(true)
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

  it('previews a revoke command with the entered reason', async () => {
    respondWith([entry(2), entry(1)])
    previewChangeMock.mockResolvedValue({ status: 'stale', message: 'durdur' })
    const wrapper = await mountPanel()

    await wrapper.get('[data-testid="schedule-history-delete-button"]').trigger('click')
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
})
