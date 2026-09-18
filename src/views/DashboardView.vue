<template>
  <div class="page">
    <div class="page-header">
      <h1 class="page-title">{{ labels.nav.dashboard }}</h1>
      <Tag v-if="stats?.term" :value="stats.term" severity="secondary" icon="pi pi-calendar" />
    </div>

    <!-- Saat dengesi: dağıtılabilir azami, dağıtılmış, kalan -->
    <Card>
      <template #title>{{ labels.dashboard.hourBalance }}</template>
      <template #content>
        <div class="balance-grid">
          <div class="balance-item">
            <div class="balance-value">{{ stats?.totalCapacityHours ?? 0 }}</div>
            <div class="balance-label">{{ labels.dashboard.totalCapacity }}</div>
            <small class="balance-note">{{ labels.dashboard.totalCapacityNote }}</small>
          </div>

          <div class="balance-item">
            <div class="balance-value">{{ stats?.assignedHours ?? 0 }}</div>
            <div class="balance-label">{{ labels.dashboard.assignedHours }}</div>
            <small class="balance-note">{{ labels.dashboard.assignedHoursNote }}</small>
          </div>

          <div class="balance-item">
            <div class="balance-value" :class="{ 'balance-value--over': isOverCapacity }">
              {{ stats?.remainingHours ?? 0 }}
            </div>
            <div class="balance-label">{{ labels.dashboard.remainingHours }}</div>
            <small class="balance-note">{{ labels.dashboard.remainingHoursNote }}</small>
          </div>
        </div>

        <ProgressBar
          :value="usagePercent"
          :class="{ 'usage-over': isOverCapacity }"
          class="usage-bar"
        />
        <small class="usage-caption">
          {{ labels.dashboard.usage }}: %{{ usagePercent }}
          <span v-if="stats && stats.activeTeacherCount > 0">
            — {{ stats.activeTeacherCount }} {{ labels.dashboard.activeTeachersLower }}
          </span>
        </small>

        <Message v-if="isOverCapacity" severity="error" :closable="false" class="balance-note-box">
          {{ labels.dashboard.overCapacityWarning }}
          <span v-if="stats && stats.teachersOverCapacity > 0">
            ({{ stats.teachersOverCapacity }} {{ labels.dashboard.teachersOverCapacity }})
          </span>
        </Message>
        <Message
          v-else-if="stats && stats.activeTeacherCount === 0"
          severity="warn"
          :closable="false"
          class="balance-note-box"
        >
          {{ labels.dashboard.noActiveTeachers }}
        </Message>
      </template>
    </Card>

    <div class="stat-grid">
      <RouterLink v-for="card in statCards" :key="card.to" :to="card.to" class="stat-link">
        <Card class="stat-card">
          <template #content>
            <div class="stat">
              <i :class="card.icon" class="stat-icon" />
              <div>
                <div class="stat-value">{{ card.value }}</div>
                <div class="stat-label">{{ card.label }}</div>
              </div>
            </div>
          </template>
        </Card>
      </RouterLink>
    </div>

    <Card>
      <template #title>{{ labels.dashboard.attentionTitle }}</template>
      <template #content>
        <div v-if="attentionItems.length === 0" class="all-clear">
          <i class="pi pi-check-circle" />
          {{ labels.dashboard.allClear }}
        </div>
        <ul v-else class="attention-list">
          <li v-for="item in attentionItems" :key="item.label">
            <Tag :value="String(item.count)" severity="warn" />
            <span>{{ item.label }}</span>
          </li>
        </ul>
      </template>
    </Card>

    <Card v-if="stats && stats.studentCount === 0">
      <template #title>{{ labels.dashboard.gettingStarted }}</template>
      <template #content>
        <ol class="steps">
          <li>{{ labels.dashboard.step1 }}</li>
          <li>{{ labels.dashboard.step2 }}</li>
          <li>{{ labels.dashboard.step3 }}</li>
        </ol>
        <RouterLink to="/import-export">
          <Button :label="labels.nav.importExport" icon="pi pi-file-import" />
        </RouterLink>
      </template>
    </Card>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useToast } from 'openvue/usetoast'
