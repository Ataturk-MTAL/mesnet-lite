<template>
  <div class="page">
    <h1 class="page-title">{{ labels.nav.dashboard }}</h1>

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

    <Card v-if="stats && stats.companyCount === 0">
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
import { computed, onMounted, ref } from 'vue'
import { useToast } from 'openvue/usetoast'
import { dashboardApi } from '../api/dashboard'
import type { DashboardStats } from '../api/dashboard'
import { labels } from '../i18n/labels'

const toast = useToast()
const stats = ref<DashboardStats | null>(null)

interface StatCard {
  to: string
  icon: string
  label: string
  value: number
}

const statCards = computed<StatCard[]>(() => {
  const current = stats.value
  return [
    {
      to: '/companies',
      icon: 'pi pi-building',
      label: labels.nav.companies,
      value: current?.companyCount ?? 0,
    },
    {
      to: '/students',
      icon: 'pi pi-users',
      label: labels.nav.students,
      value: current?.studentCount ?? 0,
    },
    {
      to: '/teachers',
      icon: 'pi pi-id-card',
      label: labels.dashboard.activeTeachers,
      value: current?.activeTeacherCount ?? 0,
    },
  ]
})

/** Kullanıcının müdahale etmesi gereken eksikler. Sıfır olanlar gösterilmez. */
const attentionItems = computed(() => {
  const current = stats.value
  if (!current) return []

  return [
    {
      count: current.companiesWithoutLocation,
      label: labels.dashboard.companiesWithoutLocation,
    },
    {
      count: current.studentsWithoutCompany,
      label: labels.dashboard.studentsWithoutCompany,
    },
    {
      count: current.companiesWithoutStudents,
      label: labels.dashboard.companiesWithoutStudents,
    },
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

onMounted(load)
</script>

<style scoped>
.page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
.page-title { font-size: 1.5rem; font-weight: 600; margin: 0; }
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
