import { defineStore } from 'pinia'
import { ref } from 'vue'
import * as authApi from '../api/auth'
import { csrfToken } from '../api/client'

// Legacy bearer token from the password login
try { localStorage.removeItem('token') } catch { /* storage unavailable */ }

export const useAuthStore = defineStore('auth', () => {
  const user = ref<authApi.User | null>(null)

  async function fetchMe() {
    const u = await authApi.me()
    csrfToken.value = u.csrf_token || ''
    user.value = u
  }

  return { user, csrf: csrfToken, fetchMe }
})
