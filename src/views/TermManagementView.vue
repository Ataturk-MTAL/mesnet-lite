<template>
  <div class="page">
    <h1 class="page-title">{{ labels.termManagement.title }}</h1>

    <Message severity="secondary" :closable="false">{{ labels.termManagement.subtitle }}</Message>

    <!-- Aktif dönemin tarihleri onaylanmadıkça varsayılan tarihler kullanılır;
         bu durum ek ders puantajı ve tarihçe kurallarını doğrudan etkiler. -->
    <Message
      v-if="activeTermDates && !activeTermDates.datesConfirmed"
      severity="warn"
      :closable="false"
      data-testid="term-dates-unconfirmed-banner"
    >
      <div class="banner-with-action">
        <span>{{ labels.termManagement.datesUnconfirmedWarning }}</span>
        <Button
          :label="labels.termManagement.editDates"
          size="small"
          severity="warn"
          data-testid="term-dates-warning-edit-button"
          @click="openEditDates(activeTermDates)"
        />
      </div>
    </Message>

    <!-- Yeni oluşturulan dönem hiçbir tabloya veri yazılmadan da "oluşabilir";
         devir yapılmadıysa `get_known_terms` sonucunda görünmez. Kullanıcıya
         sessizce kaybolmasın diye kalıcı bir uyarı + hızlı aksiyon gösterilir. -->
    <Message
      v-if="pendingEmptyTerm"
      severity="warn"
      :closable="true"
      @close="pendingEmptyTerm = null"
    >
      <div class="banner-with-action">
        <span><strong>{{ pendingEmptyTerm }}</strong>: {{ labels.termManagement.createdEmptyWarning }}</span>
        <Button
          :label="labels.termManagement.makeActive"
          size="small"
          severity="warn"
          @click="makeActiveFromWarning"
        />
      </div>
    </Message>

    <Card>
      <template #title>{{ labels.termManagement.knownTermsSection }}</template>
      <template #content>
        <DataTable :value="termDates" :loading="isLoading" dataKey="term" stripedRows>
          <template #empty>{{ labels.termManagement.emptyTermsNote }}</template>

          <Column field="term" :header="labels.term.label" />

          <Column :header="labels.termManagement.active">
            <template #body="{ data }: { data: TermWithDates }">
              <Tag v-if="data.term === activeTerm" :value="labels.termManagement.active" severity="success" />
            </template>
          </Column>

          <Column field="startDate" :header="labels.termManagement.startDate" />
          <Column field="endDate" :header="labels.termManagement.endDate" />

          <Column :header="labels.termManagement.datesConfirmed">
            <template #body="{ data }: { data: TermWithDates }">
              <Tag
                :value="data.datesConfirmed ? labels.termManagement.datesConfirmed : labels.termManagement.datesUnconfirmed"
                :severity="data.datesConfirmed ? 'success' : 'warn'"
              />
            </template>
          </Column>

          <Column>
            <template #body="{ data }: { data: TermWithDates }">
              <Button
                :label="labels.termManagement.editDates"
                size="small"
                severity="secondary"
                outlined
                data-testid="term-dates-edit-button"
                @click="openEditDates(data)"
              />
            </template>
          </Column>

          <Column>
            <template #body="{ data }: { data: TermWithDates }">
              <Button
                v-if="data.term !== activeTerm"
                :label="labels.termManagement.makeActive"
                size="small"
                severity="secondary"
                outlined
                :loading="activatingTerm === data.term"
                @click="makeActive(data.term)"
              />
            </template>
          </Column>
        </DataTable>
      </template>
    </Card>

    <Card>
      <template #title>{{ labels.termManagement.newTermSection }}</template>
      <template #content>
        <div class="grid">
          <div class="field">
            <label for="new-term">{{ labels.termManagement.newTermLabel }}</label>
            <InputText
              id="new-term"
              v-model="newTerm"
              :placeholder="labels.termManagement.newTermPlaceholder"
            />
            <small class="hint">{{ labels.termManagement.formatHint }}</small>
          </div>
        </div>

        <div class="field field--inline">
          <Checkbox inputId="copy-enabled" v-model="copyEnabled" :binary="true" />
          <label for="copy-enabled">{{ labels.termManagement.copyEnable }}</label>
        </div>

        <div v-if="copyEnabled" class="field">
          <label for="copy-source">{{ labels.termManagement.copySourceLabel }}</label>
          <Select
            id="copy-source"
            v-model="copySourceTerm"
            :options="knownTerms"
            :placeholder="labels.termManagement.copySourcePlaceholder"
          />
        </div>

        <Message v-if="formError" severity="error" :closable="false">{{ formError }}</Message>

        <div class="actions">
          <Button
            :label="labels.termManagement.create"
            icon="pi pi-plus"
            :loading="isCreating"
            :disabled="newTerm.trim().length === 0 || formError !== null"
            @click="create"
          />
        </div>
      </template>
    </Card>

    <TermDatesDialog
      v-model:visible="isDatesDialogOpen"
      :term="editingTerm"
      :saving="isSavingDates"
      @save="handleSaveDates"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { storeToRefs } from 'pinia'
