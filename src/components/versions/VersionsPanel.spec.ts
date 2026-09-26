import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import Tooltip from 'openvue/tooltip'
import Aura from '@openvue/themes/aura'
import VersionsPanel from './VersionsPanel.vue'
import { labels } from '../../i18n/labels'
import type { Version } from '../../types/models'

// Ağ sınırı burada `versionsApi`/`filesApi`; gerçek Tauri komut adları
// ImportExportView.spec.ts'te ayrıca doğrulanır.
const listMock = vi.fn<() => Promise<Version[]>>()
const createMock = vi.fn<(name: string) => Promise<Version>>()
const removeMock = vi.fn<(id: number) => Promise<void>>()
vi.mock('../../api/versions', () => ({
  versionsApi: {
    list: () => listMock(),
    create: (name: string) => createMock(name),
    remove: (id: number) => removeMock(id),
  },
}))

const exportAssignmentSheetMock = vi.fn<(fileName: string, versionId: number | null) => Promise<string>>()
const exportVisitListsMock = vi.fn<(fileName: string, versionId: number | null) => Promise<string>>()
const exportCommissionMinutesPdfMock = vi.fn<(fileName: string, versionId: number | null) => Promise<string>>()
const exportCommissionMinutesXlsxMock = vi.fn<(fileName: string, versionId: number | null) => Promise<string>>()
const exportWorkbookMock = vi.fn<(fileName: string, versionId: number | null) => Promise<string>>()
vi.mock('../../api/files', () => ({
  filesApi: {
    exportAssignmentSheet: (fileName: string, versionId: number | null) =>
      exportAssignmentSheetMock(fileName, versionId),
    exportVisitLists: (fileName: string, versionId: number | null) => exportVisitListsMock(fileName, versionId),
    exportCommissionMinutesPdf: (fileName: string, versionId: number | null) =>
      exportCommissionMinutesPdfMock(fileName, versionId),
    exportCommissionMinutesXlsx: (fileName: string, versionId: number | null) =>
      exportCommissionMinutesXlsxMock(fileName, versionId),
    exportWorkbook: (fileName: string, versionId: number | null) => exportWorkbookMock(fileName, versionId),
    geocodePending: vi.fn(),
  },
}))

const toastAddMock = vi.fn<(message: { severity: string; summary?: string; detail?: string }) => void>()
vi.mock('openvue/usetoast', () => ({
  useToast: () => ({ add: toastAddMock }),
}))

// `<ConfirmDialog />` App.vue'da yaşar; gerçek diyaloğu çizmek yerine
// `require`e verilen `accept` geri çağrısını yakalayıp elle tetikleriz.
const confirmRequireMock = vi.fn<(options: { accept?: () => void }) => void>()
vi.mock('openvue/useconfirm', () => ({
  useConfirm: () => ({ require: confirmRequireMock, close: vi.fn() }),
}))

function versionFixture(overrides: Partial<Version> = {}): Version {
  return {
    id: 1,
    name: 'Kasım puantajı öncesi',
    kind: 'manual',
    trigger: null,
    term: '2026-2027/1',
    createdAt: '2026-09-26T14:05:00',
    isAvailable: true,
    ...overrides,
  }
}

function mountPanel() {
  return mount(VersionsPanel, {
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }]],
      directives: { tooltip: Tooltip },
    },
    attachTo: document.body,
  })
}

function clickTestId(testId: string): void {
  document.body
    .querySelector<HTMLButtonElement>(`[data-testid="${testId}"]`)!
    .dispatchEvent(new MouseEvent('click', { bubbles: true }))
}

beforeEach(() => {
  listMock.mockReset()
  createMock.mockReset()
  removeMock.mockReset()
  exportAssignmentSheetMock.mockReset()
  exportVisitListsMock.mockReset()
  exportCommissionMinutesPdfMock.mockReset()
  exportCommissionMinutesXlsxMock.mockReset()
  exportWorkbookMock.mockReset()
  toastAddMock.mockReset()
  confirmRequireMock.mockReset()
  document.body.innerHTML = ''
})

