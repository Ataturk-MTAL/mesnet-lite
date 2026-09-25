<template>
  <div class="page">
    <h1 class="page-title">{{ labels.termManagement.title }}</h1>

    <Message severity="secondary" :closable="false">{{ labels.termManagement.subtitle }}</Message>

    <!-- Yeni oluşturulan dönem hiçbir tabloya veri yazılmadan da "oluşabilir";
         devir yapılmadıysa `get_known_terms` sonucunda görünmez. Kullanıcıya
         sessizce kaybolmasın diye kalıcı bir uyarı + hızlı aksiyon gösterilir. -->
    <Message
      v-if="pendingEmptyTerm"
      severity="warn"
      :closable="true"
      @close="pendingEmptyTerm = null"
    >
      <div class="empty-term-warning">
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
        <DataTable :value="termRows" :loading="isLoading" dataKey="term" stripedRows>
          <template #empty>{{ labels.termManagement.emptyTermsNote }}</template>

          <Column field="term" :header="labels.term.label" />

          <Column :header="labels.termManagement.active">
            <template #body="{ data }: { data: TermRow }">
              <Tag v-if="data.term === activeTerm" :value="labels.termManagement.active" severity="success" />
            </template>
          </Column>

          <Column>
            <template #body="{ data }: { data: TermRow }">
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
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { storeToRefs } from 'pinia'
import { useToast } from 'openvue/usetoast'
import { termsApi } from '../api/terms'
import type { CreateTermResult } from '../api/terms'
import { labels } from '../i18n/labels'
import { useTermStore } from '../stores/term'

const termStore = useTermStore()
const { activeTerm } = storeToRefs(termStore)

interface TermRow {
  term: string
}

/** Rust'taki `validate_term_format`in bire bir istemci tarafı karşılığı:
 *  YYYY-YYYY/N, ikinci yıl birincinin bir fazlası, N: 1 veya 2. */
const TERM_FORMAT = /^(\d{4})-(\d{4})\/([12])$/

const toast = useToast()

const knownTerms = ref<string[]>([])
const isLoading = ref(false)
const activatingTerm = ref<string | null>(null)

const newTerm = ref('')
const copyEnabled = ref(false)
const copySourceTerm = ref<string | null>(null)
const isCreating = ref(false)
const pendingEmptyTerm = ref<string | null>(null)

const termRows = computed<TermRow[]>(() => knownTerms.value.map((term) => ({ term })))

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
    knownTerms.value = await termsApi.list()
  } catch (error: unknown) {
    showError(error)
  } finally {
    isLoading.value = false
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
.empty-term-warning {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  flex-wrap: wrap;
}
</style>
