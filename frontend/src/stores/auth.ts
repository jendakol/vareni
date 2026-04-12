import { defineStore } from 'pinia'
import { ref } from 'vue'
import * as authApi from '../api/auth'

export const useAuthStore = defineStore('auth', () => {
  const token = ref(localStorage.getItem('token') || '')
  const user = ref<authApi.User | null>(null)

  async function login(name: string, password: string) {
    const resp = await authApi.login(name, password)
    token.value = resp.token
    user.value = resp.user
    localStorage.setItem('token', resp.token)
  }

  async function fetchMe() {
    user.value = await authApi.me()
  }

  function logout() {
    token.value = ''
    user.value = null
    localStorage.removeItem('token')
  }

  return { token, user, login, fetchMe, logout }
})
