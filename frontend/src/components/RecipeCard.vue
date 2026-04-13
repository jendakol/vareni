<template>
  <a :href="`/recipes/${recipe.id}`" @click.prevent="$router.push(`/recipes/${recipe.id}`)"
    class="block bg-white rounded-xl border border-stone-200 p-4 hover:shadow-md transition-shadow">
    <h3 class="font-semibold text-stone-800 text-lg">
      <span v-if="recipe.emoji" class="mr-1">{{ recipe.emoji }}</span>{{ recipe.title }}
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
  </a>
</template>

<script setup lang="ts">
import type { Recipe } from '../api/recipes'
defineProps<{ recipe: Recipe }>()
</script>