import { useToast } from 'openvue/usetoast'
import { termsApi, listTermsWithDates, updateTermDates } from '../api/terms'
import type { CreateTermResult } from '../api/terms'
import TermDatesDialog from '../components/term/TermDatesDialog.vue'
import { labels } from '../i18n/labels'
import { useTermStore } from '../stores/term'
import type { TermWithDates } from '../types/models'

const termStore = useTermStore()
const { activeTerm, activeTermDates } = storeToRefs(termStore)

/** Rust'taki `validate_term_format`in bire bir istemci tarafı karşılığı:
 *  YYYY-YYYY/N, ikinci yıl birincinin bir fazlası, N: 1 veya 2. */
const TERM_FORMAT = /^(\d{4})-(\d{4})\/([12])$/

const toast = useToast()

const knownTerms = ref<string[]>([])
/** "Bilinen Dönemler" tablosunun kaynağı; `terms` satırı olmayan bir dönem
 *  burada görünmez — bu arka uçta çözüldü (her yeni dönem bir satır alır). */
const termDates = ref<TermWithDates[]>([])
const isLoading = ref(false)
const activatingTerm = ref<string | null>(null)

const newTerm = ref('')
const copyEnabled = ref(false)
const copySourceTerm = ref<string | null>(null)
const isCreating = ref(false)
const pendingEmptyTerm = ref<string | null>(null)

const isDatesDialogOpen = ref(false)
const editingTerm = ref<TermWithDates | null>(null)
const isSavingDates = ref(false)

/** Kullanıcı YAZARKEN görsün diye canlı hesaplanır — yalnızca `Oluştur`a
 *  basınca değil. Alan boşken hata gösterilmez. */
const formError = computed<string | null>(() => {
  const term = newTerm.value.trim()
  if (term.length === 0) return null

  const formatError = validateTermFormat(term)
  if (formatError) return formatError

  if (copyEnabled.value && copySourceTerm.value !== null && copySourceTerm.value === term) {
    return labels.termManagement.copySameTermError
  }

  return null
})

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 8000 })
}

function validateTermFormat(term: string): string | null {
  const match = TERM_FORMAT.exec(term)
  if (!match) return labels.termManagement.formatInvalid
  const [, firstYear, secondYear] = match
  if (Number(secondYear) !== Number(firstYear) + 1) return labels.termManagement.formatInvalid
  return null
}

async function load(): Promise<void> {
  isLoading.value = true
  try {
    const [terms, dates] = await Promise.all([termsApi.list(), listTermsWithDates()])
    knownTerms.value = terms
    termDates.value = dates
  } catch (error: unknown) {
    showError(error)
  } finally {
    isLoading.value = false
  }
}

function openEditDates(term: TermWithDates | null): void {
  if (!term) return
  editingTerm.value = term
  isDatesDialogOpen.value = true
}

async function handleSaveDates(payload: { startDate: string; endDate: string; confirmed: boolean }): Promise<void> {
  const term = editingTerm.value
  if (!term) return
  isSavingDates.value = true
  try {
    await updateTermDates({
      term: term.term,
      startDate: payload.startDate,
      endDate: payload.endDate,
      confirm: payload.confirmed,
    })
    toast.add({ severity: 'success', summary: labels.termManagement.datesSaved, life: 2500 })
    isDatesDialogOpen.value = false
    // Tablo VE üst çubuktaki dönem seçicisinin `activeTermDates` önbelleği aynı
    // kaynağa (`list_terms_with_dates`) bakar; ikisi de tazelenmezse Dağıtım
    // ve Saat Ayarları ekranları eski `isPlanning`/tarih değerleriyle kalır.
    await Promise.all([load(), termStore.loadTerms()])
  } catch (error: unknown) {
    showError(error)
  } finally {
    isSavingDates.value = false
  }
}

