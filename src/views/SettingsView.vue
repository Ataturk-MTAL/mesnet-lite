<template>
  <div class="page">
    <h1 class="page-title">{{ labels.nav.settings }}</h1>

    <Card>
      <template #title>{{ labels.settings.schoolSection }}</template>
      <template #content>
        <div class="grid">
          <div class="field field--wide">
            <label for="school-name">{{ labels.settings.schoolName }}</label>
            <InputText id="school-name" v-model="form.schoolName" />
          </div>

          <div class="field">
            <label for="principal-name">{{ labels.settings.principalName }}</label>
            <InputText
              id="principal-name"
              v-model="form.principalName"
              aria-describedby="principal-name-help"
            />
            <small id="principal-name-help" class="hint">{{ labels.settings.blankPrintsDotted }}</small>
          </div>

          <div class="field">
            <label for="field-name">{{ labels.settings.fieldName }}</label>
            <InputText id="field-name" v-model="form.fieldName" aria-describedby="field-name-help" />
            <small id="field-name-help" class="hint">{{ labels.settings.blankPrintsDotted }}</small>
          </div>

          <div class="field">
            <label for="active-term">{{ labels.settings.activeTerm }}</label>
            <InputText id="active-term" v-model="form.activeTerm" />
          </div>

          <div class="field">
            <label for="institution-type">{{ labels.settings.institutionType }}</label>
            <Select
              id="institution-type"
              v-model="form.institutionType"
              :options="institutionTypeOptions"
              optionLabel="label"
              optionValue="value"
            />
          </div>

          <div class="field field--inline">
            <Checkbox
              inputId="metropolitan"
              v-model="form.isMetropolitanDistrict"
              :binary="true"
            />
            <label for="metropolitan">{{ labels.settings.isMetropolitanDistrict }}</label>
          </div>
        </div>

        <Message severity="info" :closable="false" class="cap-note">
          {{ labels.settings.statutoryCap }}: <strong>{{ statutoryCap }}</strong>
          {{ labels.settings.hoursPerWeek }} — {{ statutoryCapSource }}
        </Message>
      </template>
    </Card>

    <Card>
      <template #title>{{ labels.map.schoolLocation }}</template>
      <template #content>
        <LocationPickerMap v-model="schoolLocation" />
        <Message severity="secondary" :closable="false" class="cap-note">
          {{ labels.settings.schoolLocationNote }}
        </Message>
      </template>
    </Card>

    <Card>
      <template #title>{{ labels.settings.dayRangeSection }}</template>
      <template #content>
        <div class="grid">
          <div class="field">
            <label for="max-daily-lessons">{{ labels.settings.maxDailyLessons }}</label>
            <InputNumber
              input-id="max-daily-lessons"
              v-model="form.maxDailyLessons"
              :min="MIN_DAILY_LESSONS"
              :max="MAX_DAILY_LESSONS"
              fluid
              :aria-label="labels.settings.maxDailyLessons"
            />
            <small class="hint">{{ labels.settings.maxDailyLessonsHint }}</small>
          </div>
        </div>
      </template>
    </Card>

    <div class="actions">
      <Button :label="labels.common.save" icon="pi pi-check" :loading="isSaving" @click="save" />
    </div>

    <Card>
      <template #title>{{ labels.settings.users.title }}</template>
      <template #content>
        <div class="users-header">
          <Button :label="labels.settings.users.add" icon="pi pi-plus" @click="isCreateOpen = true" />
        </div>

        <DataTable :value="users" :loading="isLoadingUsers" dataKey="id" stripedRows>
          <template #empty>{{ labels.settings.users.empty }}</template>

          <Column field="name" :header="labels.settings.users.name" />

          <Column :header="labels.settings.users.status">
            <template #body="{ data }">
              <Tag
                :value="data.isActive ? labels.settings.users.active : labels.settings.users.inactive"
                :severity="data.isActive ? 'success' : 'secondary'"
              />
            </template>
          </Column>

          <Column :header="labels.settings.users.actions">
            <template #body="{ data }">
              <div class="row-actions">
                <Button
                  icon="pi pi-pencil"
                  severity="secondary"
                  outlined
                  size="small"
                  :aria-label="labels.settings.users.rename"
                  v-tooltip.top="labels.settings.users.rename"
                  @click="openRename(data)"
                />
                <Button
                  icon="pi pi-key"
                  severity="secondary"
                  outlined
                  size="small"
                  :aria-label="labels.settings.users.changePin"
                  v-tooltip.top="labels.settings.users.changePin"
                  @click="openPinChange(data)"
                />
                <Button
                  :icon="data.isActive ? 'pi pi-eye-slash' : 'pi pi-eye'"
                  :severity="data.isActive ? 'danger' : 'success'"
                  outlined
                  size="small"
                  :disabled="isSelf(data)"
                  :aria-label="toggleActiveLabel(data)"
                  v-tooltip.top="toggleActiveLabel(data)"
                  @click="toggleActive(data)"
                />
              </div>
            </template>
          </Column>
        </DataTable>
      </template>
    </Card>

    <Card>
      <template #title>{{ labels.settings.backup.title }}</template>
      <template #content>
        <p class="backup-description">{{ labels.settings.backup.description }}</p>
        <p class="backup-status">{{ backupStatusText }}</p>

        <div class="backup-actions">
          <Button
            :label="labels.settings.backup.createBackup"
            icon="pi pi-save"
            :loading="isCreatingBackup"
            :disabled="isCreatingBackup || isRestoringBackup"
            @click="createBackup"
          />
          <Button
            :label="labels.settings.backup.openFolder"
            icon="pi pi-folder-open"
            severity="secondary"
            outlined
            :disabled="isCreatingBackup || isRestoringBackup"
            @click="openBackupFolder"
          />
          <Button
            :label="labels.settings.backup.restore"
            icon="pi pi-history"
            severity="danger"
            outlined
            :loading="isRestoringBackup"
            :disabled="isCreatingBackup || isRestoringBackup"
            @click="restoreBackup"
          />
        </div>
      </template>
    </Card>

    <UserCreateDialog v-model:visible="isCreateOpen" :saving="isCreatingUser" @save="handleCreateUser" />
    <UserRenameDialog
      v-model:visible="isRenameOpen"
      :user="selectedUser"
      :saving="isRenamingUser"
      @save="handleRenameUser"
    />
    <UserPinDialog v-model:visible="isPinOpen" :saving="isChangingPin" @save="handlePinChange" />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { useToast } from 'openvue/usetoast'
