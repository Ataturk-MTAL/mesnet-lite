import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import DatePicker from 'openvue/datepicker'
import EffectiveDateField from './EffectiveDateField.vue'
import type { TermWithDates } from '../../types/models'

function mountField(term: TermWithDates, modelValue: string | null = null) {
  return mount(EffectiveDateField, {
    props: { modelValue, label: 'Sözleşme başlangıç tarihi', term },
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }]],
    },
  })
}

const startedTerm: TermWithDates = {
  term: '2026-2027/1',
  startDate: '2026-09-14',
  endDate: '2027-06-30',
  datesConfirmed: true,
  isPlanning: false,
  defaultAsOf: '2026-09-19',
  earliestAllowedDate: '2026-10-05',
}

const planningTerm: TermWithDates = {
  ...startedTerm,
  isPlanning: true,
}

describe('EffectiveDateField', () => {
  it('planlama döneminde hiç görünmez', () => {
    const wrapper = mountField(planningTerm)
    expect(wrapper.findComponent(DatePicker).exists()).toBe(false)
    expect(wrapper.find('label').exists()).toBe(false)
  })

  it('en erken tarihi earliestAllowedDate ile sınırlar', () => {
    const wrapper = mountField(startedTerm)
    const picker = wrapper.findComponent(DatePicker)
    expect(picker.exists()).toBe(true)

    const minDate = picker.props('minDate') as Date
    expect(minDate.getFullYear()).toBe(2026)
    expect(minDate.getMonth()).toBe(9) // 0 tabanlı: Ekim
    expect(minDate.getDate()).toBe(5)

    const maxDate = picker.props('maxDate') as Date
    expect(maxDate.getFullYear()).toBe(2027)
    expect(maxDate.getMonth()).toBe(5) // Haziran
    expect(maxDate.getDate()).toBe(30)
  })
})
