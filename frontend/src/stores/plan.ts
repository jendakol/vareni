import { defineStore } from 'pinia'
import { ref } from 'vue'
import * as planApi from '../api/plan'

export const usePlanStore = defineStore('plan', () => {
  const entries = ref<planApi.MealPlanEntry[]>([])
  const loading = ref(false)

  async function fetch(from: string, to: string) {
    loading.value = true
    try {
      entries.value = await planApi.listPlan(from, to)
    } finally {
      loading.value = false
    }
  }

  return { entries, loading, fetch }
})
