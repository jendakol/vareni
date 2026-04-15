<template>
  <div v-if="recipe">
    <!-- View mode -->
    <div v-if="!editing">
      <div class="flex items-start justify-between mb-4">
        <div class="flex items-center gap-2 flex-wrap">
          <h1 class="text-2xl font-bold text-stone-800">
            <span v-if="recipe.emoji" class="mr-1">{{ recipe.emoji }}</span>{{ recipe.title }}
          </h1>
          <span v-if="recipe.status === 'discovered'" class="text-xs bg-green-100 text-green-700 rounded-full px-2 py-1">
            Objevený ({{ Math.round((recipe.discovery_score || 0) * 100) }}%)
          </span>
          <span v-if="recipe.status === 'tested'" class="text-xs bg-blue-100 text-blue-700 rounded-full px-2 py-1">
            Vyzkoušeno
          </span>
        </div>
        <div class="flex items-center gap-1">
          <button @click="editing = true" title="Upravit"
            class="p-2 text-stone-400 hover:text-stone-700 rounded-lg hover:bg-stone-100">
            <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17 3a2.85 2.85 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z"/><path d="m15 5 4 4"/></svg>
          </button>
          <button @click="handleShare" title="Sdílet"
            class="p-2 text-stone-400 hover:text-stone-700 rounded-lg hover:bg-stone-100">
            <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="18" cy="5" r="3"/><circle cx="6" cy="12" r="3"/><circle cx="18" cy="19" r="3"/><line x1="8.59" y1="13.51" x2="15.42" y2="17.49"/><line x1="15.41" y1="6.51" x2="8.59" y2="10.49"/></svg>
          </button>
          <button @click="handleDelete" title="Smazat"
            class="p-2 text-stone-400 hover:text-red-600 rounded-lg hover:bg-red-50">
            <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/><line x1="10" y1="11" x2="10" y2="17"/><line x1="14" y1="11" x2="14" y2="17"/></svg>
          </button>
        </div>
      </div>

      <div class="flex items-center gap-2 mb-4 flex-wrap">
        <template v-if="recipe.status === 'discovered'">
          <button @click="handleStatus('saved')"
            class="px-5 py-2.5 bg-green-600 text-white rounded-lg hover:bg-green-700 font-medium inline-flex items-center gap-2">
            <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
            Uložit
          </button>
          <button @click="handleStatus('rejected')"
            class="px-5 py-2.5 bg-stone-200 text-stone-700 rounded-lg hover:bg-stone-300 font-medium">
            Odmítnout
          </button>
          <button @click="handleStatus('rejected_similar')"
            class="px-5 py-2.5 bg-red-100 text-red-700 rounded-lg hover:bg-red-200 font-medium"
            title="Odmítne tento recept i podobné v budoucnu">
            Odmítnout podobné
          </button>
        </template>
        <template v-else>
          <button @click="startCooking"
            class="px-5 py-2.5 bg-green-600 text-white rounded-lg hover:bg-green-700 font-medium inline-flex items-center gap-2">
            <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="5 3 19 12 5 21 5 3"/></svg>
            Vařit
          </button>
          <button v-if="recipe.status === 'saved'" @click="markTested"
            class="px-5 py-2.5 bg-blue-600 text-white rounded-lg hover:bg-blue-700 font-medium inline-flex items-center gap-2">
            <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
            Vyzkoušeno
          </button>
        </template>
      </div>

      <p v-if="recipe.description" class="text-stone-600 mb-4">{{ recipe.description }}</p>

      <div class="flex gap-5 text-sm text-stone-500 mb-4">
        <span v-if="recipe.prep_time_min" class="flex items-center gap-1" title="Příprava">
          <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/></svg>
          {{ recipe.prep_time_min }} min
        </span>
        <span v-if="recipe.cook_time_min" class="flex items-center gap-1" title="Vaření">
          <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
          {{ recipe.cook_time_min }} min
        </span>
        <span v-if="recipe.servings" class="flex items-center gap-1" title="Porce">
          <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 2v7c0 1.1.9 2 2 2h4a2 2 0 0 0 2-2V2"/><path d="M7 2v20"/><path d="M21 15V2a5 5 0 0 0-5 5v6c0 1.1.9 2 2 2h3Zm0 0v7"/></svg>
          {{ recipe.servings }} {{ recipe.servings === 1 ? 'porce' : recipe.servings! < 5 ? 'porce' : 'porcí' }}
        </span>
        <a v-if="recipe.source_url" :href="recipe.source_url" target="_blank"
          class="flex items-center gap-1 hover:text-orange-600" title="Zdroj receptu">
          <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/><polyline points="15 3 21 3 21 9"/><line x1="10" y1="14" x2="21" y2="3"/></svg>
          {{ sourceDomain }}
        </a>
      </div>

      <TagChips v-if="recipe.tags?.length" :tags="recipe.tags" class="mb-6" />

      <section class="mb-8">
        <h2 class="text-xl font-bold text-stone-800 mb-3">Ingredience</h2>
        <IngredientList :ingredients="recipe.ingredients || []" />
      </section>

      <section class="mb-8">
        <h2 class="text-xl font-bold text-stone-800 mb-3">Postup</h2>
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
    </div>

    <!-- Edit mode -->
    <div v-else>
      <div class="flex items-center justify-between mb-4">
        <h2 class="text-lg font-semibold text-stone-700">Upravit recept</h2>
        <button @click="editing = false" class="text-stone-500 hover:text-stone-700 text-sm">← Zpět</button>
      </div>
      <RecipeForm :initial="editData" @save="handleUpdate" />
    </div>

    <CookingMode v-if="cooking" :steps="recipe.steps || []" @close="cooking = false" />
    <ChatOverlay v-if="showChat" :recipe-id="recipe.id" @close="showChat = false" @update="refreshRecipe" />
  </div>
  <div v-else-if="loadError" class="text-center py-8">
    <p class="text-red-600 font-medium">Recept se nepodařilo načíst</p>
    <button @click="loadRecipe" class="mt-2 text-orange-600 hover:underline text-sm">Zkusit znovu</button>
  </div>
  <div v-else class="text-center text-stone-400 py-8">Načítám...</div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useToast } from 'vue-toastification'
