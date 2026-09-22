<template>
  <div class="shell">
    <!-- Geniş ekranda kalıcı kenar çubuğu. OpenVue (PrimeVue 4.5.5) v5'in bileşik
         Sidebar ailesini içermediği için navigasyon elle kuruluyor. -->
    <aside v-if="!isNarrow" class="sidebar" :class="{ 'sidebar--collapsed': isCollapsed }">
      <div class="sidebar-header">
        <span v-if="!isCollapsed" class="brand">{{ labels.app.title }}</span>
      </div>
      <div v-if="!isCollapsed" class="sidebar-as-of-date">
        <AsOfDatePicker />
      </div>
      <nav class="sidebar-nav">
        <div v-for="group in navGroups" :key="group.key" class="nav-group">
          <div v-if="!isCollapsed && group.label" class="nav-group-label">{{ group.label }}</div>
          <RouterLink
            v-for="item in group.items"
            :key="item.to"
            :to="item.to"
            class="nav-item"
            :title="item.label"
          >
            <i :class="item.icon" />
            <span v-if="!isCollapsed" class="nav-item-label">{{ item.label }}</span>
          </RouterLink>
        </div>
      </nav>
    </aside>

    <!-- Dar ekranda aynı navigasyon Drawer içinde açılır. -->
    <Drawer v-model:visible="isDrawerOpen" position="left" :header="labels.app.title">
      <div class="sidebar-as-of-date">
        <AsOfDatePicker />
      </div>
      <nav class="sidebar-nav">
        <div v-for="group in navGroups" :key="group.key" class="nav-group">
          <div v-if="group.label" class="nav-group-label">{{ group.label }}</div>
          <RouterLink
            v-for="item in group.items"
            :key="item.to"
            :to="item.to"
            class="nav-item"
            @click="isDrawerOpen = false"
          >
            <i :class="item.icon" />
            <span class="nav-item-label">{{ item.label }}</span>
          </RouterLink>
        </div>
      </nav>
    </Drawer>

    <main class="main">
      <header class="topbar">
        <Button
          icon="pi pi-bars"
          severity="secondary"
          text
          :aria-label="labels.nav.toggleMenu"
          @click="toggleMenu"
        />
        <div class="topbar-term">
          <i class="pi pi-calendar" />
          <Select
            :model-value="activeTerm"
            :options="termOptions"
            :editable="true"
            :placeholder="labels.term.placeholder"
            :aria-label="labels.term.label"
            class="term-select"
            @update:model-value="onTermChange"
          />
        </div>

        <div class="topbar-spacer" />
        <SelectButton
          :model-value="preference"
          :options="themeOptions"
          optionLabel="label"
          optionValue="value"
          :allowEmpty="false"
          :aria-label="labels.theme.label"
          @update:model-value="setThemePreference"
        >
          <template #option="{ option }">
            <i :class="option.icon" :title="option.label" />
          </template>
        </SelectButton>
      </header>
      <RouterView />
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { labels } from '../../i18n/labels'
import { useTheme } from '../../composables/useTheme'
import { activeTerm, terms, loadTerms, setActiveTerm } from '../../composables/useTerm'
import AsOfDatePicker from '../history/AsOfDatePicker.vue'

const { preference, setThemePreference } = useTheme()

// Dönem listesi; seçici serbest metin de kabul eder, böylece yeni bir
// eğitim-öğretim yılı listede olmadan da açılabilir.
const termOptions = computed(() => [...terms.value])

async function onTermChange(value: string | null): Promise<void> {
  const term = (value ?? '').trim()
  if (term.length === 0) return
  try {
    await setActiveTerm(term)
  } catch {
    // Ayar yazılamazsa seçici eski değerine döner; hata Toast ile
    // ekranların kendi yükleme akışında görünür.
  }
}

const themeOptions = [
  { value: 'light' as const, label: labels.theme.light, icon: 'pi pi-sun' },
  { value: 'dark' as const, label: labels.theme.dark, icon: 'pi pi-moon' },
  { value: 'system' as const, label: labels.theme.system, icon: 'pi pi-desktop' },
]

const isCollapsed = ref(false)
const isDrawerOpen = ref(false)
const isNarrow = ref(false)

// Dar ekranda kalıcı çubuk yerine Drawer kullanılır.
const NARROW_BREAKPOINT_PX = 900

function updateWidth(): void {
  isNarrow.value = window.innerWidth < NARROW_BREAKPOINT_PX
}

onMounted(() => {
  updateWidth()
  window.addEventListener('resize', updateWidth)
  // Dönem listesi bir kez yüklenir; okunamazsa seçici boş kalır ama
  // uygulama çalışmaya devam eder.
  void loadTerms().catch(() => undefined)
})

onUnmounted(() => window.removeEventListener('resize', updateWidth))

