import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import Tooltip from 'openvue/tooltip'
import Aura from '@openvue/themes/aura'
import VersionsPanel from './VersionsPanel.vue'
import { labels } from '../../i18n/labels'
import { useAsOfDateStore } from '../../stores/asOfDate'
import type { Version } from '../../types/models'

// Ağ sınırı burada `versionsApi`/`filesApi`; gerçek Tauri komut adları
// ImportExportView.spec.ts'te ayrıca doğrulanır.
const listMock = vi.fn<() => Promise<Version[]>>()
const createMock = vi.fn<(name: string, asOf: string | null) => Promise<Version>>()
const removeMock = vi.fn<(id: number) => Promise<void>>()
vi.mock('../../api/versions', () => ({
  versionsApi: {
    list: () => listMock(),
    create: (name: string, asOf: string | null = null) => createMock(name, asOf),
    remove: (id: number) => removeMock(id),
  },
}))

type ExportFn = (fileName: string, versionId: number | null, asOf: string | null) => Promise<string>
const exportAssignmentSheetMock = vi.fn<ExportFn>()
const exportVisitListsMock = vi.fn<ExportFn>()
const exportCommissionMinutesPdfMock = vi.fn<ExportFn>()
const exportCommissionMinutesXlsxMock = vi.fn<ExportFn>()
const exportWorkbookMock = vi.fn<ExportFn>()
vi.mock('../../api/files', () => ({
  filesApi: {
    exportAssignmentSheet: (fileName: string, versionId: number | null = null, asOf: string | null = null) =>
      exportAssignmentSheetMock(fileName, versionId, asOf),
    exportVisitLists: (fileName: string, versionId: number | null = null, asOf: string | null = null) =>
      exportVisitListsMock(fileName, versionId, asOf),
    exportCommissionMinutesPdf: (fileName: string, versionId: number | null = null, asOf: string | null = null) =>
      exportCommissionMinutesPdfMock(fileName, versionId, asOf),
    exportCommissionMinutesXlsx: (fileName: string, versionId: number | null = null, asOf: string | null = null) =>
      exportCommissionMinutesXlsxMock(fileName, versionId, asOf),
    exportWorkbook: (fileName: string, versionId: number | null = null, asOf: string | null = null) =>
      exportWorkbookMock(fileName, versionId, asOf),
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
    asOf: null,
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
    const [fileName, versionId, asOf] = exportAssignmentSheetMock.mock.calls[0]
    expect(versionId).toBe(7)
    expect(fileName).toContain('surum-2026-09-26')
    // Sürümden çıktı alınırken `asOf` GÖNDERİLMEZ; arka uç sürümün kendi
    // kayıtlı tarihini kullanır.
    expect(asOf).toBeNull()
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

    expect(createMock).toHaveBeenCalledWith('Yeni sürüm', null)
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

describe('VersionsPanel — tarihteki durum (asOf)', () => {
  async function typeVersionName(name: string): Promise<void> {
    const input = document.body.querySelector<HTMLInputElement>('#version-save-name')!
    input.value = name
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await flushPromises()
  }

  it('varsayılan görünümde Sürüm Kaydet `asOf` olarak null gönderir', async () => {
    listMock.mockResolvedValue([])
    createMock.mockResolvedValue(versionFixture({ id: 9, name: 'Yeni sürüm' }))
    const wrapper = mountPanel()
    await flushPromises()

    clickTestId('version-save-button')
    await flushPromises()
    await typeVersionName('Yeni sürüm')
    clickTestId('version-save-save-button')
    await flushPromises()

    expect(createMock).toHaveBeenCalledWith('Yeni sürüm', null)
    expect(document.body.querySelector('[data-testid="version-save-as-of-note"]')).toBeNull()

    wrapper.unmount()
  })

  it('salt okunur durumda Sürüm Kaydet seçili tarihi gönderir ve pencerede kısaca söyler', async () => {
    useAsOfDateStore().setAsOfDate('2026-09-10')
    listMock.mockResolvedValue([])
    createMock.mockResolvedValue(versionFixture({ id: 9, name: 'Yeni sürüm', asOf: '2026-09-10' }))
    const wrapper = mountPanel()
    await flushPromises()

    clickTestId('version-save-button')
    await flushPromises()
    expect(document.body.querySelector('[data-testid="version-save-as-of-note"]')?.textContent).toContain(
      '10.09.2026',
    )

    await typeVersionName('Yeni sürüm')
    clickTestId('version-save-save-button')
    await flushPromises()

    expect(createMock).toHaveBeenCalledWith('Yeni sürüm', '2026-09-10')

    wrapper.unmount()
  })

  it('`asOf` dolu bir sürümün adının yanında tarih etiketi gösterir', async () => {
    listMock.mockResolvedValue([
      versionFixture({ id: 1, name: 'Elle kayıt', asOf: '2026-09-10' }),
      versionFixture({ id: 2, name: 'Otomatik kayıt (10.09.2026 itibarıyla)', asOf: '2026-09-10' }),
      versionFixture({ id: 3, name: 'Güncel kayıt', asOf: null }),
    ])
    const wrapper = mountPanel()
    await flushPromises()

    const tags = wrapper.findAll('.name-cell .p-tag').map((tag) => tag.text())
    // Elle kaydedilen (adı "itibarıyla" içermeyen) satırda etiket görünür.
    expect(tags).toContain(labels.versions.asOfTag('10.09.2026'))
    // Otomatik kaydın adı zaten eki içeriyor; etiket TEKRAR gösterilmez.
    expect(tags.filter((text) => text === labels.versions.asOfTag('10.09.2026'))).toHaveLength(1)

    wrapper.unmount()
  })
})
