<template>
  <div class="min-h-[80vh] flex items-center justify-center">
    <form @submit.prevent="handleLogin" class="w-full max-w-sm space-y-6">
      <h1 class="text-2xl font-bold text-stone-800 text-center">Přihlášení</h1>
      <div v-if="error" class="bg-red-50 text-red-700 p-3 rounded-lg text-sm">{{ error }}</div>
      <div>
        <label class="block text-sm font-medium text-stone-600 mb-1">Jméno</label>
        <input v-model="name" type="text" required
          class="w-full px-4 py-3 border border-stone-300 rounded-lg focus:ring-2 focus:ring-orange-500 focus:border-orange-500 text-lg" />
      </div>
      <div>
        <label class="block text-sm font-medium text-stone-600 mb-1">Heslo</label>
        <input v-model="password" type="password" required
          class="w-full px-4 py-3 border border-stone-300 rounded-lg focus:ring-2 focus:ring-orange-500 focus:border-orange-500 text-lg" />
      </div>
      <button type="submit" :disabled="loading"
        class="w-full py-3 bg-orange-600 text-white font-medium rounded-lg hover:bg-orange-700 disabled:opacity-50 text-lg">
        {{ loading ? 'Přihlašování...' : 'Přihlásit' }}
      </button>
    </form>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from '../stores/auth'

const authStore = useAuthStore()
const router = useRouter()
const name = ref('')
const password = ref('')
const error = ref('')
const loading = ref(false)

async function handleLogin() {
  loading.value = true
  error.value = ''
  try {
    await authStore.login(name.value, password.value)
    router.push('/recipes')
  } catch (e: any) {
    error.value = 'Špatné jméno nebo heslo'
  } finally {
    loading.value = false
  }
}
</script>