async function makeActive(term: string): Promise<void> {
  activatingTerm.value = term
  try {
    await termStore.setActiveTerm(term)
    // Aktif yapmak `settings.active_term`'ı yazar; `get_known_terms` bu
    // sütunu da tarar, dolayısıyla önceden HİÇBİR tabloda verisi olmayan bir
    // dönem artık bu ekranın kendi listesinde de görünür hâle gelir.
    await load()
    toast.add({ severity: 'success', summary: labels.termManagement.activated, life: 2500 })
  } catch (error: unknown) {
    showError(error)
  } finally {
    activatingTerm.value = null
  }
}

async function makeActiveFromWarning(): Promise<void> {
  const term = pendingEmptyTerm.value
  if (!term) return
  await makeActive(term)
  pendingEmptyTerm.value = null
}

/** `create_term` sonucunu kullanıcıya açıkça bildirir — özellikle devir
 *  istenmediği ya da kaynak boş olduğu için dönemin HÂLÂ BOŞ kaldığı durum,
 *  aksi hâlde dönem `get_known_terms` sonucunda sessizce kaybolur (bkz.
 *  bu ekranın brief'indeki "ÖNEMLİ TUZAK"). */
function reportCreateResult(result: CreateTermResult): void {
  if (result.alreadyExisted) {
    toast.add({ severity: 'info', summary: labels.termManagement.createdAlreadyExisted, life: 5000 })
    return
  }

  if (result.teachingLoadCopySkipped) {
    toast.add({
      severity: 'warn',
      summary: `${result.term}: ${labels.termManagement.createdCopySkipped}`,
      life: 8000,
    })
    return
  }

  if (result.teachingLoadRowsCopied > 0) {
    toast.add({
      severity: 'success',
      summary: `${result.term}: ${result.teachingLoadRowsCopied} ${labels.termManagement.createdCopied}`,
      life: 6000,
    })
    return
  }

  // Devir hiç istenmedi ya da kaynak dönemde satır yoktu: dönem hiçbir
  // tabloya veri yazılmadan oluştu.
  pendingEmptyTerm.value = result.term
}

async function create(): Promise<void> {
  const term = newTerm.value.trim()
  if (term.length === 0 || formError.value !== null) return

  const source = copyEnabled.value ? copySourceTerm.value : null
  isCreating.value = true
  try {
    const result = await termsApi.create(term, source)
    // Bu ekranın kendi listesi VE üst çubuktaki dönem seçicisinin önbelleği
    // aynı kaynağa (`get_known_terms`) bakar; ikisi de tazelenmezse yeni
    // dönem (satır kopyalandıysa) seçicide görünmez.
    await Promise.all([load(), termStore.loadTerms()])
    reportCreateResult(result)
    newTerm.value = ''
    copyEnabled.value = false
    copySourceTerm.value = null
  } catch (error: unknown) {
    showError(error)
  } finally {
    isCreating.value = false
  }
}

onMounted(async () => {
  // Üst çubuktaki dönem seçicisi de aynı kaynağa bakar; burada tazelemek
  // biraz önce oluşturulan bir dönemin seçicide de görünmesini sağlar.
  await Promise.all([load(), termStore.loadTerms()])
})
</script>

<style scoped>
.page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
.page-title { font-size: 1.5rem; font-weight: 600; margin: 0; }
.grid { display: flex; flex-wrap: wrap; gap: 1rem; }
.field { display: flex; flex-direction: column; gap: 0.375rem; min-width: 16rem; margin-top: 0.75rem; }
.field--inline { flex-direction: row; align-items: center; gap: 0.5rem; }
label { font-size: 0.875rem; font-weight: 500; }
.hint { color: var(--p-text-muted-color); }
.actions { display: flex; justify-content: flex-end; margin-top: 1rem; }
.banner-with-action {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  flex-wrap: wrap;
}
</style>
