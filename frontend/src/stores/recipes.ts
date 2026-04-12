import { defineStore } from 'pinia'
import { ref } from 'vue'
import * as api from '../api/recipes'

export const useRecipeStore = defineStore('recipes', () => {
  const recipes = ref<api.Recipe[]>([])
  const total = ref(0)
  const loading = ref(false)

  async function fetch(params: { q?: string; tag?: string; page?: number } = {}) {
    loading.value = true
    try {
      const result = await api.listRecipes(params)
      recipes.value = result.items
      total.value = result.total
    } finally {
      loading.value = false
    }
  }

  return { recipes, total, loading, fetch }
})
