<template>
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" @click.self="$emit('close')">
    <div class="bg-white rounded-xl shadow-xl w-full max-w-lg mx-4 max-h-[85vh] flex flex-col">
      <!-- Header -->
      <div class="flex items-center justify-between p-4 border-b border-stone-200 flex-shrink-0">
        <h3 v-if="recipe" class="font-semibold text-stone-800 truncate">
          <span v-if="recipe.emoji" class="mr-1">{{ recipe.emoji }}</span>{{ recipe.title }}
        </h3>
        <h3 v-else class="text-stone-400">Načítám...</h3>
        <button @click="$emit('close')" class="p-2 text-stone-400 hover:text-stone-700 flex-shrink-0">
          <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
        </button>
      </div>

      <!-- Body -->
      <div v-if="recipe" class="flex-1 overflow-y-auto p-4 space-y-4">
        <p v-if="recipe.description" class="text-stone-600 text-sm">{{ recipe.description }}</p>

        <div class="flex gap-4 text-sm text-stone-500">
          <span v-if="recipe.prep_time_min">Příprava: {{ recipe.prep_time_min }} min</span>
          <span v-if="recipe.cook_time_min">Vaření: {{ recipe.cook_time_min }} min</span>
          <span v-if="recipe.servings">{{ recipe.servings }} {{ recipe.servings === 1 ? 'porce' : recipe.servings! < 5 ? 'porce' : 'porcí' }}</span>
        </div>

        <TagChips v-if="recipe.tags?.length" :tags="recipe.tags" />

        <section>
          <h4 class="font-semibold text-stone-700 mb-2">Ingredience</h4>
          <IngredientList :ingredients="recipe.ingredients || []" />
        </section>

        <section>
          <h4 class="font-semibold text-stone-700 mb-2">Postup</h4>
          <ol class="space-y-3">
            <li v-for="step in recipe.steps" :key="step.step_order" class="flex gap-3">
              <span class="flex-shrink-0 w-6 h-6 rounded-full bg-orange-100 text-orange-700 flex items-center justify-center text-xs font-medium">
                {{ step.step_order }}
              </span>
              <p class="text-stone-700 text-sm pt-0.5">{{ step.instruction }}</p>
            </li>
          </ol>
        </section>
      </div>

      <!-- Loading -->
      <div v-else class="flex-1 flex items-center justify-center p-8">
        <span class="text-stone-400">Načítám recept...</span>
      </div>

      <!-- Footer -->
      <div class="flex gap-2 p-4 border-t border-stone-200 flex-shrink-0">
        <router-link :to="`/recipes/${recipeId}`" @click="$emit('close')"
          class="flex-1 text-center px-4 py-2 bg-orange-600 text-white rounded-lg hover:bg-orange-700 text-sm font-medium">
          Otevřít recept
        </router-link>
        <button @click="$emit('close')"
          class="px-4 py-2 border border-stone-300 rounded-lg text-stone-600 hover:bg-stone-50 text-sm">
          Zavřít
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useToast } from 'vue-toastification'
import * as api from '../api/recipes'
import type { Recipe } from '../api/recipes'
import TagChips from './TagChips.vue'
import IngredientList from './IngredientList.vue'

const props = defineProps<{ recipeId: string }>()
defineEmits<{ close: [] }>()

const toast = useToast()
const recipe = ref<Recipe | null>(null)

onMounted(async () => {
  try {
    recipe.value = await api.getRecipe(props.recipeId)
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se načíst recept')
  }
})
</script>
