import { beforeEach, describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import StudentFormDialog from './StudentFormDialog.vue'
import { labels } from '../../i18n/labels'
import type { NewStudent, Student } from '../../types/models'

// Dialog `body`'ye teleport edildiğinden `wrapper.find` onu göremez;
// CompanyFormDialog.spec.ts'teki desende olduğu gibi doğrudan `document.body` sorgulanır.

function studentFixture(overrides: Partial<Student> = {}): Student {
  return {
    id: 1,
    firstName: 'Ayşe',
    lastName: 'Kaya',
    studentNo: '101',
    grade: '12/A',
    branch: 'Elektrik-Elektronik',
    companyId: null,
    submittedAt: null,
    term: '2026-2027/1',
    ...overrides,
  }
}

function mountDialog(student: Student | null) {
  return mount(StudentFormDialog, {
    props: { visible: true, student },
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }]],
    },
    attachTo: document.body,
  })
}

function setInput(id: string, value: string): void {
  const input = document.body.querySelector<HTMLInputElement | HTMLTextAreaElement>(`#${id}`)
  if (!input) throw new Error(`#${id} bulunamadı`)
  input.value = value
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

function saveButton(): HTMLButtonElement {
  const buttons = [...document.body.querySelectorAll<HTMLButtonElement>('button')]
  const button = buttons.find((b) => b.textContent?.trim() === labels.common.save)
  if (!button) throw new Error('Kaydet düğmesi bulunamadı')
  return button
}

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('StudentFormDialog — işletme alanı kaldırıldı', () => {
  it('İşletme etiketini veya seçiciyi göstermez', async () => {
    // Arrange & Act
    mountDialog(null)
    await nextTick()

    // Assert
    expect(document.body.textContent).not.toContain(labels.student.company)
    expect(document.body.querySelector('#student-company')).toBeNull()
  })

  it('save olayında companyId alanını taşımaz', async () => {
    // Arrange
    const wrapper = mountDialog(null)
    await nextTick()
    setInput('student-first', 'Mehmet')
    setInput('student-last', 'Can')
    setInput('student-grade', '12/C')
    setInput('student-branch', 'Elektrik-Elektronik')
    await nextTick()

    // Act
    saveButton().click()
    await nextTick()

    // Assert
    const emitted = wrapper.emitted('save')
    expect(emitted).toBeTruthy()
    const savedInput = emitted?.[0]?.[0] as NewStudent
    expect(savedInput).not.toHaveProperty('companyId')
  })

  it('mevcut öğrenciyi düzenlerken de işletme alanı görünmez', async () => {
    // Arrange & Act
    mountDialog(studentFixture({ companyId: 5 }))
    await nextTick()

    // Assert
    expect(document.body.querySelector('#student-company')).toBeNull()
  })
})