import { useConfirm } from 'openvue/useconfirm'
import { open as openFileDialog, save as saveFileDialog } from '@tauri-apps/plugin-dialog'
import { openPath } from '@tauri-apps/plugin-opener'
import LocationPickerMap from '../components/map/LocationPickerMap.vue'
import UserCreateDialog from '../components/user/UserCreateDialog.vue'
import UserRenameDialog from '../components/user/UserRenameDialog.vue'
import UserPinDialog from '../components/user/UserPinDialog.vue'
import { settingsApi } from '../api/settings'
import type { SettingsMap } from '../api/settings'
import { usersApi } from '../api/users'
import { backupApi } from '../api/backup'
import { useAuth } from '../composables/useAuth'
import { labels } from '../i18n/labels'
import { dateToIso, isoToDate } from '../utils/isoDate'
import type { BackupStatus, LatLng, User } from '../types/models'

const toast = useToast()
const confirm = useConfirm()
const { currentUser, refreshCurrentUser } = useAuth()

/** Günlük ders saati sayısının alt sınırı. Izgara ders numarası artık her zaman 1'den başlar. */
const MIN_DAILY_LESSONS = 1
/** Ders numarası 1'den başladığı için günün 24 saatini aşmayacak azami sayı: 24 - 1 = 23. */
const HOURS_IN_DAY = 24
const GRID_START_LESSON = 1
const MAX_DAILY_LESSONS = HOURS_IN_DAY - GRID_START_LESSON
/** Ayar anahtarı bulunmayan kurulumlarda bugünkü varsayılan: 9 ders saati. */
const DEFAULT_DAILY_LESSONS = 9

const form = reactive({
  schoolName: '',
  principalName: '',
  fieldName: '',
  activeTerm: '',
  institutionType: 'other',
  isMetropolitanDistrict: true,
  maxDailyLessons: DEFAULT_DAILY_LESSONS as number | null,
})

const schoolLocation = ref<LatLng | null>(null)
const isSaving = ref(false)

