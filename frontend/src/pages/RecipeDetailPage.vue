<template>
  <div v-if="recipe">
    <div class="flex items-start justify-between mb-4">
      <h1 class="text-2xl font-bold text-stone-800">{{ recipe.title }}</h1>
      <div class="flex gap-2">
        <button @click="startCooking"
          class="px-4 py-2 bg-orange-600 text-white rounded-lg hover:bg-orange-700">
          Vařit
        </button>
        <button @click="handleShare" class="px-4 py-2 border border-stone-300 rounded-lg hover:bg-stone-100">
          Sdílet
        </button>
      </div>
    </div>

    <p v-if="recipe.description" class="text-stone-600 mb-4">{{ recipe.description }}</p>

    <div class="flex gap-4 text-sm text-stone-500 mb-4">
      <span v-if="recipe.prep_time_min">Příprava: {{ recipe.prep_time_min }} min</span>
      <span v-if="recipe.cook_time_min">Vaření: {{ recipe.cook_time_min }} min</span>
      <span v-if="recipe.servings">{{ recipe.servings }} porcí</span>
    </div>

    <TagChips v-if="recipe.tags?.length" :tags="recipe.tags" class="mb-6" />

    <section class="mb-8">
      <h2 class="text-lg font-semibold text-stone-700 mb-3">Ingredience</h2>
      <IngredientList :ingredients="recipe.ingredients || []" />
    </section>

    <section class="mb-8">
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

    <!-- Chat FAB -->
    <button @click="showChat = true"
      class="fixed bottom-6 right-6 w-14 h-14 bg-orange-600 text-white rounded-full shadow-lg hover:bg-orange-700 flex items-center justify-center text-2xl z-40">
      💬
    </button>

    <CookingMode v-if="cooking" :steps="recipe.steps || []" @close="cooking = false" />
    <ChatOverlay v-if="showChat" :recipe-id="recipe.id" @close="showChat = false" @update="refreshRecipe" />
  </div>
  <div v-else class="text-center text-stone-400 py-8">Načítám...</div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRoute } from 'vue-router'
import * as api from '../api/recipes'
import type { Recipe } from '../api/recipes'
import TagChips from '../components/TagChips.vue'
import IngredientList from '../components/IngredientList.vue'
import CookingMode from '../components/CookingMode.vue'
import ChatOverlay from '../components/ChatOverlay.vue'

const route = useRoute()
const recipe = ref<Recipe | null>(null)
const cooking = ref(false)
const showChat = ref(false)

async function loadRecipe() {
  recipe.value = await api.getRecipe(route.params.id as string)
}

function startCooking() { cooking.value = true }

async function handleShare() {
  if (!recipe.value) return
  const result = await api.shareRecipe(recipe.value.id)
  await navigator.clipboard.writeText(result.share_url)
  alert('Odkaz zkopírován!')
}

function refreshRecipe() { loadRecipe() }

onMounted(loadRecipe)
</script>
