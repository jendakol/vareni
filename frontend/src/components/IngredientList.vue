<template>
  <div class="space-y-6">
    <div v-for="section in displaySections" :key="section.id">
      <div v-if="section.label" class="mb-3">
        <h4 class="text-xs font-semibold uppercase tracking-widest text-orange-600 border-b border-orange-200 pb-1">{{ section.label }}</h4>
        <p v-if="section.description" class="text-sm text-stone-500 italic mt-1">{{ section.description }}</p>
        <p v-if="showSectionTimes && (section.prep_time_min || section.cook_time_min)"
           class="text-xs text-stone-400 mt-0.5">
          <span v-if="section.prep_time_min">Příprava: {{ section.prep_time_min }} min</span>
          <span v-if="section.prep_time_min && section.cook_time_min"> · </span>
          <span v-if="section.cook_time_min">{{ cookLabel(section.cook_method) }}: {{ section.cook_time_min }} min</span>
        </p>
      </div>
      <ul class="space-y-2">
        <li v-for="ing in section.ingredients" :key="ing.id" class="flex items-baseline gap-2 py-1">
          <span class="font-medium text-stone-800">{{ ing.name }}</span>
          <span v-if="ing.amount" class="text-stone-600">{{ ing.amount }}{{ ing.unit ? ' ' + ing.unit : '' }}</span>
          <span v-if="ing.note" class="text-stone-400 text-sm italic">{{ ing.note }}</span>
        </li>
      </ul>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { CookMethod, Section } from '../api/recipes'

const props = defineProps<{ sections: Section[] }>()

const displaySections = computed(() => props.sections ?? [])

function cookLabel(method: CookMethod | null): string {
  switch (method) {
    case 'baking': return 'Pečení'
    case 'frying': return 'Smažení'
    case 'steaming': return 'Dušení'
    default: return 'Vaření'
  }
}

// "Single total" detection: only one section in the recipe has any time set.
// In that case, parent component shows recipe-level total; we suppress
// per-section time labels here.
const showSectionTimes = computed(() => {
  const withTime = displaySections.value.filter(
    s => s.prep_time_min != null || s.cook_time_min != null,
  )
  return withTime.length > 1
})
</script>
