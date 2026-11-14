import { apiFetch } from './client'

export interface User {
  id: string
  name: string
  email: string
  csrf_token?: string
  dietary_restrictions?: string[]
  food_preferences?: string[]
}

// No redirect on 401: callers decide (router guard, login page)
export function me() {
  return apiFetch<User>('/auth/me', {}, false)
}

export function listUsers() {
  return apiFetch<User[]>('/auth/users')
}