const institutionTypeOptions = [
  { value: 'other', label: labels.institutionType.other },
  { value: 'vocational_center', label: labels.institutionType.vocational_center },
]

/**
 * MADDE 15/2 haftalık koordinatörlük tavanı.
 * a) Meslekî eğitim merkezlerinde: büyükşehir ilçesi 24, diğer 18
 * b) Diğer okul ve kurumlarda: büyükşehir ilçesi 20, diğer 16
 */
const statutoryCap = computed<number>(() => {
  const isCenter = form.institutionType === 'vocational_center'
  if (isCenter) return form.isMetropolitanDistrict ? 24 : 18
  return form.isMetropolitanDistrict ? 20 : 16
})

const statutoryCapSource = computed<string>(() => {
  const isCenter = form.institutionType === 'vocational_center'
  if (isCenter) return form.isMetropolitanDistrict ? 'MADDE 15/2-a-1' : 'MADDE 15/2-a-2'
  return form.isMetropolitanDistrict ? 'MADDE 15/2-b-1' : 'MADDE 15/2-b-2'
})

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 6000 })
}

function parseLocation(settings: SettingsMap): LatLng | null {
  const latitude = Number.parseFloat(settings.school_latitude ?? '')
  const longitude = Number.parseFloat(settings.school_longitude ?? '')
  if (Number.isNaN(latitude) || Number.isNaN(longitude)) return null
  return { latitude, longitude }
}

function parsePositiveInt(value: string | undefined): number | null {
  const parsed = Number.parseInt(value ?? '', 10)
  return Number.isNaN(parsed) || parsed < MIN_DAILY_LESSONS ? null : parsed
}

/**
 * Günlük azami ders saati: önce `max_daily_lessons`; yoksa (eski kurulum, göç öncesi) eski
 * `day_end_hour - day_start_hour` çifti; ikisi de yoksa bugünkü varsayılan. Rust tarafı artık
 * mevcut veriyi göçle 1..N numaralandırmasına taşıyor; bu geri dönüş yalnız göç öncesi veya
 * eksik ayar durumunda devreye girer.
 */
function resolveMaxDailyLessons(settings: SettingsMap): number {
  const stored = parsePositiveInt(settings.max_daily_lessons)
  if (stored !== null) return stored
  const startHour = Number.parseInt(settings.day_start_hour ?? '', 10)
  const endHour = Number.parseInt(settings.day_end_hour ?? '', 10)
  const derived = endHour - startHour
  return Number.isNaN(derived) || derived < MIN_DAILY_LESSONS ? DEFAULT_DAILY_LESSONS : derived
}

function applySettings(settings: SettingsMap): void {
  form.schoolName = settings.school_name ?? ''
  form.principalName = settings.principal_name ?? ''
  form.fieldName = settings.field_name ?? ''
  form.activeTerm = settings.active_term ?? ''
  form.institutionType = settings.institution_type ?? 'other'
  form.isMetropolitanDistrict = settings.is_metropolitan_district === 'true'
  form.maxDailyLessons = resolveMaxDailyLessons(settings)
  schoolLocation.value = parseLocation(settings)
}

async function load(): Promise<void> {
  try {
    applySettings(await settingsApi.get())
  } catch (error: unknown) {
    showError(error)
  }
}

// ---------------------------------------------------------------------------
// Kullanıcılar — PIN'li giriş, kayıt tutma amaçlı. Rol/izin YOK; herkes tam
// yetkili. Arka uç dört kuralı (PIN biçimi, benzersiz ad, pasif giremez, son
// etkin kullanıcı pasife alınamaz) AppError::Validation ile Türkçe döner; bu
// ekran yalnız `showError` ile gösterir, kuralı yeniden yazmaz.
// ---------------------------------------------------------------------------

const users = ref<User[]>([])
const isLoadingUsers = ref(false)
const isCreateOpen = ref(false)
const isRenameOpen = ref(false)
const isPinOpen = ref(false)
const selectedUser = ref<User | null>(null)
const isCreatingUser = ref(false)
const isRenamingUser = ref(false)
const isChangingPin = ref(false)

