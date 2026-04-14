import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useToast } from 'vue-toastification'
import * as api from '../api/recipes'

export const useRecipeStore = defineStore('recipes', () => {
  const toast = useToast()
  const recipes = ref<api.Recipe[]>([])
  const total = ref(0)
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function fetch(params: { q?: string; tag?: string; page?: number; sort?: string; status?: string } = {}) {
    loading.value = true
    error.value = null
    try {
      const result = await api.listRecipes(params)
      recipes.value = result.items
      total.value = result.total
    } catch (e: any) {
      error.value = e.message || 'Nepodařilo se načíst recepty'
      toast.error(error.value!)
    } finally {
      loading.value = false
    }
  }

  return { recipes, total, loading, error, fetch }
})
