import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { useToast } from 'vue-toastification'
import * as api from '../api/recipes'

export type ListParams = { q?: string; tag?: string; sort?: string; status?: string }

/**
 * Cap on how many server pages fetchMore will walk past a run of
 * already-loaded duplicates before giving up — guards against a pathological
 * reorder spinning forever instead of just ending the list early.
 */
const MAX_FETCH_MORE_ATTEMPTS = 5

export const useRecipeStore = defineStore('recipes', () => {
  const toast = useToast()
  const recipes = ref<api.Recipe[]>([])
  const total = ref(0)
  const loading = ref(false)
  const loadingMore = ref(false)
  const error = ref<string | null>(null)
  /** Separate from `error`: a failed fetchMore() must not resurrect a stale
   *  "load failed" banner under an already-good list on the next activation —
   *  that banner belongs to fetch(), this one only to fetchMore(). */
  const moreError = ref<string | null>(null)
  /** Page size from the last response — the backend owns the default, never hardcode it. */
  const perPage = ref(20)
  /** Set once a page comes back short — the authoritative end-of-list signal. */
  const exhausted = ref(false)
  /**
   * Creating a recipe is the one mutation that cannot be patched in: its
   * position under the current sort is unknown. The list refetches on its next
   * activation instead.
   */
  const stale = ref(false)

  let lastParams: ListParams = {}
  /**
   * Bumped by every fetch(). fetchMore() captures it before its request and
   * discards a response that arrives after a newer fetch() started — otherwise
   * a fetchMore in flight when the user switches tabs would push the old tab's
   * items onto the new tab's list.
   */
  let generation = 0

  const hasMore = computed(() => !exhausted.value && recipes.value.length < total.value)

  async function fetch(params: ListParams = {}) {
    const gen = ++generation
    lastParams = params
    stale.value = false
    loading.value = true
    error.value = null
    // Eager reset: this fetch is by definition the newest generation the
    // instant it's called, so nothing async has had a chance to race it yet —
    // safe to blank the old tab's data unconditionally rather than only on
    // success. Without this, a failing fetch left the previous tab's
    // recipes/total/exhausted in place under the new tab, invisible because
    // `loading` was already covering the screen, then wrong forever because
    // `error` alone can't be trusted once `recipes.length > 0` hides it.
    recipes.value = []
    total.value = 0
    exhausted.value = false
    moreError.value = null
    try {
      const result = await api.listRecipes({ ...params, page: 1 })
      if (gen !== generation) return
      recipes.value = result.items
      total.value = result.total
      perPage.value = result.per_page
      exhausted.value = result.items.length < result.per_page
    } catch (e: any) {
      if (gen === generation) {
        error.value = e.message || 'Nepodařilo se načíst recepty'
        toast.error(error.value!)
      }
    } finally {
      if (gen === generation) loading.value = false
    }
  }

  /**
   * Paging is OFFSET-based against the server's *current* filtered/ordered
   * set, which shifts under local mutations: removing or reordering a row
   * server-side (e.g. a status change) changes what OFFSET N means. So the
   * next request is derived from how many items are actually loaded — always
   * the true server offset — never from an incrementing page counter, which
   * drifts the moment a row moves and silently drops rows for good.
   */
  async function fetchMore() {
    if (loading.value || loadingMore.value || !hasMore.value) return
    const gen = generation
    loadingMore.value = true
    moreError.value = null
    try {
      const offset = recipes.value.length
      let targetPage = Math.floor(offset / perPage.value) + 1
      let skip = offset % perPage.value

      for (let attempt = 0; attempt < MAX_FETCH_MORE_ATTEMPTS; attempt++) {
        const result = await api.listRecipes({ ...lastParams, page: targetPage })
        if (gen !== generation) return

        total.value = result.total
        // Drop the leading items of a misaligned page we already have, then
        // drop anything that shifted back into view after a reorder.
        const seen = new Set(recipes.value.map(r => r.id))
        const fresh = result.items.slice(skip).filter(r => !seen.has(r.id))
        recipes.value.push(...fresh)

        // `total` comes from a separate COUNT(*) and drifts as items are
        // patched locally, so a short page — not the count — decides when we
        // are done.
        if (result.items.length < result.per_page) {
          exhausted.value = true
          return
        }
        if (fresh.length > 0) return

        // Full-size page but nothing new: every item on it was a duplicate
        // that shifted back into range after a reorder. There is still more
        // data — walk to the next server page instead of re-requesting this one.
        targetPage++
        skip = 0
      }
      // Hit the bound: stop rather than spin forever on a pathological reshuffle.
      // This truncates the list silently from the user's point of view, so at
      // least surface it in the console — the same class of failure Fix 1/2
      // exist to prevent.
      console.warn(
        `fetchMore: gave up after ${MAX_FETCH_MORE_ATTEMPTS} attempts with no new items; list may be incomplete`,
      )
      exhausted.value = true
    } catch (e: any) {
      if (gen === generation) moreError.value = e.message || 'Nepodařilo se načíst další recepty'
    } finally {
      loadingMore.value = false
    }
  }

  /**
   * Update one cached item in place, so the scroll position survives. A miss
   * (the mutated recipe was never in the loaded window — e.g. reached via a
   * deep link straight to its detail page) is not a no-op: the server-side
   * mutation may still have reordered or refiltered the set this list is
   * paging through, and offset-based paging assumes the loaded prefix matches
   * the server's current ordering. Mark stale rather than leave that
   * assumption silently violated.
   */
  function patchRecipe(id: string, patch: Partial<api.Recipe>) {
    const i = recipes.value.findIndex(r => r.id === id)
    if (i !== -1) recipes.value[i] = { ...recipes.value[i], ...patch }
    else markStale()
  }

  /** Same reasoning as patchRecipe: a miss can still mean the server-side set changed. */
  function removeRecipe(id: string) {
    const i = recipes.value.findIndex(r => r.id === id)
    if (i === -1) {
      markStale()
      return
    }
    recipes.value.splice(i, 1)
    total.value = Math.max(0, total.value - 1)
  }

  function markStale() {
    stale.value = true
  }

  return {
    recipes, total, loading, loadingMore, error, moreError, stale, hasMore,
    fetch, fetchMore, patchRecipe, removeRecipe, markStale,
  }
})
