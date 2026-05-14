<template>
  <div>
    <div class="flex items-center justify-between mb-6">
      <h1 class="text-2xl font-bold text-stone-800">Co jsme jedli?</h1>
      <div class="flex gap-2 items-center">
        <button @click="shiftDay(-1)" class="hidden sm:block px-3 py-1 border rounded-lg text-stone-600 hover:bg-stone-100">&larr;</button>
        <label class="relative px-3 py-1 border rounded-lg text-stone-600 text-center cursor-pointer hover:bg-stone-100">
          {{ dateLabel }}
          <input type="date" :value="date" :max="today" @input="onDatePick" ref="datePicker"
            class="absolute inset-0 w-full h-full opacity-0 cursor-pointer [&::-webkit-calendar-picker-indicator]:absolute [&::-webkit-calendar-picker-indicator]:inset-0 [&::-webkit-calendar-picker-indicator]:w-full [&::-webkit-calendar-picker-indicator]:h-full [&::-webkit-calendar-picker-indicator]:opacity-0 [&::-webkit-calendar-picker-indicator]:cursor-pointer" />
        </label>
        <button @click="shiftDay(1)" :disabled="date === today"
          class="hidden sm:block px-3 py-1 border rounded-lg text-stone-600 hover:bg-stone-100 disabled:opacity-30 disabled:cursor-not-allowed">&rarr;</button>
      </div>
    </div>

    <!-- Existing entries -->
    <div v-if="entries.length > 0" class="mb-6 space-y-2">
      <div v-for="entry in entries" :key="entry.id"
        class="bg-white rounded-lg border border-stone-200 px-4 py-3">
        <!-- View mode -->
        <div v-if="editingId !== entry.id" class="flex items-center justify-between">
          <div class="flex items-center gap-2 min-w-0">
            <span class="text-xs text-stone-400 uppercase shrink-0">{{ entry.meal_type === 'lunch' ? 'oběd' : 'večeře' }}</span>
            <span v-if="entry.user_name" class="text-xs bg-stone-100 text-stone-500 px-2 py-0.5 rounded-full shrink-0">{{ capitalize(entry.user_name) }}</span>
            <router-link v-if="entry.recipe_id" :to="`/recipes/${entry.recipe_id}`"
              class="text-orange-700 hover:text-orange-800 hover:underline truncate">
              {{ entry.recipe_title || 'Recept' }}
            </router-link>
            <span v-else class="text-stone-800 truncate">{{ entry.free_text || 'Recept' }}</span>
          </div>
          <div class="flex gap-1 shrink-0 ml-2">
            <button @click="startEdit(entry)" class="p-1 text-stone-400 hover:text-stone-700" title="Upravit">
              <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17 3a2.85 2.85 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z"/><path d="m15 5 4 4"/></svg>
            </button>
            <button @click="removeEntry(entry.id)" class="p-1 text-stone-400 hover:text-red-600" title="Smazat">
              <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/></svg>
            </button>
          </div>
        </div>
        <!-- Edit mode -->
        <div v-else class="space-y-2">
          <div class="flex items-center gap-2">
            <span class="text-xs text-stone-400 uppercase shrink-0">{{ entry.meal_type === 'lunch' ? 'oběd' : 'večeře' }}</span>
            <span v-if="entry.user_name" class="text-xs bg-stone-100 text-stone-500 px-2 py-0.5 rounded-full shrink-0">{{ capitalize(entry.user_name) }}</span>
          </div>
          <input v-model="editText" @keyup.enter="saveEdit(entry)" @keyup.escape="editingId = null"
            class="w-full px-3 py-2 border border-stone-300 rounded-lg text-sm" />
          <div class="flex gap-2">
            <button @click="saveEdit(entry)" class="px-3 py-1 bg-green-600 text-white text-sm rounded-lg hover:bg-green-700">Uložit</button>
            <button @click="editingId = null" class="px-3 py-1 text-stone-500 text-sm hover:text-stone-700">Zrušit</button>
          </div>
        </div>
      </div>
    </div>

    <div v-if="entries.length > 0" class="border-t border-stone-200 pt-6 mb-2">
      <h2 class="text-sm font-medium text-stone-500 mb-4">Přidat záznam</h2>
    </div>

    <div class="space-y-6">
      <div v-for="meal in meals" :key="meal.key" class="bg-white rounded-xl border border-stone-200 p-4">
        <h3 class="font-medium text-stone-700 mb-3">{{ meal.label }}</h3>

        <!-- Who ate -->
        <div class="flex gap-2 mb-3">
          <span class="text-sm text-stone-500 py-1">Kdo:</span>
          <button @click="whoAte[meal.key] = 'both'"
            class="px-3 py-1 rounded-full text-sm"
            :class="whoAte[meal.key] === 'both' ? 'bg-orange-600 text-white' : 'bg-stone-100 text-stone-600'">
            Oba
          </button>
          <button v-for="u in users" :key="u.id" @click="whoAte[meal.key] = u.id"
            class="px-3 py-1 rounded-full text-sm"
            :class="whoAte[meal.key] === u.id ? 'bg-orange-600 text-white' : 'bg-stone-100 text-stone-600'">
            {{ capitalize(u.name) }}
          </button>
        </div>

        <!-- Unified search: recipes + free-text history -->
        <div class="relative">
          <input v-model="inputs[meal.key]" @input="onInput(meal.key)" @blur="hideSuggestions(meal.key)"
            placeholder="Co jste jedli?"
            class="w-full px-4 py-3 border border-stone-300 rounded-lg text-lg" />
          <div v-if="selectedRecipe[meal.key]" class="mt-2 flex items-center gap-2 bg-orange-50 px-3 py-2 rounded-lg">
            <span class="text-xs bg-orange-200 text-orange-800 px-2 py-0.5 rounded-full font-medium shrink-0">RECEPT</span>
            <span class="text-orange-700 font-medium truncate">{{ selectedRecipe[meal.key]!.title }}</span>
            <button @click="clearSelection(meal.key)" class="text-stone-400 hover:text-red-600 ml-auto shrink-0">✕</button>
          </div>
          <ul v-if="suggestions[meal.key]?.length && !selectedRecipe[meal.key]"
            class="absolute z-10 left-0 right-0 mt-1 bg-white border border-stone-200 rounded-lg shadow-lg max-h-60 overflow-y-auto">
            <li v-for="(s, i) in suggestions[meal.key]" :key="`${s.kind}-${i}`"
              @mousedown.prevent="selectSuggestion(meal.key, s)"
              class="px-4 py-2 hover:bg-orange-50 cursor-pointer flex items-center gap-2">
              <span v-if="s.kind === 'recipe'" class="text-xs bg-orange-100 text-orange-700 px-2 py-0.5 rounded-full font-medium shrink-0">RECEPT</span>
              <span v-else class="text-xs bg-stone-100 text-stone-500 px-2 py-0.5 rounded-full shrink-0">Z HISTORIE</span>
              <span class="text-stone-700 truncate">{{ s.kind === 'recipe' ? s.recipe.title : s.text }}</span>
            </li>
          </ul>
        </div>
      </div>

      <button @click="saveLog" :disabled="saving"
        class="w-full py-3 bg-orange-600 text-white rounded-lg hover:bg-orange-700 font-medium text-lg disabled:opacity-50">
        {{ saving ? 'Ukládám...' : 'Uložit' }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { reactive, ref, computed, onMounted, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useToast } from 'vue-toastification'
import { listPlan, createPlanEntry, updatePlanEntry, deletePlanEntry, suggestFreeText, type MealPlanEntry } from '../api/plan'
import { listRecipes, type Recipe } from '../api/recipes'
import { listUsers, type User } from '../api/auth'

type Suggestion =
  | { kind: 'recipe'; recipe: Recipe }
  | { kind: 'history'; text: string }

const toast = useToast()
const route = useRoute()
const router = useRouter()

function localIso(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

const today = localIso(new Date())
const initialDate = typeof route.query.date === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(route.query.date)
  ? route.query.date
  : today
const date = ref(initialDate)

watch(date, (d) => {
  const q = d === today ? undefined : d
  if (route.query.date !== q) {
    router.replace({ query: { ...route.query, date: q } })
  }
})
const users = ref<User[]>([])
const entries = ref<MealPlanEntry[]>([])
const editingId = ref<string | null>(null)
const editText = ref('')

const dateLabel = computed(() => {
  const d = new Date(date.value + 'T00:00:00')
  return d.toLocaleDateString('cs-CZ', { weekday: 'long', day: 'numeric', month: 'numeric' })
})

function shiftDay(dir: number) {
  const d = new Date(date.value + 'T12:00:00') // noon avoids timezone/DST edge cases
  d.setDate(d.getDate() + dir)
  const iso = localIso(d)
  if (iso > today) return
  date.value = iso
}

function onDatePick(e: Event) {
  const val = (e.target as HTMLInputElement).value
  if (val) date.value = val
}

function capitalize(s: string) {
  return s.charAt(0).toUpperCase() + s.slice(1)
}

async function loadEntries() {
  try {
    const d = date.value
    const all = await listPlan(d, d)
    const order: Record<string, number> = { lunch: 0, dinner: 1 }
    entries.value = all.sort((a, b) => (order[a.meal_type] ?? 2) - (order[b.meal_type] ?? 2))
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se načíst záznamy')
  }
}

watch(date, loadEntries)

watch(() => route.query.date, (q) => {
  const d = typeof q === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(q) ? q : today
  if (d !== date.value) date.value = d
})

function startEdit(entry: MealPlanEntry) {
  editingId.value = entry.id
  editText.value = entry.free_text || ''
}

async function saveEdit(entry: MealPlanEntry) {
  try {
    await updatePlanEntry(entry.id, { free_text: editText.value })
    editingId.value = null
    toast.success('Upraveno')
    await loadEntries()
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se upravit záznam')
  }
}

async function removeEntry(id: string) {
  try {
    await deletePlanEntry(id)
    toast.success('Smazáno')
    await loadEntries()
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se smazat záznam')
  }
}

const meals = [
  { key: 'lunch', label: 'Oběd' },
  { key: 'dinner', label: 'Večeře' },
]

const inputs = reactive<Record<string, string>>({ lunch: '', dinner: '' })
const selectedRecipe = reactive<Record<string, Recipe | null>>({ lunch: null, dinner: null })
const suggestions = reactive<Record<string, Suggestion[]>>({ lunch: [], dinner: [] })
const whoAte = reactive<Record<string, string>>({ lunch: 'both', dinner: 'both' })
const saving = ref(false)

let debounceTimers: Record<string, ReturnType<typeof setTimeout>> = {}

function onInput(meal: string) {
  // Typing after a recipe was selected = user is editing, deselect
  if (selectedRecipe[meal]) {
    selectedRecipe[meal] = null
  }
  clearTimeout(debounceTimers[meal])
  debounceTimers[meal] = setTimeout(async () => {
    const q = inputs[meal]?.trim()
    if (!q || q.length < 2) {
      suggestions[meal] = []
      return
    }
    try {
      const [recipesResult, history] = await Promise.all([
        listRecipes({ q }),
        suggestFreeText(q),
      ])
      suggestions[meal] = [
        ...recipesResult.items.map((r): Suggestion => ({ kind: 'recipe', recipe: r })),
        ...history.map((t): Suggestion => ({ kind: 'history', text: t })),
      ]
    } catch {
      suggestions[meal] = []
    }
  }, 300)
}

function selectSuggestion(meal: string, s: Suggestion) {
  if (s.kind === 'recipe') {
    selectedRecipe[meal] = s.recipe
    inputs[meal] = s.recipe.title
  } else {
    selectedRecipe[meal] = null
    inputs[meal] = s.text
  }
  suggestions[meal] = []
}

function clearSelection(meal: string) {
  selectedRecipe[meal] = null
  inputs[meal] = ''
}

function hideSuggestions(meal: string) {
  // Delay so mousedown on a suggestion fires before we hide
  setTimeout(() => { suggestions[meal] = [] }, 150)
}

async function saveLog() {
  saving.value = true
  try {
    for (const meal of meals) {
      const sel = selectedRecipe[meal.key]
      const txt = inputs[meal.key]?.trim()
      const who = whoAte[meal.key]

      const base: any = {
        date: date.value,
        meal_type: meal.key,
        entry_type: 'logged',
      }

      if (sel) {
        base.recipe_id = sel.id
      } else if (txt) {
        base.free_text = txt
      } else {
        continue
      }

      if (who === 'both') {
        for (const u of users.value) {
          await createPlanEntry({ ...base, for_user_id: u.id })
        }
      } else {
        await createPlanEntry({ ...base, for_user_id: who })
      }
    }
    inputs.lunch = ''
    inputs.dinner = ''
    selectedRecipe.lunch = null
    selectedRecipe.dinner = null
    suggestions.lunch = []
    suggestions.dinner = []
    toast.success('Uloženo')
    await loadEntries()
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se uložit záznam')
  } finally {
    saving.value = false
  }
}

onMounted(async () => {
  try {
    users.value = await listUsers()
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se načíst uživatele')
  }
  await loadEntries()
})
</script>