function toggleMenu(): void {
  if (isNarrow.value) {
    isDrawerOpen.value = !isDrawerOpen.value
  } else {
    isCollapsed.value = !isCollapsed.value
  }
}

interface NavItem {
  to: string
  label: string
  icon: string
}

interface NavGroup {
  /** v-for anahtarı; etiket boş olabildiği için ayrı tutulur. */
  key: string
  label: string
  items: readonly NavItem[]
}

// Faz 2 (Planlama) ve Faz 3 (Raporlar) grupları ilgili görünümler eklendiğinde buraya girer.
const navGroups: readonly NavGroup[] = [
  {
    // Genel Bakış tek başına durur; bir gruba girmesi onu gömüyordu.
    key: 'overview',
    label: '',
    items: [{ to: '/', label: labels.nav.dashboard, icon: 'pi pi-home' }],
  },
  {
    key: 'records',
    label: labels.nav.groupRecords,
    items: [
      { to: '/companies', label: labels.nav.companies, icon: 'pi pi-building' },
      { to: '/students', label: labels.nav.students, icon: 'pi pi-users' },
      { to: '/teachers', label: labels.nav.teachers, icon: 'pi pi-id-card' },
    ],
  },
  {
    key: 'planning',
    label: labels.nav.groupPlanning,
    items: [
      { to: '/availability', label: labels.nav.availability, icon: 'pi pi-calendar' },
      { to: '/teaching-load', label: labels.nav.teachingLoad, icon: 'pi pi-book' },
      { to: '/company-hours', label: labels.nav.companyHours, icon: 'pi pi-clock' },
      { to: '/allocation', label: labels.nav.allocation, icon: 'pi pi-share-alt' },
    ],
  },
  {
    key: 'admin',
    label: labels.nav.groupAdmin,
    items: [
      { to: '/settings', label: labels.nav.settings, icon: 'pi pi-cog' },
      { to: '/term-management', label: labels.nav.termManagement, icon: 'pi pi-calendar-plus' },
      { to: '/import-export', label: labels.nav.importExport, icon: 'pi pi-file-import' },
      { to: '/history', label: labels.nav.history, icon: 'pi pi-history' },
    ],
  },
  {
    key: 'help',
    label: labels.nav.groupHelp,
    items: [
      { to: '/help', label: labels.nav.help, icon: 'pi pi-question-circle' },
      { to: '/about', label: labels.nav.about, icon: 'pi pi-info-circle' },
    ],
  },
]
</script>

<style scoped>
.shell {
  display: flex;
  height: 100vh;
  overflow: hidden;
}

.sidebar {
  width: 16rem;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  border-right: 1px solid var(--p-content-border-color);
  background: var(--p-content-background);
  transition: width 0.2s ease;
}

.sidebar--collapsed {
  width: 3.5rem;
}

.sidebar-header {
  display: flex;
  align-items: center;
  height: 3.5rem;
  padding-inline: 1rem;
  border-bottom: 1px solid var(--p-content-border-color);
}

.brand {
  font-weight: 600;
  font-size: 1.125rem;
  white-space: nowrap;
}

.sidebar-as-of-date {
  padding: 0.75rem 1rem;
  border-bottom: 1px solid var(--p-content-border-color);
}

.sidebar-nav {
  flex: 1;
  overflow-y: auto;
  padding: 0.75rem 0.5rem;
}

.nav-group + .nav-group {
  margin-top: 1rem;
}

.nav-group-label {
  padding: 0.25rem 0.75rem;
  font-size: 0.75rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--p-text-muted-color);
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 0.625rem;
  padding: 0.5rem 0.75rem;
  border-radius: var(--p-content-border-radius);
  color: var(--p-text-color);
  text-decoration: none;
  white-space: nowrap;
}

.nav-item:hover {
  background: var(--p-content-hover-background);
}

/* '/' rotası tüm yolların önekidir; router-link-active kullanılsaydı
   Genel Bakış her sayfada vurgulu kalırdı. */
.nav-item.router-link-exact-active {
  background: var(--p-highlight-background);
  color: var(--p-highlight-color);
  font-weight: 500;
}

.nav-item-label {
  font-size: 0.875rem;
}

.main {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.topbar {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  height: 3.5rem;
  flex-shrink: 0;
  padding-inline: 0.75rem;
  border-bottom: 1px solid var(--p-content-border-color);
  background: var(--p-content-background);
}
.topbar-spacer { flex: 1; }
.topbar-term { display: flex; align-items: center; gap: 0.5rem; margin-left: 0.5rem; }
.topbar-term > i { color: var(--p-text-muted-color); }
.term-select { min-width: 11rem; }

.main > :not(.topbar) {
  flex: 1;
  overflow-y: auto;
}
</style>
