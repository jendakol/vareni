<template>
  <div>
    <div class="flex items-center justify-between mb-6">
      <h1 class="text-2xl font-bold text-stone-800">Plan</h1>
      <div class="flex gap-2 items-center">
        <button @click="shiftPeriod(-1)" class="px-3 py-1 border rounded-lg">&larr;</button>
        <span class="px-3 py-1 text-stone-600">{{ periodLabel }}</span>
        <button @click="shiftPeriod(1)" class="px-3 py-1 border rounded-lg">&rarr;</button>
      </div>
    </div>

    <!-- Day count -->
    <div class="flex gap-2 mb-4 flex-wrap">
      <button v-for="n in dayOptions" :key="n" @click="numDays = n; loadEntries()"
        class="px-3 py-1 rounded-full text-sm"
        :class="numDays === n ? 'bg-orange-600 text-white' : 'bg-stone-100 text-stone-600'">
        {{ n === 7 ? 'Týden' : n === 14 ? '2 týdny' : `${n} dní` }}
      </button>
    </div>

    <!-- Suggest -->
    <div class="mb-6 space-y-2">
      <div class="space-y-2 sm:space-y-0 sm:flex sm:gap-2">
        <input v-model="suggestPrompt" :placeholder="suggestPlaceholder"
          class="w-full sm:flex-1 px-4 py-2 border border-stone-300 rounded-lg" />
        <button @click="handleSuggest" :disabled="suggesting"
          class="w-full sm:w-auto px-4 py-2 bg-orange-600 text-white rounded-lg hover:bg-orange-700 disabled:opacity-50">
          Navrhnout
        </button>
      </div>
      <div class="flex items-center gap-3 flex-wrap">
        <span class="text-sm text-stone-500">Dietní omezení:</span>
        <button @click="planningFor = 'both'"
          class="px-3 py-1 rounded-full text-sm"
          :class="planningFor === 'both' ? 'bg-orange-600 text-white' : 'bg-stone-100 text-stone-600'">
          Pro oba
        </button>
        <button @click="planningFor = 'me'"
          class="px-3 py-1 rounded-full text-sm"
          :class="planningFor === 'me' ? 'bg-orange-600 text-white' : 'bg-stone-100 text-stone-600'">
          Pro mě
        </button>
      </div>
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
import { ref, computed, onMounted, watch } from 'vue'
import { useToast } from 'vue-toastification'
import * as planApi from '../api/plan'

const toast = useToast()

const dayOptions = [3, 5, 7, 14]
const numDays = ref(7)
const offset = ref(0)
const entries = ref<planApi.MealPlanEntry[]>([])
const suggestions = ref<any[]>([])
const suggestPrompt = ref('')
const suggesting = ref(false)
const planningFor = ref<'both' | 'me'>('both')

const startDate = computed(() => {
  if (numDays.value === 7 || numDays.value === 14) {
    // Align to Monday
    const d = new Date()
    d.setDate(d.getDate() - d.getDay() + 1 + offset.value * numDays.value)
    return d
  }
  // For other counts, start from today + offset
  const d = new Date()
  d.setDate(d.getDate() + offset.value * numDays.value)
  return d
})

const endDate = computed(() => {
  const d = new Date(startDate.value)
  d.setDate(d.getDate() + numDays.value - 1)
  return d
})

const periodLabel = computed(() => `${fmtCz(startDate.value)} – ${fmtCz(endDate.value)}`)

const suggestPlaceholder = computed(() =>
  `Návrh jídla na ${numDays.value} ${numDays.value < 5 ? 'dny' : 'dní'}...`
)

function fmt(d: Date) {
  return d.toISOString().slice(0, 10)
}

function fmtCz(d: Date) {
  return d.toLocaleDateString('cs-CZ', { day: 'numeric', month: 'numeric' })
}

const days = computed(() => {
  const result = []
  for (let i = 0; i < numDays.value; i++) {
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

function shiftPeriod(dir: number) {
  offset.value += dir
  loadEntries()
}

async function loadEntries() {
  try {
    const from = fmt(startDate.value)
    const to = fmt(endDate.value)
    entries.value = await planApi.listPlan(from, to)
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se načíst plán')
  }
}

async function handleSuggest() {
  suggesting.value = true
  try {
    const dateRange = `od ${fmt(startDate.value)} do ${fmt(endDate.value)}`
    const fullPrompt = suggestPrompt.value
      ? `${suggestPrompt.value} (${dateRange})`
      : `Navrhni jídla ${dateRange}`
    suggestions.value = await planApi.suggestPlan(fullPrompt, planningFor.value)
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se navrhnout jídla')
  } finally {
    suggesting.value = false
  }
}

async function confirmEntry(entry: any) {
  try {
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
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se potvrdit jídlo')
  }
}

async function removeEntry(id: string) {
  if (id.startsWith('sug-')) {
    suggestions.value = suggestions.value.filter(s => `sug-${s.date}-${s.meal_type}` !== id)
  } else {
    try {
      await planApi.deletePlanEntry(id)
      await loadEntries()
    } catch (e: any) {
      toast.error(e.message || 'Nepodařilo se smazat položku')
    }
  }
}

watch(numDays, () => {
  offset.value = 0
})

onMounted(loadEntries)
</script>
