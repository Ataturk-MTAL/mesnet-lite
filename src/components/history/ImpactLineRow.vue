<template>
  <div class="impact-line-row">
    <span class="impact-line-subject">{{ line.subjectLabel }}</span>
    <span class="impact-line-kind">{{ labels.history.eventKind[line.kind] }}</span>
    <span class="impact-line-change">{{ line.before ?? '—' }} → {{ line.after ?? '—' }}</span>
    <span class="impact-line-date">{{ line.effectiveDate }}</span>
  </div>
</template>

<script setup lang="ts">
import { labels } from '../../i18n/labels'
import type { ImpactLine } from '../../types/models'

// `ImpactDialog`'un Birincil ve Otomatik bölümlerinde aynı satır biçimi
// tekrarlandığı için ayrı bileşene çıkarıldı (DRY).
defineProps<{ line: ImpactLine }>()
</script>

<style scoped>
.impact-line-row { display: flex; flex-wrap: wrap; gap: 0.5rem 0.75rem; align-items: baseline; font-size: 0.875rem; }
.impact-line-subject { font-weight: 600; }
.impact-line-kind { color: var(--p-text-muted-color); }
.impact-line-date { margin-left: auto; color: var(--p-text-muted-color); font-size: 0.8125rem; }
</style>
