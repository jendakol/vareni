<template>
  <div class="fixed inset-0 z-50 bg-black flex items-center justify-center select-none"
    role="dialog" aria-modal="true">
    <div class="absolute inset-0 flex items-center justify-center overflow-auto"
      style="touch-action: pinch-zoom"
      @pointerdown="onDown" @pointerup="onUp" @pointercancel="start = null">
      <img v-if="current" :key="current.id" :src="imageUrl(recipeId, current.id, 'full')"
        class="max-w-full max-h-full object-contain" draggable="false" alt="Screenshot" />
    </div>

    <div class="absolute top-0 inset-x-0 flex items-center justify-between p-2 text-white bg-gradient-to-b from-black/60 to-transparent">
      <span class="px-2 text-sm">{{ index + 1 }} / {{ images.length }}</span>
      <div class="flex gap-1">
        <button @click="remove" :disabled="busy" title="Smazat" aria-label="Smazat"
          class="w-11 h-11 rounded-full hover:bg-white/20 flex items-center justify-center disabled:opacity-50">
          <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/></svg>
        </button>
        <button @click="close" title="Zavřít" aria-label="Zavřít"
          class="w-11 h-11 rounded-full hover:bg-white/20 flex items-center justify-center text-xl">✕</button>
      </div>
    </div>

    <template v-if="images.length > 1">
      <button @click="go(-1)" :disabled="index === 0" aria-label="Předchozí"
        class="hidden sm:flex absolute left-3 top-1/2 -translate-y-1/2 w-11 h-11 rounded-full bg-white/20 text-white text-2xl items-center justify-center disabled:opacity-30">‹</button>
      <button @click="go(1)" :disabled="index === images.length - 1" aria-label="Další"
        class="hidden sm:flex absolute right-3 top-1/2 -translate-y-1/2 w-11 h-11 rounded-full bg-white/20 text-white text-2xl items-center justify-center disabled:opacity-30">›</button>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { useToast } from 'vue-toastification'
import type { ImageRef } from '../api/recipes'
import { imageUrl, deleteImage } from '../api/images'

const props = defineProps<{ recipeId: string; images: ImageRef[]; startIndex: number }>()
const emit = defineEmits<{ close: []; deleted: [imageId: string] }>()
const toast = useToast()

const index = ref(props.startIndex)
const busy = ref(false)
const current = computed(() => props.images[index.value])

// Close = pop the entry we pushed; popstate is the single place that emits close.
let closing = false
function close() {
  if (closing) return
  closing = true
  if (history.state?.lightbox) history.back()
  else emit('close')
}
function onPop() { closing = true; emit('close') }

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') close()
  else if (e.key === 'ArrowLeft') go(-1)
  else if (e.key === 'ArrowRight') go(1)
}

function go(d: number) {
  index.value = Math.min(props.images.length - 1, Math.max(0, index.value + d))
}

let start: { x: number; y: number } | null = null
function onDown(e: PointerEvent) { start = { x: e.clientX, y: e.clientY } }
function onUp(e: PointerEvent) {
  if (!start) return
  const dx = e.clientX - start.x
  const dy = e.clientY - start.y
  start = null
  // Zoomed in: the gesture belongs to panning
  if ((window.visualViewport?.scale ?? 1) > 1.01) return
  if (Math.abs(dx) > 50 && Math.abs(dx) > Math.abs(dy) * 1.5) go(dx < 0 ? 1 : -1)
}

async function remove() {
  const img = current.value
  if (!img || busy.value || !confirm('Smazat screenshot?')) return
  busy.value = true
  try {
    await deleteImage(props.recipeId, img.id)
    emit('deleted', img.id)
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se smazat screenshot')
  } finally {
    busy.value = false
  }
}

// The list shrinks after a delete: keep the index valid, close when empty.
watch(() => props.images.length, (n) => {
  if (n === 0) close()
  else if (index.value >= n) index.value = n - 1
})

let prevOverflow = ''
onMounted(() => {
  history.pushState({ ...history.state, lightbox: true }, '')
  window.addEventListener('popstate', onPop)
  window.addEventListener('keydown', onKey)
  prevOverflow = document.body.style.overflow
  document.body.style.overflow = 'hidden'
})

onBeforeUnmount(() => {
  window.removeEventListener('popstate', onPop)
  window.removeEventListener('keydown', onKey)
  document.body.style.overflow = prevOverflow
  // Unmounted by the parent while our entry is still on top: drop it
  if (history.state?.lightbox) history.back()
})
</script>
