import { apiFetch } from './client'

export interface MealPlanEntry {
  id: string
  user_id: string | null
  user_name: string | null
  date: string
  meal_type: string
  recipe_id: string | null
  free_text: string | null
  servings: number | null
  status: string
  entry_type: string
  suggested_by_ai: boolean
  note: string | null
  recipe_title: string | null
}

export function listPlan(from: string, to: string) {
  return apiFetch<MealPlanEntry[]>(`/plan?from=${from}&to=${to}`)
}

export function createPlanEntry(data: any) {
  return apiFetch<MealPlanEntry>('/plan', { method: 'POST', body: JSON.stringify(data) })
}

export function updatePlanEntry(id: string, data: any) {
  return apiFetch<MealPlanEntry>(`/plan/${id}`, { method: 'PUT', body: JSON.stringify(data) })
}

export function deletePlanEntry(id: string) {
  return apiFetch<void>(`/plan/${id}`, { method: 'DELETE' })
}

export function suggestFreeText(q: string, limit = 10) {
  const params = new URLSearchParams({ q, limit: String(limit) })
  return apiFetch<string[]>(`/plan/suggest_free_text?${params}`)
}

export function suggestPlan(prompt: string, planningFor: 'both' | 'me' = 'both') {
  return apiFetch<any[]>('/plan/suggest', {
    method: 'POST',
    body: JSON.stringify({ prompt, planning_for: planningFor }),
  })
}
