<template>
  <a :href="`/recipes/${recipe.id}`" @click.prevent="$router.push(`/recipes/${recipe.id}`)"
    class="block rounded-xl p-4 hover:shadow-md transition-shadow"
    :class="cardClass" :title="statusLabel">
    <div class="flex items-start gap-3">
      <span v-if="recipe.emoji" class="text-5xl sm:text-3xl leading-none">{{ recipe.emoji }}</span>
      <h3 class="font-semibold text-stone-800 text-xl">{{ recipe.title }}</h3>
    </div>
    <div v-if="recipe.prep_time_min || recipe.cook_time_min" class="flex items-center gap-4 mt-3 text-sm text-stone-500">
      <span class="flex items-center gap-1">
        <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
        {{ (recipe.prep_time_min || 0) + (recipe.cook_time_min || 0) }} min
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
        class="flex-1 px-3 py-1 bg-orange-600 text-white rounded-lg text-sm hover:bg-orange-700">
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
        class="px-3 py-1 border border-blue-300 text-blue-600 rounded-lg text-sm hover:bg-blue-50">
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
  switch (props.recipe.status) {
    case 'tested':
      return 'border-2 border-green-500 bg-white'
    case 'saved':
      return 'border-2 border-orange-400 bg-white'
    case 'discovered':
      return 'border-2 border-dashed border-blue-300 bg-blue-50'
    case 'rejected':
    case 'rejected_similar':
      return 'border border-red-200 bg-red-50/30'
    default:
      return 'border border-stone-200 bg-white'
  }
})

// The border carries the status visually; this keeps it reachable to a screen reader.
const statusLabel = computed(() => {
  switch (props.recipe.status) {
    case 'tested': return 'Vyzkoušeno'
    case 'saved': return 'Ještě nevyzkoušeno'
    case 'discovered': return 'Objevený recept'
    default: return ''
  }
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
  if (s >= 0.8) return 'bg-blue-200 text-blue-800'
  if (s >= 0.6) return 'bg-blue-100 text-blue-700'
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
