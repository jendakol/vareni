<template>
  <div>
    <h1 class="text-2xl font-bold text-stone-800 mb-6">Nový recept</h1>

    <!-- Source tabs -->
    <div v-if="!preview" class="flex border-b border-stone-200 mb-6">
      <button v-for="tab in tabs" :key="tab.key" @click="activeTab = tab.key"
        class="px-4 py-2 -mb-px font-medium text-sm"
        :class="activeTab === tab.key
          ? 'border-b-2 border-orange-600 text-orange-600'
          : 'text-stone-500 hover:text-stone-700'">
        {{ tab.label }}
      </button>
    </div>

    <!-- Input forms -->
    <div v-if="!preview">
      <div v-if="activeTab === 'manual'" class="space-y-4">
        <textarea v-model="textInput" rows="8" placeholder="Vloz recept jako text..."
          class="w-full px-4 py-3 border border-stone-300 rounded-lg" />
        <button @click="handleIngest" :disabled="loading" class="px-6 py-3 bg-orange-600 text-white rounded-lg hover:bg-orange-700 disabled:opacity-50">
          {{ loading ? 'Zpracovávám...' : 'Zpracovat' }}
        </button>
      </div>

      <div v-if="activeTab === 'photo'" class="space-y-4">
        <input type="file" :accept="PICKER_ACCEPT" multiple @change="handleFiles"
          class="block w-full text-sm text-stone-500 file:mr-4 file:py-2 file:px-4 file:rounded-lg file:border-0 file:bg-orange-50 file:text-orange-700 file:font-medium" />
        <p v-if="imageFiles.length > 0" class="text-sm text-stone-500">
          {{ imageFiles.length }} {{ imageFiles.length === 1 ? 'fotka' : imageFiles.length < 5 ? 'fotky' : 'fotek' }}
        </p>
        <button @click="handleIngest" :disabled="loading || imageFiles.length === 0" class="px-6 py-3 bg-orange-600 text-white rounded-lg hover:bg-orange-700 disabled:opacity-50">
          {{ loading ? 'Zpracovávám...' : 'Zpracovat' }}
        </button>
      </div>

      <div v-if="activeTab === 'url'" class="space-y-4">
        <input v-model="urlInput" type="url" placeholder="https://..."
          class="w-full px-4 py-3 border border-stone-300 rounded-lg" />
        <button @click="handleIngest" :disabled="loading" class="px-6 py-3 bg-orange-600 text-white rounded-lg hover:bg-orange-700 disabled:opacity-50">
          {{ loading ? 'Zpracovávám...' : 'Zpracovat' }}
        </button>
      </div>
    </div>

    <!-- Preview / Edit form -->
    <div v-if="preview">
      <div class="flex items-center justify-between mb-4">
        <h2 class="text-lg font-semibold text-stone-700">Náhled</h2>
        <button @click="preview = null" class="text-stone-500 hover:text-stone-700 text-sm">← Zpět</button>
      </div>
      <div v-if="coverUrl" class="mb-4">
        <div class="aspect-square w-full sm:max-w-xs overflow-hidden rounded-lg bg-stone-100" :class="{ 'opacity-40': dropCover }">
          <img :src="coverUrl" referrerpolicy="no-referrer" alt="Úvodní fotka z webu"
            class="w-full h-full object-cover" @error="coverBroken = true" />
        </div>
        <button type="button" @click="dropCover = !dropCover"
          class="mt-2 min-h-[44px] px-4 text-sm font-medium text-stone-600 border border-stone-300 rounded-lg hover:bg-stone-50">
          {{ dropCover ? 'Použít fotku' : 'Nepoužít fotku' }}
        </button>
      </div>
      <RecipeForm :key="previewKey" :initial="preview" @save="handleSave" :saving="saveLabel" />
    </div>

  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useRouter } from 'vue-router'
