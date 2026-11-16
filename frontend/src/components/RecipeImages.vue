<template>
  <div>
    <!-- Cover -->
    <div v-if="part !== 'screenshots'" class="relative aspect-square w-full sm:max-w-xs rounded-xl overflow-hidden bg-stone-100 mb-4">
      <img v-if="recipe.cover_image_id" :src="imageUrl(recipe.id, recipe.cover_image_id, 'full')"
        alt="" class="w-full h-full object-cover" />
      <button v-else @click="coverInput?.click()" :disabled="!!coverTile"
        class="w-full h-full flex items-center justify-center text-stone-500 hover:bg-stone-200 text-lg">
        📷 Přidat fotku
      </button>

      <div v-if="recipe.cover_image_id && !coverTile" class="absolute bottom-2 right-2 flex gap-2">
        <button @click="coverInput?.click()"
          class="min-h-11 px-4 rounded-lg bg-black/60 text-white text-sm hover:bg-black/75">Změnit</button>
        <button @click="removeCover" aria-label="Odebrat fotku"
          class="min-h-11 min-w-11 rounded-lg bg-black/60 text-white hover:bg-black/75">✕</button>
      </div>

      <div v-if="coverTile" class="absolute inset-0 bg-stone-900">
        <img :src="coverTile.previewUrl" alt="" class="w-full h-full object-cover opacity-40" />
        <div class="absolute inset-0 flex flex-col items-center justify-center gap-3 p-4 text-white text-center">
          <template v-if="coverTile.error">
            <p class="text-sm">{{ coverTile.error }}</p>
            <div class="flex gap-2">
              <button @click="run(coverTile)" class="min-h-11 px-4 rounded-lg bg-orange-600 hover:bg-orange-700 text-sm">Zkusit znovu</button>
              <button @click="discard(coverTile)" class="min-h-11 px-4 rounded-lg bg-white/20 hover:bg-white/30 text-sm">✕ zahodit</button>
            </div>
          </template>
          <template v-else>
            <p class="text-sm">Nahrávám…</p>
            <div class="w-2/3 h-2 bg-white/30 rounded-full overflow-hidden">
              <div class="h-full bg-orange-500" :style="{ width: Math.round(coverTile.progress * 100) + '%' }"></div>
            </div>
          </template>
        </div>
      </div>
    </div>

    <!-- Source screenshots -->
    <div v-if="part !== 'cover'" class="mb-4">
      <div v-if="pendingFiles.length"
        class="flex items-center justify-between gap-2 mb-2 p-3 rounded-lg bg-amber-50 border border-amber-200 text-sm text-amber-800">
        <span>Některé fotky zdroje nebyly uloženy –</span>
        <button @click="retryPending" class="font-medium underline min-h-11 px-2">Zkusit znovu</button>
      </div>

      <h2 class="text-sm font-medium text-stone-500 mb-2">Původní zdroj</h2>
      <div class="flex flex-wrap gap-2">
        <button v-for="(s, i) in recipe.screenshots || []" :key="s.id" @click="lightboxIndex = i"
          class="w-20 h-20 rounded-lg overflow-hidden bg-stone-100 shrink-0">
          <img :src="imageUrl(recipe.id, s.id, 'thumb')" alt="Screenshot" loading="lazy" class="w-full h-full object-cover object-top" />
        </button>

        <div v-for="t in shotTiles" :key="t.key"
          class="relative w-20 h-20 rounded-lg overflow-hidden bg-stone-800 shrink-0">
          <img :src="t.previewUrl" alt="" class="w-full h-full object-cover object-top opacity-40" />
          <div v-if="!t.error" class="absolute inset-x-2 bottom-2 h-1.5 bg-white/30 rounded-full overflow-hidden">
            <div class="h-full bg-orange-500" :style="{ width: Math.round(t.progress * 100) + '%' }"></div>
          </div>
          <div v-else class="absolute inset-0 flex flex-col items-center justify-center gap-0.5 p-1 text-white text-center text-[10px] leading-tight bg-red-900/70">
            <span class="line-clamp-2" :title="t.error">{{ t.error }}</span>
            <button @click="run(t)" class="underline">Zkusit znovu</button>
            <button @click="discard(t)" class="underline">✕ zahodit</button>
          </div>
        </div>

        <button @click="shotInput?.click()"
          class="min-h-20 h-20 px-3 rounded-lg border-2 border-dashed border-stone-300 text-stone-500 text-sm hover:border-orange-400 hover:text-orange-600">
          + screenshot
        </button>
      </div>
    </div>

    <input v-if="part !== 'screenshots'" ref="coverInput" type="file" :accept="PICKER_ACCEPT" class="hidden" @change="onCover" />
    <input v-if="part !== 'cover'" ref="shotInput" type="file" :accept="PICKER_ACCEPT" multiple class="hidden" @change="onShots" />

    <CoverCropper v-if="cropFile" :file="cropFile" @done="onCropped" @cancel="cropFile = null" />

    <ImageLightbox v-if="lightboxIndex !== null"
      :recipe-id="recipe.id" :images="recipe.screenshots || []" :start-index="lightboxIndex"
      @close="lightboxIndex = null" @deleted="emit('changed')" />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onBeforeUnmount, defineAsyncComponent } from 'vue'
