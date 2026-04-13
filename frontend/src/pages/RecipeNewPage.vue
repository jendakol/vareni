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
        <input type="file" accept="image/*" multiple @change="handleFiles"
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
      <RecipeForm :key="previewKey" :initial="preview" @save="handleSave" />
    </div>

  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { useToast } from 'vue-toastification'
import { ingest, createRecipe } from '../api/recipes'
import RecipeForm from '../components/RecipeForm.vue'

const router = useRouter()
const toast = useToast()
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

function handleFiles(e: Event) {
  const input = e.target as HTMLInputElement
  imageFiles.value = input.files ? Array.from(input.files) : []
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
  try {
    const recipe = await createRecipe(data)
    toast.success('Recept uložen')
    router.push(`/recipes/${recipe.id}`)
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se uložit recept')
  }
}
</script>
