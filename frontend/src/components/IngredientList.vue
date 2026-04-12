<template>
  <ul class="space-y-2">
    <li v-for="ing in ingredients" :key="ing.id" class="flex items-baseline gap-2 py-1">
      <span class="font-medium text-stone-800">{{ ing.name }}</span>
      <span v-if="ing.amount" class="text-stone-600">{{ ing.amount }}{{ ing.unit ? ' ' + ing.unit : '' }}</span>
      <span v-if="ing.note" class="text-stone-400 text-sm italic">{{ ing.note }}</span>
    </li>
  </ul>
</template>

<script setup lang="ts">
import type { Ingredient } from '../api/recipes'
defineProps<{ ingredients: Ingredient[] }>()
</script>
