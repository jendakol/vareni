import { apiFetch } from './client'

export interface User {
  id: string
  name: string
  email: string
  dietary_restrictions: string[]
  food_preferences: string[]
}

export interface LoginResponse {
  token: string
  user: User
}

export function login(name: string, password: string) {
  return apiFetch<LoginResponse>('/auth/login', {
    method: 'POST',
    body: JSON.stringify({ name, password }),
  })
}

export function me() {
  return apiFetch<User>('/auth/me')
}

export function listUsers() {
  return apiFetch<User[]>('/auth/users')
}