import { dashboardApi } from '../api/dashboard'
import type { DashboardStats } from '../api/dashboard'
import { labels } from '../i18n/labels'
import { activeTerm } from '../composables/useTerm'

const toast = useToast()
const stats = ref<DashboardStats | null>(null)

const isOverCapacity = computed(() => (stats.value?.remainingHours ?? 0) < 0)

/** Dağıtılmışın azamiye oranı. Kapasite sıfırken oran tanımsızdır, 0 gösterilir. */
const usagePercent = computed(() => {
  const current = stats.value
  if (!current || current.totalCapacityHours <= 0) return 0
  return Math.round((current.assignedHours / current.totalCapacityHours) * 100)
})

interface StatCard {
  to: string
  icon: string
  label: string
  value: number
}

const statCards = computed<StatCard[]>(() => [
  {
    to: '/companies',
    icon: 'pi pi-building',
    label: labels.nav.companies,
    value: stats.value?.companyCount ?? 0,
  },
  {
    to: '/students',
    icon: 'pi pi-users',
    label: labels.dashboard.studentsThisTerm,
    value: stats.value?.studentCount ?? 0,
  },
  {
    to: '/teachers',
    icon: 'pi pi-id-card',
    label: labels.dashboard.activeTeachers,
    value: stats.value?.activeTeacherCount ?? 0,
  },
])

/** Kullanıcının müdahale etmesi gereken eksikler. Sıfır olanlar gösterilmez. */
const attentionItems = computed(() => {
  const current = stats.value
  if (!current) return []

  return [
    { count: current.companiesWithoutAssignment, label: labels.dashboard.companiesWithoutAssignment },
    { count: current.studentsWithoutCompany, label: labels.dashboard.studentsWithoutCompany },
    { count: current.companiesWithoutLocation, label: labels.dashboard.companiesWithoutLocation },
    { count: current.companiesWithoutStudents, label: labels.dashboard.companiesWithoutStudents },
  ].filter((item) => item.count > 0)
})

async function load(): Promise<void> {
  try {
    stats.value = await dashboardApi.get()
  } catch (error: unknown) {
    const detail = error instanceof Error ? error.message : labels.common.error
    toast.add({ severity: 'error', summary: labels.common.error, detail, life: 6000 })
  }
}

// Dönem değişince sayılar da değişmeli.
watch(activeTerm, load)

onMounted(load)
</script>

<style scoped>
.page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
.page-header { display: flex; align-items: center; gap: 0.75rem; }
.page-title { font-size: 1.5rem; font-weight: 600; margin: 0; }

.balance-grid { display: flex; flex-wrap: wrap; gap: 2rem; }
.balance-item { min-width: 12rem; }
.balance-value { font-size: 2rem; font-weight: 700; line-height: 1.1; }
.balance-value--over { color: var(--p-red-500); }
.balance-label { font-size: 0.9375rem; font-weight: 500; margin-top: 0.125rem; }
.balance-note { display: block; color: var(--p-text-muted-color); font-size: 0.75rem; margin-top: 0.25rem; }
.usage-bar { margin-top: 1.25rem; height: 0.75rem; }
.usage-caption { display: block; margin-top: 0.375rem; color: var(--p-text-muted-color); font-size: 0.75rem; }
.balance-note-box { margin-top: 1rem; }

.stat-grid { display: flex; flex-wrap: wrap; gap: 1rem; }
.stat-link { text-decoration: none; color: inherit; flex: 1; min-width: 14rem; }
.stat-card { height: 100%; }
.stat { display: flex; align-items: center; gap: 1rem; }
.stat-icon { font-size: 1.75rem; color: var(--p-primary-color); }
.stat-value { font-size: 1.75rem; font-weight: 600; line-height: 1.1; }
.stat-label { font-size: 0.875rem; color: var(--p-text-muted-color); }

.attention-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.625rem; }
.attention-list li { display: flex; align-items: center; gap: 0.625rem; }
.all-clear { display: flex; align-items: center; gap: 0.5rem; color: var(--p-text-muted-color); }
.steps { margin: 0 0 1rem; padding-left: 1.25rem; display: flex; flex-direction: column; gap: 0.375rem; }
</style>
