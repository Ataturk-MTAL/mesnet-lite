import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import type { VueWrapper } from '@vue/test-utils'
import { ref } from 'vue'
import { createMemoryHistory, createRouter } from 'vue-router'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import ConfirmationService from 'openvue/confirmationservice'
import Tooltip from 'openvue/tooltip'
import Aura from '@openvue/themes/aura'
import TeachingLoadView from './TeachingLoadView.vue'
import { labels } from '../i18n/labels'
import { teachingLoadApi } from '../api/teachingLoad'
import type { TeachingLoadBoard } from '../api/teachingLoad'

// Aktif dönem `watch()` ile izlendiği için gerçek bir `ref` olmalı.
vi.mock('../composables/useTerm', () => ({
  activeTerm: ref('2026-2027/1'),
}))

const getBoardMock = vi.fn<() => Promise<TeachingLoadBoard>>()
vi.mock('../api/teachingLoad', () => ({
  teachingLoadApi: {
    get: () => getBoardMock(),
    save: vi.fn(),
  },
}))

function boardFixture(overrides: Partial<TeachingLoadBoard> = {}): TeachingLoadBoard {
  return {
    term: '2026-2027/1',
    rows: [
      { id: 1, grade: '10', branch: 'Elektrik', weeklyHours: 6, groupCount: 2, autoGroupCount: 2, isGroupManual: false, isSuggested: false },
      { id: 2, grade: '11', branch: 'Elektronik', weeklyHours: 4, groupCount: 1, autoGroupCount: 1, isGroupManual: false, isSuggested: false },
    ],
    branchHours: 16,
    chiefPlanningHours: 22,
    poolHours: 38,
    ...overrides,
  }
}

async function mountView(): Promise<VueWrapper> {
  // `RouterLink` ve `onBeforeRouteLeave` bir yönlendirici gerektirir.
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/company-hours', component: { template: '<div />' } },
    ],
  })
  await router.push('/')
  await router.isReady()

  const wrapper = mount(TeachingLoadView, {
    global: {
      plugins: [
        [OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }],
        ToastService,
        ConfirmationService,
        router,
      ],
      directives: { tooltip: Tooltip },
    },
    attachTo: document.body,
  })
  await flushPromises()
  return wrapper
}

function textOf(wrapper: VueWrapper, testId: string): string {
  return wrapper.get(`[data-test="${testId}"]`).text()
}

beforeEach(() => {
  getBoardMock.mockReset()
  document.body.replaceChildren()
})

