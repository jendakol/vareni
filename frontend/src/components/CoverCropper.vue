<template>
  <div class="fixed inset-0 z-50 bg-black flex flex-col select-none" role="dialog" aria-modal="true" aria-label="Oříznout fotku">
    <div class="flex-1 min-h-0" style="touch-action: none; overscroll-behavior: contain">
      <Cropper ref="cropper" class="h-full w-full" :src="url" :stencil-props="{ aspectRatio: 1 }"
        :stencil-component="RectangleStencil" image-restriction="stencil"
        :canvas="{ maxWidth: 1600, maxHeight: 1600 }" />
    </div>
    <div class="flex justify-end gap-2 p-3 bg-black">
      <button @click="close" :disabled="busy"
        class="min-h-11 px-4 rounded-lg bg-white/20 text-white text-sm hover:bg-white/30">Zrušit</button>
      <button @click="apply" :disabled="busy"
        class="min-h-11 px-4 rounded-lg bg-orange-600 text-white text-sm hover:bg-orange-700 disabled:opacity-50">Použít</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue'
import { Cropper, RectangleStencil } from 'vue-advanced-cropper'
import 'vue-advanced-cropper/dist/style.css'
import { looksBlank, flattenOnWhite } from '../lib/resizeImage'

const props = defineProps<{ file: File }>()
const emit = defineEmits<{ done: [file: File]; cancel: [] }>()

const cropper = ref<InstanceType<typeof Cropper> | null>(null)
const url = URL.createObjectURL(props.file)
const busy = ref(false)

// Close = pop the entry we pushed; popstate is the single place that emits cancel.
let closing = false
function close() {
  if (closing) return
  closing = true
  if (history.state?.coverCropper) history.back()
  else emit('cancel')
}
function onPop() { closing = true; emit('cancel') }
function onKey(e: KeyboardEvent) { if (e.key === 'Escape') close() }

async function apply() {
  const result = cropper.value?.getResult()
  const canvas = result?.canvas
  if (!canvas || busy.value) return
  busy.value = true
  try {
    const ctx = canvas.getContext('2d')
    if (!ctx || looksBlank(ctx, canvas.width, canvas.height)) return finish(props.file)
    flattenOnWhite(ctx, canvas.width, canvas.height)
    const blob = await new Promise<Blob | null>((r) => canvas.toBlob(r, 'image/jpeg', 0.92))
    const name = props.file.name.replace(/\.[^.]+$/, '') + '.jpg'
    finish(blob ? new File([blob], name, { type: 'image/jpeg' }) : props.file)
  } catch {
    finish(props.file)
  }
}

// Emit without going through close(): the parent unmounts us and the entry is dropped there.
function finish(f: File) {
  closing = true
  emit('done', f)
}

let prevOverflow = ''
onMounted(() => {
  history.pushState({ ...history.state, coverCropper: true }, '')
  window.addEventListener('popstate', onPop)
  window.addEventListener('keydown', onKey)
  prevOverflow = document.body.style.overflow
  document.body.style.overflow = 'hidden'
})

onBeforeUnmount(() => {
  window.removeEventListener('popstate', onPop)
  window.removeEventListener('keydown', onKey)
  document.body.style.overflow = prevOverflow
  URL.revokeObjectURL(url)
  if (history.state?.coverCropper) history.back()
})
</script>
