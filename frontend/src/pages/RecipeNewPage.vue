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
        <input type="file" accept="image/*" capture="environment" @change="handleFile"
          class="block w-full text-sm text-stone-500 file:mr-4 file:py-2 file:px-4 file:rounded-lg file:border-0 file:bg-orange-50 file:text-orange-700 file:font-medium" />
        <button @click="handleIngest" :disabled="loading || !imageFile" class="px-6 py-3 bg-orange-600 text-white rounded-lg hover:bg-orange-700 disabled:opacity-50">
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
      <RecipeForm :initial="preview" @save="handleSave" />
    </div>

    <div v-if="error" class="mt-4 bg-red-50 text-red-700 p-3 rounded-lg text-sm">{{ error }}</div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { ingest, createRecipe } from '../api/recipes'
import RecipeForm from '../components/RecipeForm.vue'

const router = useRouter()
const activeTab = ref('manual')
const tabs = [
  { key: 'manual', label: 'Napsat' },
  { key: 'photo', label: 'Fotka' },
  { key: 'url', label: 'Web' },
]

const textInput = ref('')
const urlInput = ref('')
const imageFile = ref<File | null>(null)
const preview = ref<any>(null)
const loading = ref(false)
const error = ref('')

function handleFile(e: Event) {
  const input = e.target as HTMLInputElement
  imageFile.value = input.files?.[0] || null
}

async function handleIngest() {
  loading.value = true
  error.value = ''
  try {
    const form = new FormData()
    form.append('source_type', activeTab.value)
    if (activeTab.value === 'manual') form.append('text', textInput.value)
    if (activeTab.value === 'photo' && imageFile.value) form.append('image', imageFile.value)
    if (activeTab.value === 'url') form.append('url', urlInput.value)

    preview.value = await ingest(form)
    preview.value.source_type = activeTab.value
  } catch (e: any) {
    error.value = e.message
  } finally {
    loading.value = false
  }
}

async function handleSave(data: any) {
  try {
    const recipe = await createRecipe(data)
    router.push(`/recipes/${recipe.id}`)
  } catch (e: any) {
    error.value = e.message
  }
}
</script>
