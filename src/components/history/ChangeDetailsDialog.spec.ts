import { beforeEach, describe, expect, it } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import ChangeDetailsDialog from './ChangeDetailsDialog.vue'
import EffectiveDateField from './EffectiveDateField.vue'
import { labels } from '../../i18n/labels'
import type { TermWithDates } from '../../types/models'

const startedTerm: TermWithDates = {
  term: '2026-2027/1',
  startDate: '2026-09-14',
  endDate: '2027-06-30',
  datesConfirmed: true,
  isPlanning: false,
  defaultAsOf: '2026-09-19',
  earliestAllowedDate: '2026-10-05',
}

interface MountOptions {
  effectiveDate?: string | null
  reason?: string
  visible?: boolean
}

function mountDialog(options: MountOptions = {}) {
  return mount(ChangeDetailsDialog, {
    props: {
      visible: options.visible ?? true,
      term: startedTerm,
      title: labels.history.changeDetailsTitle,
      effectiveDate: options.effectiveDate ?? null,
      reason: options.reason ?? '',
    },
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }]],
    },
    attachTo: document.body,
  })
}

type MountedDialog = ReturnType<typeof mountDialog>

function reasonInput(): HTMLTextAreaElement {
  return document.body.querySelector<HTMLTextAreaElement>('[data-testid="change-details-reason"]')!
}

async function typeReason(value: string): Promise<void> {
  reasonInput().value = value
  reasonInput().dispatchEvent(new Event('input', { bubbles: true }))
  await flushPromises()
}

async function pickDate(wrapper: MountedDialog, value: string): Promise<void> {
  wrapper.findComponent(EffectiveDateField).vm.$emit('update:modelValue', value)
  await flushPromises()
}

async function clickConfirm(): Promise<void> {
  document.body
    .querySelector<HTMLButtonElement>('[data-testid="change-details-confirm-button"]')!
    .dispatchEvent(new MouseEvent('click', { bubbles: true }))
  await flushPromises()
}

function hasRequiredWarning(): boolean {
  return document.body.querySelector('.field-error, .change-details-error') !== null
}

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('ChangeDetailsDialog', () => {
  it('opens without any required-field warning', async () => {
    const wrapper = mountDialog()
    await flushPromises()

    expect(document.body.querySelector('[data-testid="change-details-dialog"]')).not.toBeNull()
    expect(hasRequiredWarning()).toBe(false)
    wrapper.unmount()
  })

  it('does not confirm and shows the warnings when both fields are empty', async () => {
    const wrapper = mountDialog()
    await flushPromises()

    await clickConfirm()

    expect(wrapper.emitted('confirm')).toBeUndefined()
    expect(document.body.textContent).toContain(labels.history.reasonRequired)
    expect(document.body.textContent).toContain(labels.effectiveDateField.required)
    wrapper.unmount()
  })

  it('does not confirm while only the reason is filled', async () => {
    const wrapper = mountDialog()
    await flushPromises()

    await typeReason('program değişti')
    await clickConfirm()

    expect(wrapper.emitted('confirm')).toBeUndefined()
    expect(document.body.textContent).toContain(labels.effectiveDateField.required)
    wrapper.unmount()
  })

  it('emits confirm with the trimmed reason once both fields are filled', async () => {
    const wrapper = mountDialog()
    await flushPromises()

    await pickDate(wrapper, '2026-10-15')
    await typeReason('  program değişti  ')
    await clickConfirm()

    expect(wrapper.emitted('confirm')).toEqual([[{ effectiveDate: '2026-10-15', reason: 'program değişti' }]])
    wrapper.unmount()
  })

  it('shows the reason warning after the field is touched and left empty', async () => {
    const wrapper = mountDialog()
    await flushPromises()
    expect(hasRequiredWarning()).toBe(false)

    reasonInput().dispatchEvent(new Event('blur'))
    await flushPromises()

    expect(document.body.textContent).toContain(labels.history.reasonRequired)
    // Tarih alanı henüz denenmediği için uyarısı görünmez.
    expect(document.body.textContent).not.toContain(labels.effectiveDateField.required)
    wrapper.unmount()
  })

  it('emits cancel from the cancel button', async () => {
    const wrapper = mountDialog()
    await flushPromises()

    document.body
      .querySelector<HTMLButtonElement>('[data-testid="change-details-cancel-button"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    expect(wrapper.emitted('cancel')).toHaveLength(1)
    expect(wrapper.emitted('confirm')).toBeUndefined()
    wrapper.unmount()
  })

  it('opens prefilled with the given date and reason', async () => {
    const wrapper = mountDialog({ effectiveDate: '2026-10-12', reason: 'eski gerekçe' })
    await flushPromises()

    expect(wrapper.findComponent(EffectiveDateField).props('modelValue')).toBe('2026-10-12')
    expect(reasonInput().value).toBe('eski gerekçe')

    // Değerleri değiştirmeden onaylamak, önceden dolu değerlerle onaylar.
    await clickConfirm()
    expect(wrapper.emitted('confirm')).toEqual([[{ effectiveDate: '2026-10-12', reason: 'eski gerekçe' }]])
    wrapper.unmount()
  })

  it('reloads the initial values and clears warnings on every open', async () => {
    const wrapper = mountDialog({ effectiveDate: '2026-10-12', reason: 'eski gerekçe' })
    await flushPromises()
    await typeReason('')
    await clickConfirm()
    expect(document.body.textContent).toContain(labels.history.reasonRequired)

    await wrapper.setProps({ visible: false })
    await flushPromises()
    await wrapper.setProps({ visible: true })
    await flushPromises()

    expect(reasonInput().value).toBe('eski gerekçe')
    expect(hasRequiredWarning()).toBe(false)
    wrapper.unmount()
  })
})
