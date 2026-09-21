import { beforeEach, describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import TeacherFormDialog from './TeacherFormDialog.vue'
import EffectiveDateField from '../history/EffectiveDateField.vue'
import type { NewTeacher, TeacherWithCapacity, TermWithDates } from '../../types/models'

const teacherFixture: TeacherWithCapacity = {
  id: 5,
  firstName: 'Ahmet',
  lastName: 'Yılmaz',
  registryNo: '12345',
  field: 'Elektrik-Elektronik Teknolojisi',
  branches: '[]',
  employmentType: 'tenured',
  baseHours: 20,
  maxExtraHours: 24,
  otherExtraHours: 0,
  chiefType: 'none',
  isActive: 1,
  chiefHours: 0,
  statutoryCap: 40,
  capacity: 24,
}

const planningTerm: TermWithDates = {
  term: '2026-2027/1',
  startDate: '2026-11-15',
  endDate: '2027-06-30',
  datesConfirmed: false,
  isPlanning: true,
  defaultAsOf: '2026-11-15',
  earliestAllowedDate: '2026-11-15',
}

const startedTerm: TermWithDates = {
  ...planningTerm,
  startDate: '2026-09-14',
  isPlanning: false,
  earliestAllowedDate: '2026-10-01',
}

function mountDialog(props: { teacher: TeacherWithCapacity | null; term: TermWithDates | null }) {
  return mount(TeacherFormDialog, {
    props: { visible: true, teacher: props.teacher, knownBranches: [], term: props.term },
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }]],
    },
    attachTo: document.body,
  })
}

function saveButton(): HTMLButtonElement {
  const button = document.body.querySelector<HTMLButtonElement>('[data-testid="teacher-form-save-button"]')
  if (!button) throw new Error('teacher-form-save-button bulunamadı')
  return button
}

function setInput(id: string, value: string): void {
  const input = document.body.querySelector<HTMLInputElement>(`#${id}`)
  if (!input) throw new Error(`#${id} bulunamadı`)
  input.value = value
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

/** Yalnız kimlik alanlarını doldurur; formu geçerli kılmak için yeterlidir. */
function fillIdentity(): void {
  setInput('teacher-first', 'Mehmet')
  setInput('teacher-last', 'Can')
}

type SavePayload = [NewTeacher, { effectiveDate: string | null; reason: string | null }]

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('TeacherFormDialog — yürürlük tarihi/gerekçe penceresi', () => {
  it('planlama evresinde yeni öğretmeni doğrudan kaydeder, pencere açılmaz', async () => {
    const wrapper = mountDialog({ teacher: null, term: planningTerm })
    await nextTick()
    fillIdentity()
    await nextTick()

    saveButton().dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await nextTick()

    expect(document.body.querySelector('[data-testid="change-details-dialog"]')).toBeNull()
    const emitted = wrapper.emitted('save')
    expect(emitted).toHaveLength(1)
    const [input, details] = emitted![0] as SavePayload
    expect(input.firstName).toBe('Mehmet')
    expect(details).toEqual({ effectiveDate: null, reason: null })

    wrapper.unmount()
  })

  it('dönem başladıktan sonra yeni öğretmen oluştururken önce pencereyi açar, seçilen tarih/gerekçeyi kaydete taşır', async () => {
    const wrapper = mountDialog({ teacher: null, term: startedTerm })
    await nextTick()
    fillIdentity()
    await nextTick()

    saveButton().dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await nextTick()

    expect(document.body.querySelector('[data-testid="change-details-dialog"]')).not.toBeNull()
    expect(wrapper.emitted('save')).toBeUndefined()

    await wrapper.findComponent(EffectiveDateField).vm.$emit('update:modelValue', '2026-10-15')
    const reasonField = document.body.querySelector<HTMLTextAreaElement>('[data-testid="change-details-reason"]')
    if (!reasonField) throw new Error('change-details-reason bulunamadı')
    reasonField.value = 'yeni öğretmen ataması'
    reasonField.dispatchEvent(new Event('input', { bubbles: true }))
    await nextTick()

    const confirmButton = document.body.querySelector<HTMLButtonElement>(
      '[data-testid="change-details-confirm-button"]',
    )
    if (!confirmButton) throw new Error('change-details-confirm-button bulunamadı')
    confirmButton.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await nextTick()

    const emitted = wrapper.emitted('save')
    expect(emitted).toHaveLength(1)
    const [, details] = emitted![0] as SavePayload
    expect(details.reason).toBe('yeni öğretmen ataması')
    expect(details.effectiveDate).toBe('2026-10-15')

    wrapper.unmount()
  })

  it('dönem başlamış olsa da yalnız kimlik alanı değişen bir düzenlemeyi doğrudan kaydeder', async () => {
    const wrapper = mountDialog({ teacher: teacherFixture, term: startedTerm })
    await nextTick()
    setInput('teacher-last', 'Demir')
    await nextTick()

    saveButton().dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await nextTick()

    expect(document.body.querySelector('[data-testid="change-details-dialog"]')).toBeNull()
    const emitted = wrapper.emitted('save')
    expect(emitted).toHaveLength(1)
    const [input, details] = emitted![0] as SavePayload
    expect(input.lastName).toBe('Demir')
    expect(details).toEqual({ effectiveDate: null, reason: null })

    wrapper.unmount()
  })

  it('dönem başladıktan sonra istihdam türü (yük alanı) değişince pencereyi açar', async () => {
    const wrapper = mountDialog({ teacher: teacherFixture, term: startedTerm })
    await nextTick()

    // İstihdam türü Select'i düzenlemede de gösterilir; `load_changed`
    // kontrolündeki beş alandan biridir (bkz. Rust `load_changed`).
    wrapper.findAllComponents({ name: 'Select' })[0].vm.$emit('update:modelValue', 'contracted')
    await nextTick()

    saveButton().dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await nextTick()

    expect(document.body.querySelector('[data-testid="change-details-dialog"]')).not.toBeNull()
    expect(wrapper.emitted('save')).toBeUndefined()

    wrapper.unmount()
  })

  it('dönem bilgisi henüz yüklenmemişse (null) doğrudan kaydeder', async () => {
    const wrapper = mountDialog({ teacher: null, term: null })
    await nextTick()
    fillIdentity()
    await nextTick()

    saveButton().dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await nextTick()

    expect(document.body.querySelector('[data-testid="change-details-dialog"]')).toBeNull()
    expect(wrapper.emitted('save')).toHaveLength(1)

    wrapper.unmount()
  })
})
