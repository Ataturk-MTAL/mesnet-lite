import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', name: 'dashboard', component: () => import('../views/DashboardView.vue') },
    { path: '/companies', name: 'companies', component: () => import('../views/CompaniesView.vue') },
    { path: '/students', name: 'students', component: () => import('../views/StudentsView.vue') },
    { path: '/teachers', name: 'teachers', component: () => import('../views/TeachersView.vue') },
    { path: '/settings', name: 'settings', component: () => import('../views/SettingsView.vue') },
    { path: '/import-export', name: 'importExport', component: () => import('../views/ImportExportView.vue') },
  ],
})

export default router
