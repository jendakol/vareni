<template>
  <div>
    <div class="flex items-center justify-between mb-6">
      <h1 class="text-2xl font-bold text-stone-800">Recepty</h1>
      <router-link to="/recipes/new"
        class="px-4 py-2 bg-orange-600 text-white rounded-lg hover:bg-orange-700 font-medium">
        + Nový recept
      </router-link>
    </div>

    <div class="mb-4">
      <input v-model="search" @input="debouncedFetch" placeholder="Hledat recepty..."
        class="w-full px-4 py-3 border border-stone-300 rounded-lg text-lg" />
    </div>

    <div class="flex gap-2 mb-4">
      <button v-for="s in sorts" :key="s.value" @click="sort = s.value; fetchRecipes()"
        class="px-3 py-1 rounded-full text-sm"
        :class="sort === s.value ? 'bg-orange-600 text-white' : 'bg-stone-100 text-stone-600 hover:bg-stone-200'">
        {{ s.label }}
      </button>
    </div>

    <div v-if="store.loading" class="text-center text-stone-500 py-8">Načítám...</div>
    <div v-else-if="store.error" class="text-center py-8">
      <p class="text-red-600 font-medium">Chyba při načítání receptů</p>
      <button @click="store.fetch()" class="mt-2 text-orange-600 hover:underline text-sm">Zkusit znovu</button>
    </div>
    <div v-else-if="store.recipes.length === 0" class="text-center text-stone-400 py-8">Žádné recepty</div>
    <div v-else class="space-y-3">
      <RecipeCard v-for="recipe in store.recipes" :key="recipe.id" :recipe="recipe" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRecipeStore } from '../stores/recipes'
import RecipeCard from '../components/RecipeCard.vue'

const store = useRecipeStore()
const search = ref('')
const sort = ref('recent')
let debounceTimer: ReturnType<typeof setTimeout>

const sorts = [
  { value: 'recent', label: 'Nejnovější' },
  { value: 'least_cooked', label: 'Dlouho nevařené' },
  { value: 'prep_time', label: 'Nejrychlejší' },
]

function fetchRecipes() {
  store.fetch({ q: search.value || undefined, sort: sort.value })
}

function debouncedFetch() {
  clearTimeout(debounceTimer)
  debounceTimer = setTimeout(fetchRecipes, 300)
}

onMounted(fetchRecipes)
</script>
