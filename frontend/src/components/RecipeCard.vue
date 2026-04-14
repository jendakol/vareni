<template>
  <a :href="`/recipes/${recipe.id}`" @click.prevent="$router.push(`/recipes/${recipe.id}`)"
    class="block rounded-xl p-4 hover:shadow-md transition-shadow"
    :class="cardClass">
    <h3 class="font-semibold text-stone-800 text-lg">
      <span v-if="recipe.emoji" class="mr-1">{{ recipe.emoji }}</span>{{ recipe.title }}
      <span v-if="recipe.status === 'tested'" class="ml-1 text-green-600" title="Vyzkoušeno">&#x2713;</span>
      <span v-else-if="recipe.status === 'saved'" class="ml-1 text-orange-400" title="Ještě nevyzkoušeno">?</span>
    </h3>
    <p v-if="recipe.description" class="text-stone-500 text-sm mt-1 line-clamp-2">{{ recipe.description }}</p>
    <div class="flex items-center gap-4 mt-3 text-sm text-stone-500">
      <span v-if="recipe.prep_time_min || recipe.cook_time_min" class="flex items-center gap-1">
        <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
        {{ (recipe.prep_time_min || 0) + (recipe.cook_time_min || 0) }} min
      </span>
      <span v-if="recipe.servings" class="flex items-center gap-1">
        <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 2v7c0 1.1.9 2 2 2h4a2 2 0 0 0 2-2V2"/><path d="M7 2v20"/><path d="M21 15V2a5 5 0 0 0-5 5v6c0 1.1.9 2 2 2h3Zm0 0v7"/></svg>
        {{ recipe.servings }} {{ recipe.servings === 1 ? 'porce' : recipe.servings < 5 ? 'porce' : 'porcí' }}
      </span>
    </div>

    <!-- Discovery badge -->
    <div v-if="recipe.status === 'discovered'" class="flex items-center gap-2 mt-2">
      <span class="text-xs rounded-full px-2 py-0.5 cursor-help"
        :class="scoreBadgeClass"
        :title="scoreTooltip">
        {{ scoreLabel }}
      </span>
      <span v-if="sourceHost" class="text-xs text-stone-400 truncate">
        z {{ sourceHost }}
      </span>
    </div>

    <!-- Action buttons for discovered recipes -->
    <div v-if="recipe.status === 'discovered'" class="flex gap-2 mt-3" @click.stop.prevent>
      <button @click.stop.prevent="$emit('status', recipe.id, 'saved')"
        class="flex-1 px-3 py-1 bg-green-600 text-white rounded-lg text-sm hover:bg-green-700">
        Uložit
      </button>
      <button @click.stop.prevent="$emit('status', recipe.id, 'rejected')"
        class="px-3 py-1 border border-red-300 text-red-600 rounded-lg text-sm hover:bg-red-50">
        Odmítnout
      </button>
      <button @click.stop.prevent="confirmRejectSimilar"
        class="px-3 py-1 border border-red-300 text-red-600 rounded-lg text-sm hover:bg-red-50"
        title="Odmítne tento recept a podobné v budoucnu">
        Odmítnout podobné
      </button>
    </div>

    <!-- Restore button for rejected recipes -->
    <div v-if="recipe.status === 'rejected' || recipe.status === 'rejected_similar'" class="flex gap-2 mt-3" @click.stop.prevent>
      <button @click.stop.prevent="$emit('status', recipe.id, 'discovered')"
        class="px-3 py-1 border border-stone-300 text-stone-600 rounded-lg text-sm hover:bg-stone-50">
        Obnovit
      </button>
      <span v-if="recipe.status === 'rejected_similar'" class="text-xs text-red-400 self-center">
        Blokuje podobné recepty
      </span>
    </div>
  </a>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { Recipe } from '../api/recipes'

const props = defineProps<{ recipe: Recipe }>()
const emit = defineEmits<{ status: [id: string, status: string] }>()

function confirmRejectSimilar() {
  if (window.confirm('Opravdu odmítnout tento recept a blokovat podobné v budoucnu? Toto ovlivní objevování pro všechny uživatele. (Lze vrátit v záložce Odmítnuté.)')) {
    emit('status', props.recipe.id, 'rejected_similar')
  }
}

const cardClass = computed(() => {
  if (props.recipe.status === 'discovered') {
    return 'border-2 border-dashed border-green-300 bg-green-50'
  }
  if (props.recipe.status === 'rejected' || props.recipe.status === 'rejected_similar') {
    return 'border border-red-200 bg-red-50/30'
  }
  return 'border border-stone-200 bg-white'
})

const scoreLabel = computed(() => {
  const s = props.recipe.discovery_score ?? 0
  if (s >= 0.8) return 'Skvělý tip'
  if (s >= 0.6) return 'Dobrý tip'
  if (s >= 0.4) return 'Možná zajímavé'
  return 'Náhodný objev'
})

const scoreBadgeClass = computed(() => {
  const s = props.recipe.discovery_score ?? 0
  if (s >= 0.8) return 'bg-green-200 text-green-800'
  if (s >= 0.6) return 'bg-green-100 text-green-700'
  if (s >= 0.4) return 'bg-stone-100 text-stone-600'
  return 'bg-stone-100 text-stone-500'
})

const scoreTooltip = computed(() => {
  const parts: string[] = []
  const score = props.recipe.discovery_score
  if (score !== null && score !== undefined) {
    parts.push(`Relevance: ${Math.round(score * 100)}%`)
  }
  if (props.recipe.canonical_name) {
    parts.push(`Kanonický název: ${props.recipe.canonical_name}`)
  }
  if (sourceHost.value) {
    parts.push(`Zdroj: ${sourceHost.value}`)
  }
  if (props.recipe.discovered_at) {
    const date = new Date(props.recipe.discovered_at)
    if (!isNaN(date.getTime())) {
      parts.push(`Objeveno: ${date.toLocaleDateString('cs-CZ')}`)
    }
  }
  return parts.join('\n')
})

const sourceHost = computed(() => {
  if (!props.recipe.source_url) return ''
  try {
    return new URL(props.recipe.source_url).hostname.replace('www.', '')
  } catch {
    return ''
  }
})
</script>
