<template>
  <div class="space-y-6">
    <div v-for="(section, sIdx) in displaySections" :key="section.id">
      <h4 v-if="section.label" class="text-xs font-semibold uppercase tracking-widest text-orange-600 border-b border-orange-200 pb-1 mb-3">{{ section.label }}</h4>
      <ol class="space-y-3">
        <li v-for="(step, stepIdx) in section.steps"
            :key="`${section.id}-${step.step_order}`"
            class="flex gap-3">
          <span class="flex-shrink-0 w-7 h-7 rounded-full bg-orange-100 text-orange-700 flex items-center justify-center text-sm font-medium">
            {{ continuousNumber(sIdx, stepIdx) }}
          </span>
          <p class="text-stone-700 pt-0.5">{{ step.instruction }}</p>
        </li>
      </ol>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { Section } from '../api/recipes'

const props = defineProps<{ sections: Section[] }>()

const displaySections = computed(() => props.sections ?? [])

// Continuous numbering across sections — display only.
function continuousNumber(sectionIdx: number, stepIdx: number): number {
  let n = stepIdx + 1
  for (let i = 0; i < sectionIdx; i++) {
    n += displaySections.value[i].steps.length
  }
  return n
}
</script>