import { useToast } from 'vue-toastification'
import type { Recipe } from '../api/recipes'
import { imageUrl, uploadImages, deleteImage, PICKER_ACCEPT, prepareFiles } from '../api/images'
import { resizeForUpload } from '../lib/resizeImage'
import { getPending, clearPending } from '../lib/pendingUploads'
import { useLeaveGuard } from '../lib/leaveGuard'
import ImageLightbox from './ImageLightbox.vue'

const CoverCropper = defineAsyncComponent(() => import('./CoverCropper.vue'))

interface Tile {
  key: number
  kind: 'cover' | 'screenshot'
  file: File
  previewUrl: string
  progress: number
  error: string | null
  running: boolean
}

const props = defineProps<{ recipe: Recipe; part?: 'cover' | 'screenshots' }>()
const emit = defineEmits<{ changed: [] }>()
const toast = useToast()

const coverInput = ref<HTMLInputElement | null>(null)
const shotInput = ref<HTMLInputElement | null>(null)
const lightboxIndex = ref<number | null>(null)
const cropFile = ref<File | null>(null)
const tiles = ref<Tile[]>([])
const pendingFiles = ref<File[]>(props.part === 'cover' ? [] : getPending(props.recipe.id))
let seq = 0

useLeaveGuard(computed(() => tiles.value.some((t) => t.error)))

const coverTile = computed(() => tiles.value.find((t) => t.kind === 'cover'))
const shotTiles = computed(() => tiles.value.filter((t) => t.kind === 'screenshot'))

function accept(files: File[]): Promise<File[]> {
  return prepareFiles(files, (m) => toast.error(m))
}

function addTiles(kind: Tile['kind'], files: File[]): Tile[] {
  const added = files.map((file) => {
    tiles.value.push({ key: ++seq, kind, file, previewUrl: URL.createObjectURL(file), progress: 0, error: null, running: false })
    return tiles.value[tiles.value.length - 1] // reactive proxy
  })
  return added
}

async function run(tile: Tile) {
  if (tile.running) return
  tile.running = true
  tile.error = null
  tile.progress = 0
  try {
    const body = tile.kind === 'cover' ? await resizeForUpload(tile.file) : tile.file
    await uploadImages(props.recipe.id, tile.kind, [body], (f) => (tile.progress = f))
    drop(tile)
    emit('changed')
  } catch (e: any) {
    tile.error = e?.status === 401
      ? 'Přihlášení vypršelo – přihlaste se v jiné záložce a zkuste znovu'
      : e.message || 'Nahrávání se nezdařilo'
  } finally {
    tile.running = false
  }
}

function drop(tile: Tile) {
  URL.revokeObjectURL(tile.previewUrl)
  tiles.value = tiles.value.filter((t) => t.key !== tile.key)
}
const discard = drop

async function onCover(e: Event) {
  const input = e.target as HTMLInputElement
  const picked = [...(input.files ?? [])]
  input.value = ''
  const files = await accept(picked)
  if (!files.length || coverTile.value) return
  cropFile.value = files[0]
}

async function onCropped(file: File) {
  cropFile.value = null
  if (coverTile.value) return
  await run(addTiles('cover', [file])[0])
}

async function onShots(e: Event) {
  const input = e.target as HTMLInputElement
  const picked = [...(input.files ?? [])]
  input.value = ''
  const files = await accept(picked)
  await uploadAll(files)
}

async function uploadAll(files: File[]) {
  for (const t of addTiles('screenshot', files)) await run(t)
}

async function retryPending() {
  const files = pendingFiles.value
  pendingFiles.value = []
  clearPending(props.recipe.id) // the tiles hold the files from here on
  await uploadAll(files)
}

async function removeCover() {
  if (!props.recipe.cover_image_id || !confirm('Odebrat fotku receptu?')) return
  try {
    await deleteImage(props.recipe.id, props.recipe.cover_image_id)
    emit('changed')
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se odebrat fotku')
  }
}

onBeforeUnmount(() => tiles.value.forEach((t) => URL.revokeObjectURL(t.previewUrl)))
</script>
