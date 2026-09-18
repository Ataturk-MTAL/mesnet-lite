<template>
  <SidebarLayout>
    <SidebarBackdrop v-if="isNarrow && open" />

    <Sidebar
      id="app-sidebar"
      variant="sidebar"
      side="left"
      :collapsible="isNarrow ? 'offcanvas' : 'icon'"
      :overlay="isNarrow"
      width="16rem"
      iconWidth="3rem"
      v-model:open="open"
    >
      <SidebarAside>
        <SidebarPanel>
          <SidebarHeader>
            <span class="brand">{{ labels.app.title }}</span>
          </SidebarHeader>

          <SidebarContent>
            <SidebarGroup v-for="group in navGroups" :key="group.label">
              <SidebarGroupLabel>{{ group.label }}</SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  <SidebarMenuItem v-for="item in group.items" :key="item.to">
                    <SidebarMenuButton as-child>
                      <RouterLink :to="item.to">{{ item.label }}</RouterLink>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          </SidebarContent>
        </SidebarPanel>
      </SidebarAside>
    </Sidebar>

    <SidebarMain>
      <header class="topbar">
        <SidebarTrigger target="app-sidebar" severity="secondary" :text="true" size="small" />
      </header>
      <RouterView />
    </SidebarMain>
  </SidebarLayout>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { labels } from '../../i18n/labels'

const open = ref(true)
const isNarrow = ref(false)

// Dar ekranda sidebar offcanvas + overlay moduna geçer.
const NARROW_BREAKPOINT_PX = 900

function updateWidth(): void {
  isNarrow.value = window.innerWidth < NARROW_BREAKPOINT_PX
}

onMounted(() => {
  updateWidth()
  window.addEventListener('resize', updateWidth)
})

onUnmounted(() => window.removeEventListener('resize', updateWidth))

interface NavItem {
  to: string
  label: string
}

interface NavGroup {
  label: string
  items: readonly NavItem[]
}

// Faz 2 (Planlama) ve Faz 3 (Raporlar) grupları ilgili görünümler eklendiğinde buraya girer.
const navGroups: readonly NavGroup[] = [
  {
    label: labels.nav.groupRecords,
    items: [
      { to: '/companies', label: labels.nav.companies },
      { to: '/students', label: labels.nav.students },
      { to: '/teachers', label: labels.nav.teachers },
    ],
  },
  {
    label: labels.nav.groupAdmin,
    items: [
      { to: '/settings', label: labels.nav.settings },
      { to: '/import-export', label: labels.nav.importExport },
    ],
  },
]
</script>

<style scoped>
.brand { font-weight: 600; font-size: 1.125rem; padding-inline: 0.5rem; }
.topbar {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  height: 3rem;
  padding-inline: 1rem;
  border-bottom: 1px solid var(--p-content-border-color);
}
</style>
