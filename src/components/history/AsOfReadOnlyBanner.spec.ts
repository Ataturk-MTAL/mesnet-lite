import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import AsOfReadOnlyBanner from './AsOfReadOnlyBanner.vue'
import { labels } from '../../i18n/labels'
import { useAsOfDateStore } from '../../stores/asOfDate'
import { useTermStore } from '../../stores/term'
import type { TermWithDates } from '../../types/models'

const term: TermWithDates = {
  term: '2026-2027/1',
  startDate: '2026-09-01',
  endDate: '2027-06-30',
  datesConfirmed: true,
  isPlanning: false,
  defaultAsOf: '2026-11-10',
  earliestAllowedDate: '2026-09-01',
}

function mountBanner() {
  return mount(AsOfReadOnlyBanner, {
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }]],
    },
  })
}

describe('AsOfReadOnlyBanner', () => {
  it('varsayılan tarihte hiçbir şey göstermez', () => {
    const termStore = useTermStore()
    termStore.activeTermDates = term
    useAsOfDateStore()

    const wrapper = mountBanner()

    expect(wrapper.find('[data-testid="as-of-readonly-banner"]').exists()).toBe(false)
  })

  it('salt okunurken seçili tarihi Türkçe (gg.aa.yyyy) biçimde içeren başlığı ve notu gösterir', () => {
    const termStore = useTermStore()
    termStore.activeTermDates = term
    const asOfDateStore = useAsOfDateStore()
    asOfDateStore.setAsOfDate('2026-10-01')

    const wrapper = mountBanner()

    // Seçiciyle aynı biçim: ISO değil, 'gg.aa.yyyy' (bkz. AsOfDatePicker `dateFormat="dd.mm.yy"`).
    expect(wrapper.text()).toContain(labels.asOfDate.readOnlyBanner('01.10.2026'))
    expect(wrapper.text()).not.toContain('2026-10-01')
    expect(wrapper.text()).toContain(labels.asOfDate.readOnlyBannerNote)
  })

  it('varsayılana dön düğmesi seçiciyi varsayılana döndürür', async () => {
    const termStore = useTermStore()
    termStore.activeTermDates = term
    const asOfDateStore = useAsOfDateStore()
    asOfDateStore.setAsOfDate('2026-10-01')

    const wrapper = mountBanner()
    await wrapper.find('[data-testid="as-of-readonly-banner-today-button"]').trigger('click')

    expect(asOfDateStore.asOfDate).toBe('2026-11-10')
    expect(wrapper.find('[data-testid="as-of-readonly-banner"]').exists()).toBe(false)
  })
})
