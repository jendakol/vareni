<template>
  <div v-if="recipe">
    <div v-if="recipe.cover_image_id" class="aspect-square w-full sm:max-w-xs overflow-hidden rounded-lg bg-stone-100 mb-4">
      <img :src="publicCoverUrl(route.params.slug as string, 'full')" :alt="recipe.title" class="w-full h-full object-cover" />
    </div>
    <h1 class="text-2xl font-bold text-stone-800 mb-2">{{ recipe.title }}</h1>
    <p v-if="recipe.description" class="text-stone-600 mb-4">{{ recipe.description }}</p>
    <TagChips v-if="recipe.tags?.length" :tags="recipe.tags" class="mb-6" />

    <section class="mb-8">
      <h2 class="text-lg font-semibold text-stone-700 mb-3">Ingredience</h2>
      <IngredientList :sections="recipe.sections || []" />
    </section>

    <section>
      <h2 class="text-lg font-semibold text-stone-700 mb-3">Postup</h2>
      <StepList :sections="recipe.sections || []" />
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
import type { PublicRecipe } from '../api/recipes'
import { publicCoverUrl } from '../api/images'
import TagChips from '../components/TagChips.vue'
import IngredientList from '../components/IngredientList.vue'
import StepList from '../components/StepList.vue'

const route = useRoute()
const toast = useToast()
const recipe = ref<PublicRecipe | null>(null)
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