describe('TeachingLoadView havuz kırılımı', () => {
  it('ders saatleri, şeflik saatleri ve toplam havuzu ayrı ayrı gösterir', async () => {
    // Arrange
    getBoardMock.mockResolvedValue(boardFixture())

    // Act
    const wrapper = await mountView()

    // Assert
    expect(textOf(wrapper, 'branch-hours')).toBe('16')
    expect(textOf(wrapper, 'chief-hours')).toBe('22')
    expect(textOf(wrapper, 'pool-hours')).toBe('38')
    expect(wrapper.text()).toContain(labels.teachingLoad.branchHours)
    expect(wrapper.text()).toContain(labels.teachingLoad.chiefHours)
    expect(wrapper.text()).toContain(labels.teachingLoad.pool)
    wrapper.unmount()
  })

  it('satır düzenlenince ders saatleri ve toplam canlı güncellenir, şeflik saatleri sabit kalır', async () => {
    // Arrange
    getBoardMock.mockResolvedValue(boardFixture())
    const wrapper = await mountView()

    // Act — yeni satır ekle, 3 saat × 2 grup gir (+6).
    const addButton = wrapper.findAll('button').find((b) => b.text() === labels.teachingLoad.addRow)
    expect(addButton).toBeDefined()
    await addButton!.trigger('click')
    const inputs = wrapper.findAllComponents({ name: 'InputNumber' })
    const weeklyInput = inputs[inputs.length - 2]
    const groupInput = inputs[inputs.length - 1]
    weeklyInput.vm.$emit('update:modelValue', 3)
    groupInput.vm.$emit('update:modelValue', 2)
    await flushPromises()

    // Assert
    expect(textOf(wrapper, 'branch-hours')).toBe('22')
    expect(textOf(wrapper, 'chief-hours')).toBe('22')
    expect(textOf(wrapper, 'pool-hours')).toBe('44')
    wrapper.unmount()
  })

  it('şeflik saati sıfırsa toplam yalnızca ders saatlerine eşittir', async () => {
    // Arrange
    getBoardMock.mockResolvedValue(boardFixture({ chiefPlanningHours: 0, poolHours: 16 }))

    // Act
    const wrapper = await mountView()

    // Assert
    expect(textOf(wrapper, 'chief-hours')).toBe('0')
    expect(textOf(wrapper, 'pool-hours')).toBe('16')
    wrapper.unmount()
  })

  it('kayıtlı satır silinince toplam ders saati düşer, şeflik saati toplama eklenmeye devam eder', async () => {
    // Arrange
    getBoardMock.mockResolvedValue(boardFixture())
    const wrapper = await mountView()

    // Act — ilk satırı (6 × 2 = 12 saat) sil.
    const removeButton = wrapper.findAll(`button[aria-label="${labels.teachingLoad.removeRow}"]`)[0]
    await removeButton.trigger('click')

    // Assert — kalan: 4 × 1 = 4; toplam = 4 + 22.
    expect(textOf(wrapper, 'branch-hours')).toBe('4')
    expect(textOf(wrapper, 'chief-hours')).toBe('22')
    expect(textOf(wrapper, 'pool-hours')).toBe('26')
    wrapper.unmount()
  })

  it('bilgi notu şeflik saatlerini de havuza dahil eder', async () => {
    // Arrange
    getBoardMock.mockResolvedValue(boardFixture())

    // Act
    const wrapper = await mountView()

    // Assert
    expect(wrapper.text()).toContain(labels.teachingLoad.subtitle)
    expect(labels.teachingLoad.subtitle).toContain('md. 6/4')
    wrapper.unmount()
  })
})

