<template>
  <div v-if="recipe">
    <h1 class="text-2xl font-bold text-stone-800 mb-2">{{ recipe.title }}</h1>
    <p v-if="recipe.description" class="text-stone-600 mb-4">{{ recipe.description }}</p>
    <TagChips v-if="recipe.tags?.length" :tags="recipe.tags" class="mb-6" />

    <section class="mb-8">
      <h2 class="text-lg font-semibold text-stone-700 mb-3">Ingredience</h2>
      <IngredientList :ingredients="recipe.ingredients || []" />
    </section>

    <section>
      <h2 class="text-lg font-semibold text-stone-700 mb-3">Postup</h2>
      <ol class="space-y-4">
        <li v-for="step in recipe.steps" :key="step.step_order" class="flex gap-3">
          <span class="flex-shrink-0 w-7 h-7 rounded-full bg-orange-100 text-orange-700 flex items-center justify-center text-sm font-medium">
            {{ step.step_order }}
          </span>
          <p class="text-stone-700 pt-0.5">{{ step.instruction }}</p>
        </li>
      </ol>
    </section>
  </div>
  <div v-else-if="loadError" class="text-center py-8">
    <p class="text-red-600 font-medium">Recept nenalezen</p>
  </div>
  <div v-else class="text-center text-stone-400 py-8">Načítám...</div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRoute } from 'vue-router'
import { useToast } from 'vue-toastification'
import { getPublicRecipe } from '../api/recipes'
import type { Recipe } from '../api/recipes'
import TagChips from '../components/TagChips.vue'
import IngredientList from '../components/IngredientList.vue'

const route = useRoute()
const toast = useToast()
const recipe = ref<Recipe | null>(null)
const loadError = ref(false)

onMounted(async () => {
  try {
    recipe.value = await getPublicRecipe(route.params.slug as string)
  } catch (e: any) {
    loadError.value = true
    toast.error(e.message || 'Recept nenalezen')
  }
})
</script>
