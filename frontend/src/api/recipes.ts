import { apiFetch } from './client'

export interface Recipe {
  id: string
  title: string
  description: string | null
  servings: number | null
  prep_time_min: number | null
  cook_time_min: number | null
  emoji: string | null
  source_type: string | null
  source_url: string | null
  tags?: string[]
  ingredients?: Ingredient[]
  steps?: Step[]
  is_public: boolean
  public_slug: string | null
  status: string
  discovery_score: number | null
  discovered_at: string | null
  scored_at: string | null
  canonical_name: string | null
}

export interface Ingredient {
  id: string
  name: string
  amount: number | null
  unit: string | null
  note: string | null
  sort_order: number
}

export interface Step {
  step_order: number
  instruction: string
}

export interface Paginated<T> {
  items: T[]
  total: number
  page: number
  per_page: number
}

export function listRecipes(params: { q?: string; tag?: string; page?: number; sort?: string; status?: string } = {}) {
  const search = new URLSearchParams()
  if (params.q) search.set('q', params.q)
  if (params.tag) search.set('tag', params.tag)
  if (params.page) search.set('page', String(params.page))
  if (params.sort) search.set('sort', params.sort)
  if (params.status) search.set('status', params.status)
  return apiFetch<Paginated<Recipe>>(`/recipes?${search}`)
}

export function updateRecipeStatus(id: string, status: string): Promise<Recipe> {
  return apiFetch(`/recipes/${id}/status`, {
    method: 'PATCH',
    body: JSON.stringify({ status }),
  })
}

export function getRecipe(id: string) {
  return apiFetch<Recipe>(`/recipes/${id}`)
}

export function createRecipe(data: any) {
  return apiFetch<Recipe>('/recipes', { method: 'POST', body: JSON.stringify(data) })
}

export function updateRecipe(id: string, data: any) {
  return apiFetch<Recipe>(`/recipes/${id}`, { method: 'PUT', body: JSON.stringify(data) })
}

export function deleteRecipe(id: string) {
  return apiFetch<void>(`/recipes/${id}`, { method: 'DELETE' })
}

export function shareRecipe(id: string) {
  return apiFetch<{ share_url: string; slug: string }>(`/recipes/${id}/share`, { method: 'POST' })
}

export function unshareRecipe(id: string) {
  return apiFetch<void>(`/recipes/${id}/share`, { method: 'DELETE' })
}

export function ingest(formData: FormData) {
  return apiFetch<any>('/ingest', { method: 'POST', body: formData })
}

export function getPublicRecipe(slug: string) {
  return apiFetch<Recipe>(`/public/recipes/${slug}`)
}