async function loadUsers(): Promise<void> {
  isLoadingUsers.value = true
  try {
    users.value = await usersApi.list()
    // Oturumdaki kullanıcı listede yeniden adlandırılmış olabilir; ekran eski
    // adı göstermesin diye bellekteki oturum burada tazelenir.
    refreshCurrentUser(users.value)
  } catch (error: unknown) {
    showError(error)
  } finally {
    isLoadingUsers.value = false
  }
}

function isSelf(user: User): boolean {
  return currentUser.value?.id === user.id
}

function toggleActiveLabel(user: User): string {
  if (isSelf(user)) return labels.auth.cannotDeactivateSelf
  return user.isActive ? labels.settings.users.deactivate : labels.settings.users.activate
}

/**
 * Diyalog `save` olayında KENDİNİ KAPATMAZ: arka uç reddederse (ör. aynı ad,
 * PIN biçimi) girilen değerler kaybolmasın diye diyalog açık, form dolu
 * kalır ve hata toast'u gösterilir. Yalnız başarıda ilgili `is*Open` false
 * yapılır.
 */
async function handleCreateUser(name: string, pin: string): Promise<void> {
  isCreatingUser.value = true
  try {
    await usersApi.create(name, pin)
    toast.add({ severity: 'success', summary: labels.common.saved, life: 2500 })
    isCreateOpen.value = false
    await loadUsers()
  } catch (error: unknown) {
    showError(error)
  } finally {
    isCreatingUser.value = false
  }
}

function openRename(user: User): void {
  selectedUser.value = user
  isRenameOpen.value = true
}

async function handleRenameUser(name: string): Promise<void> {
  if (!selectedUser.value) return
  isRenamingUser.value = true
  try {
    await usersApi.rename(selectedUser.value.id, name)
    toast.add({ severity: 'success', summary: labels.common.saved, life: 2500 })
    isRenameOpen.value = false
    await loadUsers()
  } catch (error: unknown) {
    showError(error)
  } finally {
    isRenamingUser.value = false
  }
}

function openPinChange(user: User): void {
  selectedUser.value = user
  isPinOpen.value = true
}

async function handlePinChange(pin: string): Promise<void> {
  if (!selectedUser.value) return
  isChangingPin.value = true
  try {
    await usersApi.setPin(selectedUser.value.id, pin)
    toast.add({ severity: 'success', summary: labels.common.saved, life: 2500 })
    isPinOpen.value = false
    await loadUsers()
  } catch (error: unknown) {
    showError(error)
  } finally {
    isChangingPin.value = false
  }
}

async function toggleActive(user: User): Promise<void> {
  if (isSelf(user)) return
  try {
    await usersApi.setActive(user.id, !user.isActive)
    await loadUsers()
  } catch (error: unknown) {
    // Ör. son etkin kullanıcı pasife alınamaz — arka ucun Türkçe mesajı olduğu gibi gösterilir.
    showError(error)
  }
}

async function save(): Promise<void> {
  const lessons = form.maxDailyLessons
  if (lessons === null || lessons < MIN_DAILY_LESSONS || lessons > MAX_DAILY_LESSONS) {
    toast.add({
      severity: 'warn',
      summary: labels.common.error,
      detail: labels.settings.maxDailyLessonsInvalid,
      life: 5000,
    })
    return
  }

  isSaving.value = true
  try {
    const entries: SettingsMap = {
      school_name: form.schoolName,
      principal_name: form.principalName,
      field_name: form.fieldName,
      active_term: form.activeTerm,
      institution_type: form.institutionType,
      is_metropolitan_district: String(form.isMetropolitanDistrict),
      // Gün başlangıç ve bitiş saatleri artık burada yazılmıyor; aralığı Rust tarafı
      // `max_daily_lessons`'tan türetiyor (ders numarası her zaman 1'den başlar).
      max_daily_lessons: String(lessons),
      school_latitude: schoolLocation.value ? String(schoolLocation.value.latitude) : '',
      school_longitude: schoolLocation.value ? String(schoolLocation.value.longitude) : '',
    }
    applySettings(await settingsApi.save(entries))
    toast.add({ severity: 'success', summary: labels.common.saved, life: 2500 })
  } catch (error: unknown) {
    showError(error)
  } finally {
    isSaving.value = false
  }
}

