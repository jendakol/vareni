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

    <div v-if="store.loading" class="text-center text-stone-500 py-8">Načítám...</div>
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
let debounceTimer: ReturnType<typeof setTimeout>

function debouncedFetch() {
  clearTimeout(debounceTimer)
  debounceTimer = setTimeout(() => {
    store.fetch({ q: search.value || undefined })
  }, 300)
}

onMounted(() => store.fetch())
</script>
