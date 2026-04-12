<template>
  <div class="fixed inset-0 bg-stone-900 text-white z-50 flex flex-col" @click="next" @touchstart="handleTouch">
    <div class="flex items-center justify-between p-4">
      <span class="text-stone-400">Krok {{ current + 1 }} / {{ steps.length }}</span>
      <button @click.stop="$emit('close')" class="text-stone-400 hover:text-white p-2 text-xl">✕</button>
    </div>
    <div class="flex-1 flex items-center justify-center px-8">
      <p class="text-2xl sm:text-3xl leading-relaxed text-center font-light">
        {{ steps[current]?.instruction }}
      </p>
    </div>
    <div class="flex justify-center gap-2 pb-8">
      <div v-for="(_, i) in steps" :key="i"
        class="w-3 h-3 rounded-full"
        :class="i === current ? 'bg-orange-500' : 'bg-stone-600'" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import type { Step } from '../api/recipes'

const props = defineProps<{ steps: Step[] }>()
defineEmits<{ close: [] }>()

const current = ref(0)
let touchStartX = 0

function next() {
  if (current.value < props.steps.length - 1) current.value++
}

function handleTouch(e: TouchEvent) {
  touchStartX = e.touches[0].clientX
  const handler = (e2: TouchEvent) => {
    const diff = e2.changedTouches[0].clientX - touchStartX
    if (Math.abs(diff) > 50) {
      if (diff > 0 && current.value > 0) current.value--
      else if (diff < 0 && current.value < props.steps.length - 1) current.value++
    }
    document.removeEventListener('touchend', handler)
  }
  document.addEventListener('touchend', handler)
}
</script>
