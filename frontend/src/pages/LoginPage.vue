<template>
  <div class="min-h-[80vh] flex items-center justify-center">
    <div class="w-full max-w-sm space-y-6">
      <h1 class="text-2xl font-bold text-stone-800 text-center">Přihlášení</h1>
      <div v-if="error" class="bg-red-50 text-red-700 p-3 rounded-lg text-sm">{{ error }}</div>
      <a :href="loginUrl"
        class="block w-full py-3 bg-orange-600 text-white font-medium rounded-lg hover:bg-orange-700 text-lg text-center">
        Přihlásit
      </a>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'

// Whitelist: error codes (backend + frontend) -> fixed messages
const ERRORS: Record<string, string> = {
  idp_unavailable: 'Přihlašovací služba je nedostupná, zkuste to později',
  state_expired: 'Přihlášení vypršelo, zkuste to znovu',
  forbidden_unknown_user: 'Tento účet nemá do aplikace přístup',
  forbidden_role: 'Tento účet nemá do aplikace přístup',
  invalid_token: 'Přihlášení se nepodařilo ověřit, zkuste to znovu',
  login_failed: 'Přihlášení se nezdařilo, zkuste to znovu',
  session_expired: 'Přihlášení vypršelo, přihlaste se znovu',
  server_unavailable: 'Server nedostupný, zkuste to později',
}
const GENERIC_ERROR = 'Přihlášení se nezdařilo'

const route = useRoute()

const error = computed(() => {
  const code = route.query.error
  if (typeof code !== 'string' || !code) return ''
  return Object.prototype.hasOwnProperty.call(ERRORS, code) ? ERRORS[code] : GENERIC_ERROR
})

// Same-origin relative path only
const returnUrl = computed(() => {
  const r = route.query.redirect
  return typeof r === 'string' && r.startsWith('/') && !r.startsWith('//') && !r.startsWith('/login')
    ? r
    : '/recipes'
})

// Plain navigation to the backend; never auto-redirected, so no login loop
const loginUrl = computed(() => `/api/auth/login?return_url=${encodeURIComponent(returnUrl.value)}`)
</script>
