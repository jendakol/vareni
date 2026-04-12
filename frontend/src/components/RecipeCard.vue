<template>
  <a :href="`/recipes/${recipe.id}`" @click.prevent="$router.push(`/recipes/${recipe.id}`)"
    class="block bg-white rounded-xl border border-stone-200 p-4 hover:shadow-md transition-shadow">
    <h3 class="font-semibold text-stone-800 text-lg">{{ recipe.title }}</h3>
    <p v-if="recipe.description" class="text-stone-500 text-sm mt-1 line-clamp-2">{{ recipe.description }}</p>
    <div class="flex items-center gap-3 mt-3 text-sm text-stone-500">
      <span v-if="recipe.prep_time_min">{{ recipe.prep_time_min + (recipe.cook_time_min || 0) }} min</span>
      <span v-if="recipe.servings">{{ recipe.servings }} porci</span>
    </div>
  </a>
</template>

<script setup lang="ts">
import type { Recipe } from '../api/recipes'
defineProps<{ recipe: Recipe }>()
</script>
