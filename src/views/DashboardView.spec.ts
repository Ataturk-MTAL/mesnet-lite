import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import type { VueWrapper } from '@vue/test-utils'
import { createMemoryHistory, createRouter } from 'vue-router'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import Aura from '@openvue/themes/aura'
import DashboardView from './DashboardView.vue'
import { labels } from '../i18n/labels'
import { useTermStore } from '../stores/term'
import { useAsOfDateStore } from '../stores/asOfDate'
import type { DashboardStats } from '../api/dashboard'

const getStatsMock = vi.fn<(asOf: string | null) => Promise<DashboardStats>>()
vi.mock('../api/dashboard', () => ({
  dashboardApi: {
    get: (asOf: string | null = null) => getStatsMock(asOf),
  },
}))

function statsFixture(overrides: Partial<DashboardStats> = {}): DashboardStats {
  return {
    term: '2026-2027/1',
    companyCount: 5,
    studentCount: 20,
    teacherCount: 4,
    activeTeacherCount: 3,
    totalCapacityHours: 100,
    assignedHours: 40,
    remainingHours: 60,
    teachersAtCapacity: 0,
    teachersOverCapacity: 0,
    poolHours: 160,
    awardedHours: 40,
    remainingPoolHours: 120,
    companiesWithoutLocation: 0,
    studentsWithoutCompany: 0,
    companiesWithoutStudents: 0,
    companiesWithoutAssignment: 0,
    ...overrides,
  }
}

async function mountView(): Promise<VueWrapper> {
  // `RouterLink` istatistik kartları ve "Başlarken" bağlantısı için gerekli.
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/companies', component: { template: '<div />' } },
      { path: '/students', component: { template: '<div />' } },
      { path: '/teachers', component: { template: '<div />' } },
      { path: '/import-export', component: { template: '<div />' } },
    ],
  })
  await router.push('/')
  await router.isReady()

  const wrapper = mount(DashboardView, {
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }], ToastService, router],
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
  getStatsMock.mockReset()
  document.body.replaceChildren()
  useTermStore().activeTerm = '2026-2027/1'
})

describe('DashboardView alan koordinatörlük ders yükü kartı', () => {
  it('havuz, takdir edilen ve kalan saatleri ayrı ayrı gösterir', async () => {
    // Arrange
    getStatsMock.mockResolvedValue(statsFixture({ poolHours: 160, awardedHours: 40, remainingPoolHours: 120 }))

    // Act
    const wrapper = await mountView()

    // Assert
    expect(wrapper.text()).toContain(labels.dashboard.poolCardTitle)
    expect(wrapper.text()).toContain(labels.dashboard.poolCardSubtitle)
    expect(textOf(wrapper, 'pool-hours')).toBe('160')
    expect(textOf(wrapper, 'awarded-hours')).toBe('40')
    expect(textOf(wrapper, 'remaining-pool-hours')).toBe('120')
    expect(wrapper.text()).not.toContain(labels.dashboard.poolExceededWarning)
    wrapper.unmount()
  })

  it('havuz aşılınca kalan saat negatif görünür ve uyarı gösterilir', async () => {
    // Arrange
    getStatsMock.mockResolvedValue(statsFixture({ poolHours: 160, awardedHours: 180, remainingPoolHours: -20 }))

    // Act
    const wrapper = await mountView()

    // Assert
    expect(textOf(wrapper, 'remaining-pool-hours')).toBe('-20')
    expect(wrapper.get('[data-test="remaining-pool-hours"]').classes()).toContain('balance-value--over')
    expect(wrapper.text()).toContain(labels.dashboard.poolExceededWarning)
    wrapper.unmount()
  })

  it('koordinatörlük saat dengesi kartında güncel başlığı kullanır', async () => {
    // Arrange
    getStatsMock.mockResolvedValue(statsFixture())

    // Act
    const wrapper = await mountView()

    // Assert
    expect(wrapper.text()).toContain(labels.dashboard.totalCapacity)
    expect(labels.dashboard.totalCapacity).toBe('Takdir Edilebilecek Koordinatörlük Saati')
    wrapper.unmount()
  })
})

describe('DashboardView tarihteki durum (asOf)', () => {
  it('varsayılan tarihte istatistikler `null` ile istenir ve başlık görünmez', async () => {
    getStatsMock.mockResolvedValue(statsFixture())
    const wrapper = await mountView()

    expect(getStatsMock).toHaveBeenCalledWith(null)
    expect(wrapper.find('[data-testid="as-of-readonly-banner"]').exists()).toBe(false)
    wrapper.unmount()
  })

  it('geçmiş bir tarih seçilince istatistikler o tarihle istenir ve başlık görünür', async () => {
    useAsOfDateStore().setAsOfDate('2026-09-10')
    getStatsMock.mockResolvedValue(statsFixture())
    const wrapper = await mountView()

    expect(getStatsMock).toHaveBeenCalledWith('2026-09-10')
    expect(wrapper.find('[data-testid="as-of-readonly-banner"]').exists()).toBe(true)
    wrapper.unmount()
  })
})