// ---------------------------------------------------------------------------
// Yedekleme — uygulama her açılışta günlük otomatik yedek alır. Bu bölüm
// yalnızca durumu gösterir ve elle yedek alma/geri yükleme akışlarını
// tetikler; zamanlama ve dosya işlemleri Rust tarafındadır.
// ---------------------------------------------------------------------------

const backupStatus = ref<BackupStatus | null>(null)
const isCreatingBackup = ref(false)
const isRestoringBackup = ref(false)

const backupStatusText = computed<string>(() => {
  const status = backupStatus.value
  if (!status || !status.lastBackupAt) return labels.settings.backup.noBackupYet
  const parsed = isoToDate(status.lastBackupAt)
  const formatted = parsed ? parsed.toLocaleDateString('tr-TR') : status.lastBackupAt
  return labels.settings.backup.status(formatted, status.backupCount)
})

async function loadBackupStatus(): Promise<void> {
  try {
    backupStatus.value = await backupApi.status()
  } catch (error: unknown) {
    showError(error)
  }
}

async function createBackup(): Promise<void> {
  const today = dateToIso(new Date()) ?? ''
  let target: string | null
  try {
    target = await saveFileDialog({
      defaultPath: labels.settings.backup.defaultFileName(today),
      filters: [{ name: labels.settings.backup.fileFilterName, extensions: ['db'] }],
    })
  } catch (error: unknown) {
    showError(error)
    return
  }
  if (!target) return

  isCreatingBackup.value = true
  try {
    await backupApi.create(target)
    toast.add({ severity: 'success', summary: labels.settings.backup.createBackupSaved, life: 2500 })
    await loadBackupStatus()
  } catch (error: unknown) {
    showError(error)
  } finally {
    isCreatingBackup.value = false
  }
}

async function openBackupFolder(): Promise<void> {
  if (!backupStatus.value) return
  try {
    await openPath(backupStatus.value.backupDir)
  } catch (error: unknown) {
    showError(error)
  }
}

/** Onaylanan geri yükleme isteğini yürütür; başarıda uygulama kendiliğinden yeniden başlar. */
async function performRestore(path: string): Promise<void> {
  isRestoringBackup.value = true
  try {
    await backupApi.restore(path)
  } catch (error: unknown) {
    showError(error)
  } finally {
    isRestoringBackup.value = false
  }
}

async function restoreBackup(): Promise<void> {
  let target: string | string[] | null
  try {
    target = await openFileDialog({
      defaultPath: backupStatus.value?.backupDir,
      filters: [{ name: labels.settings.backup.fileFilterName, extensions: ['db'] }],
    })
  } catch (error: unknown) {
    showError(error)
    return
  }
  if (!target || Array.isArray(target)) return

  const path = target
  confirm.require({
    message: labels.settings.backup.restoreConfirmMessage,
    header: labels.settings.backup.restoreConfirmHeader,
    acceptLabel: labels.common.yes,
    rejectLabel: labels.common.no,
    acceptProps: { severity: 'danger' },
    accept: () => performRestore(path),
  })
}

onMounted(async () => {
  await load()
  await loadUsers()
  await loadBackupStatus()
})
</script>

<style scoped>
.page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
.page-title { font-size: 1.5rem; font-weight: 600; margin: 0; }
.grid { display: flex; flex-wrap: wrap; gap: 1rem; }
.field { display: flex; flex-direction: column; gap: 0.375rem; flex: 1 1 16rem; max-width: 26rem; min-width: 0; }
/* Okul adı uzun olabilir; diğer alanlardan geniş tutulur ki kırpılmasın. */
.field--wide { flex-basis: 24rem; max-width: 40rem; }
.field--inline { flex: 0 0 auto; flex-direction: row; align-items: center; gap: 0.5rem; }
label { font-size: 0.875rem; font-weight: 500; }
.hint { color: var(--p-text-muted-color); font-size: 0.75rem; }
.cap-note { margin-top: 1rem; }
.actions { display: flex; justify-content: flex-end; }
.users-header { display: flex; justify-content: flex-end; margin-bottom: 0.75rem; }
.row-actions { display: flex; gap: 0.25rem; }
.backup-description { margin: 0 0 0.5rem; color: var(--p-text-muted-color); font-size: 0.875rem; }
.backup-status { margin: 0 0 1rem; font-weight: 500; }
.backup-actions { display: flex; flex-wrap: wrap; gap: 0.5rem; }
</style>