describe('VersionsPanel — liste', () => {
  it('sürümleri adı, dönemi, Türkçe tarihi ve türüyle listeler', async () => {
    listMock.mockResolvedValue([
      versionFixture({ id: 1, name: 'Otomatik kayıt', kind: 'auto', trigger: 'assignmentSheet' }),
      versionFixture({ id: 2, name: 'Elle kayıt', kind: 'manual', createdAt: '2026-01-03T09:30:00' }),
    ])
    const wrapper = mountPanel()
    await flushPromises()

    expect(wrapper.text()).toContain('Otomatik kayıt')
    expect(wrapper.text()).toContain('Elle kayıt')
    expect(wrapper.text()).toContain('2026-2027/1')
    expect(wrapper.text()).toContain('26.09.2026 14:05')
    expect(wrapper.text()).toContain('03.01.2026 09:30')
    expect(wrapper.text()).toContain(labels.versions.kindAuto)
    expect(wrapper.text()).toContain(labels.versions.kindManual)

    wrapper.unmount()
  })

  it('hiç sürüm yokken boş durum metnini gösterir', async () => {
    listMock.mockResolvedValue([])
    const wrapper = mountPanel()
    await flushPromises()

    expect(wrapper.text()).toContain(labels.versions.empty)

    wrapper.unmount()
  })
})

describe('VersionsPanel — çıktı al', () => {
  it('menüden seçilen çıktıyı doğru versionId ile çağırır', async () => {
    const version = versionFixture({ id: 7 })
    listMock.mockResolvedValue([version])
    exportAssignmentSheetMock.mockResolvedValue('/Downloads/dosya.pdf')
    const wrapper = mountPanel()
    await flushPromises()

    clickTestId('version-export-button')
    await flushPromises()

    const item = document.body.querySelector<HTMLAnchorElement>(
      `li[aria-label="${labels.reports.assignmentSheet}"] a`,
    )!
    item.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    expect(exportAssignmentSheetMock).toHaveBeenCalledTimes(1)
    const [fileName, versionId] = exportAssignmentSheetMock.mock.calls[0]
    expect(versionId).toBe(7)
    expect(fileName).toContain('surum-2026-09-26')
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'success', summary: labels.export.saved }),
    )

    wrapper.unmount()
  })
})

describe('VersionsPanel — kayıp dosya', () => {
  it('isAvailable=false satırında "Çıktı Al" devre dışıdır', async () => {
    listMock.mockResolvedValue([versionFixture({ isAvailable: false })])
    const wrapper = mountPanel()
    await flushPromises()

    const button = document.body.querySelector<HTMLButtonElement>('[data-testid="version-export-button"]')!
    expect(button.disabled).toBe(true)

    wrapper.unmount()
  })
})

describe('VersionsPanel — elle kayıt', () => {
  async function openSaveDialogAndType(name: string): Promise<void> {
    clickTestId('version-save-button')
    await flushPromises()
    const input = document.body.querySelector<HTMLInputElement>('#version-save-name')!
    input.value = name
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await flushPromises()
  }

  it('girilen adla create çağırır ve listeyi yeniler', async () => {
    listMock.mockResolvedValue([])
    createMock.mockResolvedValue(versionFixture({ id: 9, name: 'Yeni sürüm' }))
    const wrapper = mountPanel()
    await flushPromises()

    await openSaveDialogAndType('Yeni sürüm')
    clickTestId('version-save-save-button')
    await flushPromises()

    expect(createMock).toHaveBeenCalledWith('Yeni sürüm')
    expect(listMock).toHaveBeenCalledTimes(2) // ilk yükleme + kayıt sonrası yenileme
    expect(document.body.querySelector('#version-save-name')).toBeNull() // diyalog kapandı

    wrapper.unmount()
  })

  it('arka uç reddederse pencere açık kalır ve hata gösterilir', async () => {
    listMock.mockResolvedValue([])
    createMock.mockRejectedValue(new Error('create_version: Ad 80 karakteri aşamaz'))
    const wrapper = mountPanel()
    await flushPromises()

    await openSaveDialogAndType('Çok uzun bir isim')
    clickTestId('version-save-save-button')
    await flushPromises()

    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'error', detail: 'create_version: Ad 80 karakteri aşamaz' }),
    )
    expect(document.body.querySelector('#version-save-name')).not.toBeNull() // diyalog hâlâ açık
    expect(listMock).toHaveBeenCalledTimes(1) // yenilenmedi

    wrapper.unmount()
  })
})

describe('VersionsPanel — silme', () => {
  it('onaydan sonra remove çağırır ve listeyi yeniler', async () => {
    const version = versionFixture({ id: 3 })
    listMock.mockResolvedValue([version])
    removeMock.mockResolvedValue(undefined)
    const wrapper = mountPanel()
    await flushPromises()

    clickTestId('version-delete-button')
    await flushPromises()

    const options = confirmRequireMock.mock.calls[confirmRequireMock.mock.calls.length - 1]?.[0]
    options?.accept?.()
    await flushPromises()

    expect(removeMock).toHaveBeenCalledWith(3)
    expect(listMock).toHaveBeenCalledTimes(2) // ilk yükleme + silme sonrası yenileme

    wrapper.unmount()
  })
})
