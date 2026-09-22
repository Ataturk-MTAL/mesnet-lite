<template>
  <div class="login-screen">
    <Card class="login-card">
      <template #title>{{ labels.app.title }}</template>
      <template #content>
        <div v-if="isLoading" class="login-loading">{{ labels.common.loading }}</div>

        <form v-else-if="mode === 'setup'" class="login-form" @submit.prevent="submitSetup">
          <h2 class="login-subtitle">{{ labels.auth.firstSetupTitle }}</h2>

          <div class="field">
            <label for="setup-name">{{ labels.auth.name }}</label>
            <InputText id="setup-name" v-model="setupForm.name" autofocus fluid :aria-label="labels.auth.name" />
          </div>

          <div class="field">
            <label for="setup-pin">{{ labels.auth.pin }}</label>
            <Password
              input-id="setup-pin"
              v-model="setupPinProxy"
              :feedback="false"
              fluid
              :aria-label="labels.auth.pin"
              :input-props="pinInputProps"
            />
          </div>

          <div class="field">
            <label for="setup-pin-confirm">{{ labels.auth.pinConfirm }}</label>
            <Password
              input-id="setup-pin-confirm"
              v-model="setupPinConfirmProxy"
              :feedback="false"
              fluid
              :aria-label="labels.auth.pinConfirm"
              :input-props="pinInputProps"
            />
          </div>

          <Button
            type="submit"
            :label="labels.auth.createAndSignIn"
            :loading="isSubmitting"
            :disabled="!canSubmitSetup"
            class="login-submit"
          />
        </form>

        <form v-else class="login-form" @submit.prevent="submitLogin">
          <div class="field">
            <label for="login-user">{{ labels.auth.user }}</label>
            <Select
              id="login-user"
              v-model="loginForm.userId"
              :options="activeUsers"
              optionLabel="name"
              optionValue="id"
              :placeholder="labels.auth.userPlaceholder"
              :aria-label="labels.auth.user"
              fluid
            />
          </div>

          <div class="field">
            <label for="login-pin">{{ labels.auth.pin }}</label>
            <Password
              input-id="login-pin"
              ref="pinFieldRef"
              v-model="loginPinProxy"
              :feedback="false"
              fluid
              :aria-label="labels.auth.pin"
              :input-props="pinInputProps"
            />
          </div>

          <Button
            type="submit"
            :label="labels.auth.signIn"
            :loading="isSubmitting"
            :disabled="!canSubmitLogin"
            class="login-submit"
          />
        </form>
      </template>
    </Card>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, reactive, ref } from 'vue'
import { useToast } from 'openvue/usetoast'
import { usersApi } from '../api/users'
import { signIn } from '../composables/useAuth'
import { labels } from '../i18n/labels'
import { sanitizePinInput } from '../utils/pin'
import type { User } from '../types/models'

const toast = useToast()

type Mode = 'setup' | 'login'

const isLoading = ref(true)
const isSubmitting = ref(false)
const mode = ref<Mode>('login')
const users = ref<User[]>([])

const setupForm = reactive({ name: '', pin: '', pinConfirm: '' })
const loginForm = reactive<{ userId: number | null; pin: string }>({ userId: null, pin: '' })

const pinFieldRef = ref<{ $el: HTMLElement } | null>(null)

/** Sayısal tuş takımı ve tarayıcı otomatik doldurmasını kapatmak için; doğrulama içermez. */
const pinInputProps = { inputmode: 'numeric' as const, autocomplete: 'off', maxlength: 6 }

/** Yalnız etkin kullanıcılar seçilebilir; pasif kullanıcı giriş yapamaz (arka uç kuralı). */
const activeUsers = computed(() => users.value.filter((user) => user.isActive))

const canSubmitSetup = computed(() => setupForm.name.trim().length > 0 && setupForm.pin.length > 0)
const canSubmitLogin = computed(() => loginForm.userId !== null && loginForm.pin.length > 0)

const setupPinProxy = computed({
  get: (): string => setupForm.pin,
  set: (value: string | null | undefined): void => {
    setupForm.pin = sanitizePinInput(value)
  },
})

