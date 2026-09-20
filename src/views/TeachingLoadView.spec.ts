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
      { id: 1, grade: '10', branch: 'Elektrik', weeklyHours: 6, groupCount: 2, isSuggested: false },
      { id: 2, grade: '11', branch: 'Elektronik', weeklyHours: 4, groupCount: 1, isSuggested: false },
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