describe('TeachingLoadView grup sayısı: otomatik / elle', () => {
  /** n. satırın grup sayısı girişi (her satırda önce saat, sonra grup). */
  function groupInput(wrapper: VueWrapper, rowIndex: number): VueWrapper {
    return wrapper.findAllComponents({ name: 'InputNumber' })[rowIndex * 2 + 1] as VueWrapper
  }

  function groupValue(wrapper: VueWrapper, rowIndex: number): number | null | undefined {
    return (groupInput(wrapper, rowIndex).props() as { modelValue?: number | null }).modelValue
  }

  function modeTexts(wrapper: VueWrapper): string[] {
    return wrapper.findAll('[data-test="group-mode"]').map((tag) => tag.text())
  }

  it('sunucudan gelen satırları Otomatik olarak işaretler, otomatiğe dön düğmesi göstermez', async () => {
    // Arrange
    getBoardMock.mockResolvedValue(boardFixture())

    // Act
    const wrapper = await mountView()

    // Assert
    expect(modeTexts(wrapper)).toEqual([labels.teachingLoad.groupAuto, labels.teachingLoad.groupAuto])
    expect(wrapper.find('[data-test="group-reset"]').exists()).toBe(false)
    expect(wrapper.text()).toContain(labels.teachingLoad.groupCountNote)
    wrapper.unmount()
  })

  it('sunucudan elle gelen satırı Elle olarak işaretler ve otomatiğe dön düğmesi gösterir', async () => {
    // Arrange
    const board = boardFixture()
    getBoardMock.mockResolvedValue({
      ...board,
      rows: [{ ...board.rows[0], groupCount: 3, autoGroupCount: 2, isGroupManual: true }, board.rows[1]],
    })

    // Act
    const wrapper = await mountView()

    // Assert
    expect(modeTexts(wrapper)).toEqual([labels.teachingLoad.groupManual, labels.teachingLoad.groupAuto])
    expect(wrapper.findAll('[data-test="group-reset"]')).toHaveLength(1)
    wrapper.unmount()
  })

  it('sayıyı değiştirince satır elle olur, toplama anında yansır ve düğme çıkar', async () => {
    // Arrange
    getBoardMock.mockResolvedValue(boardFixture())
    const wrapper = await mountView()

    // Act — ilk satır: 6 saat × 2 grup → 6 saat × 5 grup (+18).
    groupInput(wrapper, 0).vm.$emit('update:modelValue', 5)
    await flushPromises()

    // Assert
    expect(modeTexts(wrapper)[0]).toBe(labels.teachingLoad.groupManual)
    expect(wrapper.findAll('[data-test="group-reset"]')).toHaveLength(1)
    expect(textOf(wrapper, 'branch-hours')).toBe('34')
    expect(textOf(wrapper, 'pool-hours')).toBe('56')
    wrapper.unmount()
  })

  it('aynı değer yeniden gelirse satırı elle işaretlemez', async () => {
    // Arrange
    getBoardMock.mockResolvedValue(boardFixture())
    const wrapper = await mountView()

    // Act
    groupInput(wrapper, 0).vm.$emit('update:modelValue', 2)
    await flushPromises()

    // Assert
    expect(modeTexts(wrapper)[0]).toBe(labels.teachingLoad.groupAuto)
    wrapper.unmount()
  })

  it('otomatiğe dön satırı otomatik yapar, sayıyı otomatik değere çeker ve toplamı geri alır', async () => {
    // Arrange
    getBoardMock.mockResolvedValue(boardFixture())
    const wrapper = await mountView()
    groupInput(wrapper, 0).vm.$emit('update:modelValue', 5)
    await flushPromises()

    // Act
    await wrapper.get('[data-test="group-reset"]').trigger('click')
    await flushPromises()

    // Assert
    expect(modeTexts(wrapper)[0]).toBe(labels.teachingLoad.groupAuto)
    expect(wrapper.find('[data-test="group-reset"]').exists()).toBe(false)
    expect(groupValue(wrapper, 0)).toBe(2)
    expect(textOf(wrapper, 'branch-hours')).toBe('16')
    wrapper.unmount()
  })

  it('kaydederken her satır için isGroupManual gönderir', async () => {
    // Arrange
    const board = boardFixture()
    getBoardMock.mockResolvedValue(board)
    const saveMock = vi.mocked(teachingLoadApi.save)
    saveMock.mockReset()
    saveMock.mockResolvedValue(board)
    const wrapper = await mountView()
    groupInput(wrapper, 0).vm.$emit('update:modelValue', 5)
    await flushPromises()

    // Act
    const saveButton = wrapper.findAll('button').find((b) => b.text() === labels.teachingLoad.save)
    await saveButton!.trigger('click')
    await flushPromises()

    // Assert
    expect(saveMock).toHaveBeenCalledWith([
      { grade: '10', branch: 'Elektrik', weeklyHours: 6, groupCount: 5, isGroupManual: true },
      { grade: '11', branch: 'Elektronik', weeklyHours: 4, groupCount: 1, isGroupManual: false },
    ])
    wrapper.unmount()
  })

  it('yeni eklenen satır otomatik başlar ve geçici sayı ipucunu gösterir', async () => {
    // Arrange
    getBoardMock.mockResolvedValue(boardFixture())
    const wrapper = await mountView()
    expect(wrapper.find('[data-test="new-row-hint"]').exists()).toBe(false)

    // Act
    const addButton = wrapper.findAll('button').find((b) => b.text() === labels.teachingLoad.addRow)
    await addButton!.trigger('click')

    // Assert
    expect(modeTexts(wrapper)[2]).toBe(labels.teachingLoad.groupAuto)
    expect(groupValue(wrapper, 2)).toBe(1)
    expect(wrapper.get('[data-test="new-row-hint"]').text()).toBe(labels.teachingLoad.groupCountNewRowHint)
    wrapper.unmount()
  })
})