const setupPinConfirmProxy = computed({
  get: (): string => setupForm.pinConfirm,
  set: (value: string | null | undefined): void => {
    setupForm.pinConfirm = sanitizePinInput(value)
  },
})

const loginPinProxy = computed({
  get: (): string => loginForm.pin,
  set: (value: string | null | undefined): void => {
    loginForm.pin = sanitizePinInput(value)
  },
})

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 6000 })
}

function focusPinField(): void {
  pinFieldRef.value?.$el?.querySelector('input')?.focus()
}

async function loadInitialState(): Promise<void> {
  isLoading.value = true
  try {
    const hasAny = await usersApi.hasAny()
    if (!hasAny) {
      mode.value = 'setup'
      return
    }
    mode.value = 'login'
    users.value = await usersApi.list()
  } catch (error: unknown) {
    showError(error)
  } finally {
    isLoading.value = false
  }
}

/** Yanlış PIN'de alan temizlenir, odak PIN'e döner; bu bir hata FIRLATMAZ. */
async function handleWrongPin(): Promise<void> {
  toast.add({ severity: 'error', summary: labels.common.error, detail: labels.auth.wrongPin, life: 5000 })
  loginForm.pin = ''
  await nextTick()
  focusPinField()
}

async function submitLogin(): Promise<void> {
  if (!canSubmitLogin.value || loginForm.userId === null) return
  isSubmitting.value = true
  try {
    const ok = await signIn(loginForm.userId, loginForm.pin)
    if (!ok) await handleWrongPin()
  } catch (error: unknown) {
    showError(error)
  } finally {
    isSubmitting.value = false
  }
}

/**
 * Kullanıcı oluşturulduktan SONRA giriş herhangi bir şekilde başarısız
 * olursa (yanlış sonuç ya da hata) `create` yine de başarılı sayılır: ekran
 * kilitlenip 'setup' modunda kalmaz, kullanıcı listesi yeniden alınır ve
 * yeni oluşturulan kullanıcı seçili, PIN alanı boş biçimde giriş moduna
 * geçilir — aksi halde tekrar deneme aynı adı yeniden oluşturmaya çalışıp
 * "Bu isimde bir kullanıcı zaten var" ile düşer.
 */
async function signInAfterSetup(created: User): Promise<void> {
  try {
    const ok = await signIn(created.id, setupForm.pin)
    if (!ok) throw new Error(labels.auth.wrongPin)
  } catch (error: unknown) {
    showError(error)
    await returnToLoginAfterFailedSetupSignIn(created.id)
  }
}

async function returnToLoginAfterFailedSetupSignIn(createdUserId: number): Promise<void> {
  mode.value = 'login'
  try {
    users.value = await usersApi.list()
  } catch (error: unknown) {
    showError(error)
  }
  loginForm.userId = createdUserId
  loginForm.pin = ''
}

async function submitSetup(): Promise<void> {
  if (!canSubmitSetup.value) return
  if (setupForm.pin !== setupForm.pinConfirm) {
    toast.add({ severity: 'warn', summary: labels.common.error, detail: labels.auth.pinMismatch, life: 5000 })
    return
  }
  isSubmitting.value = true
  try {
    const created = await usersApi.create(setupForm.name.trim(), setupForm.pin)
    await signInAfterSetup(created)
  } catch (error: unknown) {
    // `create` başarısızsa (ör. isim çakışması) ekran 'setup' modunda kalır.
    showError(error)
  } finally {
    isSubmitting.value = false
  }
}

onMounted(loadInitialState)
</script>

<style scoped>
.login-screen {
  height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--p-content-background);
}
.login-card {
  width: 24rem;
  max-width: calc(100vw - 2rem);
}
.login-loading {
  text-align: center;
  color: var(--p-text-muted-color);
}
.login-subtitle {
  font-size: 1rem;
  font-weight: 600;
  margin: 0 0 0.5rem;
}
.login-form {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}
label {
  font-size: 0.875rem;
  font-weight: 500;
}
.login-submit {
  margin-top: 0.25rem;
}
</style>