import { useToast } from 'vue-toastification'
import { ingest, createRecipe } from '../api/recipes'
import { PICKER_ACCEPT, prepareFiles, uploadImages } from '../api/images'
import { setPending } from '../lib/pendingUploads'
import { useLeaveGuard } from '../lib/leaveGuard'
import RecipeForm from '../components/RecipeForm.vue'
import { useRecipeStore } from '../stores/recipes'

const router = useRouter()
const toast = useToast()
const store = useRecipeStore()
const activeTab = ref('manual')
const tabs = [
  { key: 'manual', label: 'Napsat' },
  { key: 'photo', label: 'Fotka' },
  { key: 'url', label: 'Web' },
]

const textInput = ref('')
const urlInput = ref('')
const imageFiles = ref<File[]>([])
const preview = ref<any>(null)
const previewKey = ref(0)
const loading = ref(false)
const saveLabel = ref<string | null>(null)
useLeaveGuard(computed(() => !!saveLabel.value))
const dropCover = ref(false)
const coverBroken = ref(false)
const coverUrl = computed<string | null>(() =>
  coverBroken.value ? null : preview.value?.cover_image_url || null,
)

async function handleFiles(e: Event) {
  const input = e.target as HTMLInputElement
  const files = input.files ? Array.from(input.files) : []
  const ok = await prepareFiles(files, (m) => toast.error(m))
  imageFiles.value = ok
  if (ok.length !== files.length) input.value = ''
}

async function handleIngest() {
  loading.value = true
  const toastId = activeTab.value === 'photo'
    ? toast.info('Zpracovávám fotku...', { timeout: false })
    : activeTab.value === 'url'
      ? toast.info(
          urlInput.value.includes('instagram.com')
            ? 'Importuji z Instagramu...'
            : 'Stahuji recept...',
          { timeout: false }
        )
      : null
  try {
    const form = new FormData()
    form.append('source_type', activeTab.value)
    if (activeTab.value === 'manual') form.append('text', textInput.value)
    if (activeTab.value === 'photo') {
      for (const file of imageFiles.value) {
        form.append('image', file)
      }
    }
    if (activeTab.value === 'url') form.append('url', urlInput.value)

    const result = await ingest(form)
    result.source_type = activeTab.value
    if (activeTab.value === 'url') {
      result.source_url = urlInput.value
    }
    preview.value = result
    dropCover.value = false
    coverBroken.value = false
    previewKey.value++
    toast.success('Recept zpracován')
  } catch (e: any) {
    toast.error(e.message || 'Zpracování selhalo')
  } finally {
    if (toastId !== null) toast.dismiss(toastId)
    loading.value = false
  }
}

async function handleSave(data: any) {
  if (saveLabel.value) return
  saveLabel.value = 'Ukládám…'
  try {
    const photoFiles = preview.value?.source_type === 'photo' ? [...imageFiles.value] : []
    const withCover = coverUrl.value && !dropCover.value
    const recipe = await createRecipe(withCover ? { ...data, cover_image_url: coverUrl.value } : data)
    // The cached list cannot patch this in — where the new recipe belongs under
    // the current sort is unknown — so it reloads on its next activation.
    store.markStale()
    toast.success('Recept uložen')
    if (photoFiles.length) {
      // Registered up front so the files survive navigation mid-upload
      setPending(recipe.id, photoFiles)
      // One request per file: the server commits each screenshot as it goes,
      // so a failed batch could not tell which photos were already saved.
      let remaining = photoFiles
      try {
        while (remaining.length) {
          saveLabel.value = `Nahrávám fotky (${photoFiles.length - remaining.length + 1}/${photoFiles.length})…`
          await uploadImages(recipe.id, 'screenshot', [remaining[0]])
          remaining = remaining.slice(1)
          setPending(recipe.id, remaining)
        }
      } catch {
        toast.warning('Některé originální fotky se nepodařilo nahrát')
      }
    }
    router.push(`/recipes/${recipe.id}`)
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se uložit recept')
  } finally {
    saveLabel.value = null
  }
}
</script>
