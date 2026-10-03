import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import Tooltip from 'openvue/tooltip'
import Aura from '@openvue/themes/aura'
import GroupingSettings from './GroupingSettings.vue'
import { labels } from '../../i18n/labels'
import { defaultGrouping } from '../../utils/groupingSettings'
import type { GroupingDraft } from '../../utils/groupingSettings'

function mountSettings(modelValue: GroupingDraft) {
  return mount(GroupingSettings, {
    props: {
      modelValue,
      neighborhoodOptions: ['Örnek', 'Kurgu'],
      districtOptions: ['Deneme'],
    },
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }]],
      directives: { tooltip: Tooltip },
    },
  })
}

function manualDraft(names: string[]): GroupingDraft {
  return {
    ...defaultGrouping(),
    mode: 'manual',
    groups: names.map((name) => ({ name, neighborhoods: [], districts: [] })),
  }
}

function lastEmitted(wrapper: ReturnType<typeof mountSettings>): GroupingDraft {
  const events = wrapper.emitted('update:modelValue')
  if (!events) throw new Error('update:modelValue yayınlanmadı')
  return events[events.length - 1][0] as GroupingDraft
}

describe('GroupingSettings yöntem seçimi', () => {
  it('mesafe modunda çap girişini gösterir, elle grup listesini göstermez', () => {
    const wrapper = mountSettings(defaultGrouping())

    expect(wrapper.find('#grouping-max-diameter').exists()).toBe(true)
    expect(wrapper.text()).not.toContain(labels.settings.grouping.addGroup)
    wrapper.unmount()
  })

  it('elle modda grup listesini gösterir, çap girişini göstermez', () => {
    const wrapper = mountSettings(manualDraft([]))

    expect(wrapper.find('#grouping-max-diameter').exists()).toBe(false)
    expect(wrapper.text()).toContain(labels.settings.grouping.addGroup)
    expect(wrapper.text()).toContain(labels.settings.grouping.noGroups)
    wrapper.unmount()
  })

  it('"Elle tanımlı gruplar" seçilince modu manual olarak yayınlar', async () => {
    const wrapper = mountSettings(defaultGrouping())

    await wrapper.find('#grouping-mode-manual').setValue(true)

    expect(lastEmitted(wrapper).mode).toBe('manual')
    wrapper.unmount()
  })

  it('her iki seçenek de etiketlidir', () => {
    const wrapper = mountSettings(defaultGrouping())

    expect(wrapper.text()).toContain(labels.settings.grouping.modeDistance)
    expect(wrapper.text()).toContain(labels.settings.grouping.modeManual)
    wrapper.unmount()
  })
})

describe('GroupingSettings mesafe doğrulaması', () => {
  it('aralık dışı çapta hata metnini gösterir', () => {
    const wrapper = mountSettings({ ...defaultGrouping(), maxDiameterKm: 80 })

    expect(wrapper.text()).toContain(labels.settings.grouping.maxDiameterInvalid)
    wrapper.unmount()
  })

  it('geçerli çapta ipucunu gösterir', () => {
    const wrapper = mountSettings(defaultGrouping())

    expect(wrapper.text()).toContain(labels.settings.grouping.maxDiameterHint)
    expect(wrapper.text()).not.toContain(labels.settings.grouping.maxDiameterInvalid)
    wrapper.unmount()
  })
})

describe('GroupingSettings elle gruplar', () => {
  it('Grup Ekle boş adlı yeni bir grup ekler', async () => {
    const wrapper = mountSettings(manualDraft(['Kurgu Grubu']))
    const add = wrapper.findAll('button').find((b) => b.text() === labels.settings.grouping.addGroup)!

    await add.trigger('click')

    expect(lastEmitted(wrapper).groups).toEqual([
      { name: 'Kurgu Grubu', neighborhoods: [], districts: [] },
      { name: '', neighborhoods: [], districts: [] },
    ])
    wrapper.unmount()
  })

  it('sil düğmesi yalnız ilgili grubu kaldırır ve girdiyi değiştirmez', async () => {
    const input = manualDraft(['A', 'B'])
    const wrapper = mountSettings(input)

    await wrapper.findAll(`button[aria-label="${labels.settings.grouping.removeGroup}"]`)[0].trigger('click')

    expect(lastEmitted(wrapper).groups.map((g) => g.name)).toEqual(['B'])
    expect(input.groups).toHaveLength(2)
    wrapper.unmount()
  })

  it('ad girişi yeni bir taslak yayınlar', async () => {
    const wrapper = mountSettings(manualDraft(['']))

    await wrapper.find('#grouping-name-0').setValue('Deneme Bölgesi')

    expect(lastEmitted(wrapper).groups[0].name).toBe('Deneme Bölgesi')
    wrapper.unmount()
  })

  it('boş adı ve yinelenen adı arayüzde işaretler', () => {
    const wrapper = mountSettings(manualDraft(['Işık', '', 'ışık']))

    const alerts = wrapper.findAll('[role="alert"]').map((el) => el.text())
    expect(alerts).toEqual([labels.settings.grouping.nameEmpty, labels.settings.grouping.nameDuplicate])
    wrapper.unmount()
  })

  it('mahalle ve ilçe girişlerinin etiketi ve erişilebilir adı vardır', () => {
    const wrapper = mountSettings(manualDraft(['A']))

    expect(wrapper.find('label[for="grouping-neighborhoods-0"]').text()).toBe(labels.settings.grouping.neighborhoods)
    expect(wrapper.find('label[for="grouping-districts-0"]').text()).toBe(labels.settings.grouping.districts)
    wrapper.unmount()
  })

  it('mevcut mahalle değerlerini çip olarak gösterir', () => {
    const input: GroupingDraft = {
      ...manualDraft(['A']),
      groups: [{ name: 'A', neighborhoods: ['Örnek'], districts: ['Deneme'] }],
    }
    const wrapper = mountSettings(input)

    expect(wrapper.text()).toContain('Örnek')
    expect(wrapper.text()).toContain('Deneme')
    wrapper.unmount()
  })
})
