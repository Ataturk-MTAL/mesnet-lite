import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import type { VueWrapper } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import Aura from '@openvue/themes/aura'
import ProposalDialog from './ProposalDialog.vue'
import { fullProposalFixture, proposalFixture, proposedAssignmentFixture } from './proposalFixture'
import type { AllocationProposal, ProposalMode } from '../../api/assignments'
import { labels } from '../../i18n/labels'

const proposeMock = vi.fn<(mode: ProposalMode) => Promise<AllocationProposal>>()
vi.mock('../../api/assignments', async () => {
  const actual = await vi.importActual<typeof import('../../api/assignments')>('../../api/assignments')
  return { ...actual, assignmentsApi: { propose: (mode: ProposalMode) => proposeMock(mode) } }
})

const toastAddMock = vi.fn<(message: { severity: string; detail?: string }) => void>()
vi.mock('openvue/usetoast', () => ({ useToast: () => ({ add: toastAddMock }) }))

async function mountDialog(props: Partial<{ isPlanning: boolean; isApplying: boolean; rejection: string | null }> = {}) {
  const wrapper = mount(ProposalDialog, {
    props: { visible: true, isPlanning: true, isApplying: false, rejection: null, ...props },
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }], ToastService],
    },
    attachTo: document.body,
  })
  await flushPromises()
  return wrapper
}

function bodyButton(text: string): HTMLButtonElement | undefined {
  return Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent?.includes(text))
}

function section(testId: string): Element | null {
  return document.body.querySelector(`[data-testid="${testId}"]`)
}

beforeEach(() => {
  proposeMock.mockReset()
  toastAddMock.mockReset()
  document.body.replaceChildren()
})

describe('ProposalDialog', () => {
  it('tüm bölümleri, özeti ve gerekçeleri gösterir', async () => {
    // Arrange
    proposeMock.mockResolvedValue(fullProposalFixture())

    // Act
    const wrapper = await mountDialog()

    // Assert
    const text = document.body.textContent ?? ''
    for (const id of [
      'proposal-assignments',
      'proposal-hour-changes',
      'proposal-released',
      'proposal-unassigned',
      'proposal-group-splits',
      'proposal-teacher-loads',
    ]) {
      expect(section(id), id).not.toBeNull()
    }
    // Taşınanda önceki yer, yenide değil.
    expect(text).toContain(labels.allocation.proposalMoved)
    expect(text).toContain(labels.allocation.proposalNew)
    expect(text).toContain(`${labels.allocation.proposalPreviousPlace}: Ayşe Demir Salı 10–11`)
    // Saat değişikliği ve gerekçe; 0'a inen belirgin.
    expect(text).toContain('2 → 3 saat')
    expect(text).toContain('Havuz tükendiği için fahriye düşürüldü.')
    expect(section('proposal-hour-changes')?.textContent).toContain(labels.allocation.proposalHonoraryBadge)
    expect(text).toContain('Salı günü için uygun boş saat yok.')
    expect(text).toContain(labels.allocation.proposalWasAssigned)
    expect(text).toContain('Toroslar')
    expect(text).toContain('18 / 20')
    expect(text).toContain('Havuz zaten aşılmıştı.')
    // Özet: yerleşen, toplam saat, negatif kalan havuz.
    expect(section('proposal-summary')?.textContent).toContain('30')
    expect(section('proposal-summary')?.textContent).toContain('-2')
    wrapper.unmount()
  })

  it('boş bölümleri göstermez; değişiklik yoksa mesaj çıkar ve Uygula kapalıdır', async () => {
    // Arrange
    proposeMock.mockResolvedValue(proposalFixture({ poolRemaining: null }))

    // Act
    const wrapper = await mountDialog()

    // Assert
    expect(section('proposal-no-changes')?.textContent).toContain(labels.allocation.proposalNoChanges)
    for (const id of ['proposal-assignments', 'proposal-hour-changes', 'proposal-released', 'proposal-unassigned', 'proposal-group-splits']) {
      expect(section(id), id).toBeNull()
    }
    expect(section('proposal-summary')?.textContent).toContain(labels.allocation.proposalPoolUndefined)
    expect(bodyButton(labels.allocation.proposalApply)?.disabled).toBe(true)
    wrapper.unmount()
  })

  it('yalnız saat değişikliği ya da bırakma varsa da Uygula açıktır', async () => {
    // Arrange
    const full = fullProposalFixture()
    proposeMock.mockResolvedValue(proposalFixture({ hourChanges: full.hourChanges }))

    // Act
    const wrapper = await mountDialog()

    // Assert
    expect(section('proposal-no-changes')).toBeNull()
    expect(bodyButton(labels.allocation.proposalApply)?.disabled).toBe(false)
    wrapper.unmount()
  })

  it('varsayılan kip fillGaps; kip değişince öneri yeni kiple yeniden istenir', async () => {
    // Arrange
    proposeMock.mockResolvedValue(proposalFixture())
    const wrapper = await mountDialog()
    expect(proposeMock).toHaveBeenCalledTimes(1)
    expect(proposeMock).toHaveBeenLastCalledWith('fillGaps')

    // Act
    bodyButton(labels.allocation.proposalModeRedistribute)!.click()
    await flushPromises()

    // Assert
    expect(proposeMock).toHaveBeenCalledTimes(2)
    expect(proposeMock).toHaveBeenLastCalledWith('redistribute')
    wrapper.unmount()
  })

  it('dönem başladıysa "Baştan dağıt" kapalıdır ve açıklama görünür', async () => {
    // Arrange & Act
    proposeMock.mockResolvedValue(proposalFixture())
    const wrapper = await mountDialog({ isPlanning: false })

    // Assert
    const button = bodyButton(labels.allocation.proposalModeRedistribute)!
    expect(button.disabled || button.getAttribute('aria-disabled') === 'true' || button.hasAttribute('data-p-disabled')).toBe(true)
    button.click()
    await flushPromises()
    expect(proposeMock).toHaveBeenCalledTimes(1)
    expect(document.body.textContent).toContain(labels.allocation.proposalRedistributeDisabled)
    wrapper.unmount()
  })

  it('Uygula, mevcut öneriyi `apply` olayıyla yayar', async () => {
    // Arrange
    const proposal = proposalFixture({ assignments: [proposedAssignmentFixture()] })
    proposeMock.mockResolvedValue(proposal)
    const wrapper: VueWrapper = await mountDialog()

    // Act
    bodyButton(labels.allocation.proposalApply)!.click()
    await flushPromises()

    // Assert
    expect(wrapper.emitted('apply')?.[0]).toEqual([proposal])
    wrapper.unmount()
  })

  it('reddedilen uygulamanın gerekçesini gösterir', async () => {
    proposeMock.mockResolvedValue(proposalFixture({ assignments: [proposedAssignmentFixture()] }))
    const wrapper = await mountDialog({ rejection: 'Firma B için çakışma var' })

    expect(document.body.textContent).toContain('Firma B için çakışma var')
    expect(document.body.textContent).toContain(labels.allocation.proposalRejectedNote)
    wrapper.unmount()
  })

  it('öneri istenemezse Rust mesajı toast ile gösterilir ve pencere kapanmayı ister', async () => {
    // Arrange
    proposeMock.mockRejectedValue(new Error('Dönem başladı; baştan dağıtılamaz.'))

    // Act
    const wrapper = await mountDialog()

    // Assert
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'error', detail: 'Dönem başladı; baştan dağıtılamaz.' }),
    )
    expect(wrapper.emitted('update:visible')?.[0]).toEqual([false])
    wrapper.unmount()
  })
})
