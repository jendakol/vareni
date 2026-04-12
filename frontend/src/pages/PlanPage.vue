<template>
  <div>
    <div class="flex items-center justify-between mb-6">
      <h1 class="text-2xl font-bold text-stone-800">Plan</h1>
      <div class="flex gap-2">
        <button @click="shiftWeek(-1)" class="px-3 py-1 border rounded-lg">←</button>
        <span class="px-3 py-1 text-stone-600">{{ weekLabel }}</span>
        <button @click="shiftWeek(1)" class="px-3 py-1 border rounded-lg">→</button>
      </div>
    </div>

    <!-- Suggest -->
    <div class="mb-6 flex gap-2">
      <input v-model="suggestPrompt" placeholder="Např. Návrh jídla na tento týden..."
        class="flex-1 px-4 py-2 border border-stone-300 rounded-lg" />
      <button @click="handleSuggest" :disabled="suggesting"
        class="px-4 py-2 bg-orange-600 text-white rounded-lg hover:bg-orange-700 disabled:opacity-50">
        Navrhnout
      </button>
    </div>

    <!-- Calendar grid -->
    <div class="space-y-4">
      <div v-for="day in days" :key="day.date" class="bg-white rounded-xl border border-stone-200 p-4">
        <h3 class="font-medium text-stone-700 mb-2">{{ formatDay(day.date) }}</h3>
        <div class="space-y-2">
          <div v-for="entry in day.entries" :key="entry.id"
            class="flex items-center justify-between px-3 py-2 rounded-lg"
            :class="entry.status === 'suggested' ? 'border-2 border-dashed border-orange-300 bg-orange-50' : 'bg-stone-50'">
            <span class="text-stone-800">
              <span class="text-stone-400 text-sm mr-2">{{ entry.meal_type }}</span>
              {{ entry.free_text || 'Recept' }}
            </span>
            <div class="flex gap-1">
              <button v-if="entry.status === 'suggested'" @click="confirmEntry(entry)"
                class="text-green-600 text-sm hover:underline">Potvrdit</button>
              <button @click="removeEntry(entry.id)" class="text-red-400 text-sm hover:underline">✕</button>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import * as planApi from '../api/plan'

const weekOffset = ref(0)
const entries = ref<planApi.MealPlanEntry[]>([])
const suggestions = ref<any[]>([])
const suggestPrompt = ref('')
const suggesting = ref(false)

const startDate = computed(() => {
  const d = new Date()
  d.setDate(d.getDate() - d.getDay() + 1 + weekOffset.value * 7) // Monday
  return d
})

const weekLabel = computed(() => {
  const s = startDate.value
  const e = new Date(s)
  e.setDate(e.getDate() + 6)
  return `${fmt(s)} – ${fmt(e)}`
})

function fmt(d: Date) {
  return d.toISOString().slice(0, 10)
}

const days = computed(() => {
  const result = []
  for (let i = 0; i < 7; i++) {
    const d = new Date(startDate.value)
    d.setDate(d.getDate() + i)
    const date = fmt(d)
    const dayEntries = [
      ...entries.value.filter(e => e.date === date),
      ...suggestions.value.filter(s => s.date === date).map(s => ({ ...s, id: `sug-${s.date}-${s.meal_type}`, status: 'suggested' })),
    ]
    result.push({ date, entries: dayEntries })
  }
  return result
})

function formatDay(date: string) {
  const d = new Date(date)
  return d.toLocaleDateString('cs-CZ', { weekday: 'long', day: 'numeric', month: 'numeric' })
}

function shiftWeek(dir: number) {
  weekOffset.value += dir
  loadEntries()
}

async function loadEntries() {
  const from = fmt(startDate.value)
  const to = fmt(new Date(startDate.value.getTime() + 6 * 86400000))
  entries.value = await planApi.listPlan(from, to)
}

async function handleSuggest() {
  suggesting.value = true
  try {
    suggestions.value = await planApi.suggestPlan(suggestPrompt.value)
  } finally {
    suggesting.value = false
  }
}

async function confirmEntry(entry: any) {
  await planApi.createPlanEntry({
    date: entry.date,
    meal_type: entry.meal_type,
    recipe_id: entry.recipe_id,
    free_text: entry.free_text,
    note: entry.note,
    status: 'confirmed',
  })
  suggestions.value = suggestions.value.filter(s => !(s.date === entry.date && s.meal_type === entry.meal_type))
  await loadEntries()
}

async function removeEntry(id: string) {
  if (id.startsWith('sug-')) {
    suggestions.value = suggestions.value.filter(s => `sug-${s.date}-${s.meal_type}` !== id)
  } else {
    await planApi.deletePlanEntry(id)
    await loadEntries()
  }
}

onMounted(loadEntries)
</script>