import * as api from '../api/recipes'
import { updateRecipeStatus } from '../api/recipes'
import type { Recipe } from '../api/recipes'
import TagChips from '../components/TagChips.vue'
import IngredientList from '../components/IngredientList.vue'
import CookingMode from '../components/CookingMode.vue'
import ChatOverlay from '../components/ChatOverlay.vue'
import RecipeForm from '../components/RecipeForm.vue'

const route = useRoute()
const router = useRouter()
const toast = useToast()
const recipe = ref<Recipe | null>(null)
const cooking = ref(false)
const showChat = ref(false)
const editing = ref(false)
const loadError = ref(false)

const sourceDomain = computed(() => {
  if (!recipe.value?.source_url) return ''
  try {
    const hostname = new URL(recipe.value.source_url).hostname
    if (hostname.endsWith('instagram.com')) return 'Instagram'
    return hostname.replace('www.', '')
  } catch { return recipe.value.source_url }
})

const editData = computed(() => {
  if (!recipe.value) return null
  return {
    title: recipe.value.title,
    description: recipe.value.description,
    emoji: recipe.value.emoji,
    servings: recipe.value.servings,
    prep_time_min: recipe.value.prep_time_min,
    cook_time_min: recipe.value.cook_time_min,
    tags: recipe.value.tags || [],
    ingredients: (recipe.value.ingredients || []).map(i => ({
      name: i.name,
      amount: i.amount,
      unit: i.unit,
      note: i.note,
    })),
    steps: (recipe.value.steps || []).map(s => ({
      step_order: s.step_order,
      instruction: s.instruction,
    })),
  }
})

async function loadRecipe() {
  try {
    loadError.value = false
    recipe.value = await api.getRecipe(route.params.id as string)
  } catch (e: any) {
    loadError.value = true
    toast.error(e.message || 'Nepodařilo se načíst recept')
  }
}

function startCooking() { cooking.value = true }

async function handleStatus(status: string) {
  if (!recipe.value) return
  if (status === 'rejected_similar' && !confirm('Odmítnout tento recept a podobné v budoucnu?')) return
  try {
    await updateRecipeStatus(recipe.value.id, status)
    const msg = status === 'saved' ? 'Recept uložen' : 'Recept odmítnut'
    toast.success(msg)
    router.push({ path: '/recipes', query: { tab: 'discovered' } })
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se změnit stav')
  }
}

async function markTested() {
  if (!recipe.value) return
  try {
    await updateRecipeStatus(recipe.value.id, 'tested')
    recipe.value.status = 'tested'
    toast.success('Recept označen jako vyzkoušený')
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se změnit stav')
  }
}

async function handleShare() {
  if (!recipe.value) return
  try {
    const result = await api.shareRecipe(recipe.value.id)
    await navigator.clipboard.writeText(result.share_url)
    toast.success('Odkaz zkopírován!')
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se sdílet recept')
  }
}

async function handleUpdate(data: any) {
  if (!recipe.value) return
  try {
    await api.updateRecipe(recipe.value.id, data)
    editing.value = false
    toast.success('Recept uložen')
    await loadRecipe()
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se uložit změny')
  }
}

async function handleDelete() {
  if (!recipe.value) return
  if (!confirm(`Opravdu smazat "${recipe.value.title}"?`)) return
  try {
    await api.deleteRecipe(recipe.value.id)
    toast.success('Recept smazán')
    router.push('/recipes')
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se smazat recept')
  }
}

function refreshRecipe() { loadRecipe() }

onMounted(loadRecipe)
</script>
