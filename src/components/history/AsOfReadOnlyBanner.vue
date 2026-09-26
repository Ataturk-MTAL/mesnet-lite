<template>
  <Message
    v-if="isReadOnly"
    severity="warn"
    :closable="false"
    class="as-of-readonly-banner"
    data-testid="as-of-readonly-banner"
  >
    <div class="as-of-readonly-banner-content">
      <div class="as-of-readonly-banner-text">
        <strong>{{ labels.asOfDate.readOnlyBanner(formattedAsOfDate) }}</strong>
        <span>{{ labels.asOfDate.readOnlyBannerNote }}</span>
      </div>
      <Button
        :label="labels.asOfDate.backToDefault"
        :aria-label="labels.asOfDate.backToDefault"
        size="small"
        severity="secondary"
        outlined
        data-testid="as-of-readonly-banner-today-button"
        @click="asOfDateStore.goToToday"
      />
    </div>
  </Message>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { storeToRefs } from 'pinia'
import { labels } from '../../i18n/labels'
import { useAsOfDateStore } from '../../stores/asOfDate'
import { isoToDate } from '../../utils/isoDate'

/**
 * Tarihçesi olan ekranların ortak salt okunur başlığı. `useAsOfDateStore`
 * salt okunur DEĞİLSE kendini hiç göstermez; ekranlar koşulsuz
 * `<AsOfReadOnlyBanner />` yerleştirebilir.
 */
const asOfDateStore = useAsOfDateStore()
const { asOfDate, isReadOnly } = storeToRefs(asOfDateStore)

// `isoToDate` yerel yıl/ay/gün bileşenlerinden `Date` kurar (UTC'ye çevirmez),
// bu yüzden `toLocaleDateString('tr-TR')` gün kaymadan seçiciyle AYNI
// "gg.aa.yyyy" biçimini üretir (bkz. SettingsView.vue'daki yedekleme tarihi).
const formattedAsOfDate = computed(() => {
  const date = isoToDate(asOfDate.value)
  return date ? date.toLocaleDateString('tr-TR') : asOfDate.value
})
</script>

<style scoped>
.as-of-readonly-banner-content {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  flex-wrap: wrap;
  width: 100%;
}
.as-of-readonly-banner-text { display: flex; flex-direction: column; gap: 0.125rem; }
</style>
