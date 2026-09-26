import { beforeEach, describe, expect, it } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import DatePicker from 'openvue/datepicker'
import Checkbox from 'openvue/checkbox'
import TermDatesDialog from './TermDatesDialog.vue'
import { labels } from '../../i18n/labels'
import type { TermWithDates } from '../../types/models'

function termFixture(overrides: Partial<TermWithDates> = {}): TermWithDates {
  return {
    term: '2026-2027/1',
    startDate: '2026-09-01',
    endDate: '2027-06-30',
    datesConfirmed: false,
    isPlanning: true,
    defaultAsOf: '2026-09-26',
    earliestAllowedDate: '2026-09-01',
    ...overrides,
  }
}

function mountDialog(options: { term?: TermWithDates | null; saving?: boolean } = {}) {
  return mount(TermDatesDialog, {
    props: {
      visible: true,
      term: options.term === undefined ? termFixture() : options.term,
      saving: options.saving ?? false,
    },
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }]],
    },
    attachTo: document.body,
  })
}

type MountedDialog = ReturnType<typeof mountDialog>

function saveButton(): HTMLButtonElement {
  return document.body.querySelector<HTMLButtonElement>('[data-testid="term-dates-save-button"]')!
}

function cancelButton(): HTMLButtonElement {
  return document.body.querySelector<HTMLButtonElement>('[data-testid="term-dates-cancel-button"]')!
}

async function clickSave(): Promise<void> {
  saveButton().dispatchEvent(new MouseEvent('click', { bubbles: true }))
  await flushPromises()
}

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('TermDatesDialog — mevcut değerlerle açılış', () => {
  it('başlangıç, bitiş ve onay durumunu seçilen dönemden alır', async () => {
    // Arrange & Act
    const wrapper = mountDialog({
      term: termFixture({ startDate: '2026-09-14', endDate: '2027-06-20', datesConfirmed: true }),
    })
    await flushPromises()

    // Assert
    const pickers = wrapper.findAllComponents(DatePicker)
    expect(pickers).toHaveLength(2)
    const startValue = pickers[0].props('modelValue') as Date
    const endValue = pickers[1].props('modelValue') as Date
    expect(startValue.getFullYear()).toBe(2026)
    expect(startValue.getMonth()).toBe(8) // 0 tabanlı: Eylül
    expect(startValue.getDate()).toBe(14)
    expect(endValue.getFullYear()).toBe(2027)
    expect(endValue.getMonth()).toBe(5) // Haziran
    expect(endValue.getDate()).toBe(20)

    const checkbox = wrapper.findComponent(Checkbox)
    expect(checkbox.props('modelValue')).toBe(true)
    wrapper.unmount()
  })
});

describe('TermDatesDialog — kaydet', () => {
  it('Kaydet tıklanınca doğru { startDate, endDate, confirmed } yükünü yayar', async () => {
    // Arrange
    const wrapper = mountDialog({ term: termFixture({ datesConfirmed: false }) })
    await flushPromises()
    const pickers = wrapper.findAllComponents(DatePicker)

    // Act — tarihleri değiştir ve onay kutusunu işaretle.
    await pickers[0].vm.$emit('update:modelValue', new Date(2026, 8, 1))
    await pickers[1].vm.$emit('update:modelValue', new Date(2027, 5, 30))
    await wrapper.findComponent(Checkbox).vm.$emit('update:modelValue', true)
    await clickSave()

    // Assert
    expect(wrapper.emitted('save')).toEqual([
      [{ startDate: '2026-09-01', endDate: '2027-06-30', confirmed: true }],
    ])
    wrapper.unmount()
  })

  it('kendini kapatmaz — kapanış üst görünüme bırakılır', async () => {
    // Arrange
    const wrapper = mountDialog()
    await flushPromises()

    // Act
    await clickSave()

    // Assert
    expect(wrapper.emitted('update:visible')).toBeUndefined()
    wrapper.unmount()
  })

  it('saving true iken Kaydet ve Vazgeç devre dışıdır', async () => {
    // Arrange & Act
    mountDialog({ saving: true })
    await flushPromises()

    // Assert
    expect(saveButton().disabled).toBe(true)
    expect(cancelButton().disabled).toBe(true)
  })
})

describe('TermDatesDialog — vazgeç', () => {
  it('Vazgeç tıklanınca yalnızca update:visible(false) yayar, save yayılmaz', async () => {
    // Arrange
    const wrapper: MountedDialog = mountDialog()
    await flushPromises()

    // Act
    cancelButton().dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    // Assert
    expect(wrapper.emitted('update:visible')).toEqual([[false]])
    expect(wrapper.emitted('save')).toBeUndefined()
    wrapper.unmount()
  })
})

describe('TermDatesDialog — etiketler', () => {
  it('pencere başlığı ve onay kutusu metnini labels.ts üzerinden gösterir', async () => {
    // Arrange & Act
    mountDialog()
    await flushPromises()

    // Assert
    expect(document.body.textContent).toContain(labels.termManagement.editDates)
    expect(document.body.textContent).toContain(labels.termManagement.confirmCheckbox)
  })
})
