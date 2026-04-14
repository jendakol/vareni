<template>
  <div>
    <div class="flex items-center justify-between mb-6">
      <h1 class="text-2xl font-bold text-stone-800">Recepty</h1>
      <router-link to="/recipes/new"
        class="px-4 py-2 bg-orange-600 text-white rounded-lg hover:bg-orange-700 font-medium">
        + Nový recept
      </router-link>
    </div>

    <!-- Tab toggle -->
    <div class="flex gap-2 mb-4">
      <button v-for="tab in tabs" :key="tab.key" @click="activeTab = tab.key; fetchRecipes()"
        class="px-4 py-2 rounded-full text-sm"
        :class="activeTab === tab.key ? 'bg-orange-600 text-white' : 'bg-stone-100 text-stone-600 hover:bg-stone-200'">
        {{ tab.label }}
        <span v-if="tab.key === 'discovered' && discoveredCount > 0"
          class="ml-1 bg-orange-200 text-orange-800 rounded-full px-2 text-xs">{{ discoveredCount }}</span>
      </button>
    </div>

    <!-- Discover section -->
    <div class="mb-6 space-y-2 sm:space-y-0 sm:flex sm:gap-2">
      <input v-model="discoverPrompt" placeholder="Najdi nové recepty... (např. 'ryba', 'těstoviny', 'rychlá večeře')"
        class="w-full sm:flex-1 px-4 py-2 border border-stone-300 rounded-lg"
        @keyup.enter="handleDiscover" />
      <button v-if="!discovering" @click="handleDiscover"
        class="w-full sm:w-auto px-4 py-2 bg-green-600 text-white rounded-lg hover:bg-green-700">
        Objevit nové
      </button>
      <button v-else @click="cancelDiscover"
        class="w-full sm:w-auto px-4 py-2 bg-red-500 text-white rounded-lg hover:bg-red-600">
        Zrušit
      </button>
    </div>

    <!-- Search and sort (only for "mine" tab) -->
    <template v-if="activeTab === 'mine'">
      <div class="mb-4">
        <input v-model="search" @input="debouncedFetch" placeholder="Hledat recepty..."
          class="w-full px-4 py-3 border border-stone-300 rounded-lg text-lg" />
      </div>

      <div class="flex gap-2 mb-4">
        <button v-for="s in sorts" :key="s.value" @click="sort = s.value; fetchRecipes()"
          class="px-3 py-1 rounded-full text-sm"
          :class="sort === s.value ? 'bg-orange-600 text-white' : 'bg-stone-100 text-stone-600 hover:bg-stone-200'">
          {{ s.label }}
        </button>
      </div>
    </template>

    <div v-if="store.loading" class="text-center text-stone-500 py-8">Načítám...</div>
    <div v-else-if="store.error" class="text-center py-8">
      <p class="text-red-600 font-medium">Chyba při načítání receptů</p>
      <button @click="fetchRecipes()" class="mt-2 text-orange-600 hover:underline text-sm">Zkusit znovu</button>
    </div>
    <div v-else-if="store.recipes.length === 0" class="text-center text-stone-400 py-8">
      {{ activeTab === 'mine' ? 'Žádné recepty' : activeTab === 'discovered' ? 'Žádné objevené recepty' : 'Žádné odmítnuté recepty' }}
    </div>
    <div v-else class="space-y-3">
      <RecipeCard v-for="recipe in store.recipes" :key="recipe.id" :recipe="recipe"
        @status="handleStatusChange" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useToast } from 'vue-toastification'
import { useRecipeStore } from '../stores/recipes'
import { updateRecipeStatus } from '../api/recipes'
import { discover } from '../api/discover'
import RecipeCard from '../components/RecipeCard.vue'

const store = useRecipeStore()
const toast = useToast()
const search = ref('')
const sort = ref('recent')
let debounceTimer: ReturnType<typeof setTimeout>

const activeTab = ref<'mine' | 'discovered' | 'rejected'>('mine')
const discoverPrompt = ref('')
const discovering = ref(false)
const discoveredCount = ref(0)
let discoverAbort: AbortController | null = null

const tabs = [
  { key: 'mine' as const, label: 'Moje recepty' },
  { key: 'discovered' as const, label: 'Objevené' },
  { key: 'rejected' as const, label: 'Odmítnuté' },
]

const sorts = [
  { value: 'recent', label: 'Nejnovější' },
  { value: 'least_cooked', label: 'Dlouho nevařené' },
  { value: 'prep_time', label: 'Nejrychlejší' },
]

function statusForTab(): string | undefined {
  switch (activeTab.value) {
    case 'mine': return 'saved,tested'
    case 'discovered': return 'discovered'
    case 'rejected': return 'rejected,rejected_similar'
  }
}

function fetchRecipes() {
  store.fetch({
    q: activeTab.value === 'mine' ? (search.value || undefined) : undefined,
    sort: activeTab.value === 'mine' ? sort.value : undefined,
    status: statusForTab(),
  })
}

function debouncedFetch() {
  clearTimeout(debounceTimer)
  debounceTimer = setTimeout(fetchRecipes, 300)
}

async function fetchDiscoveredCount() {
  try {
    const result = await import('../api/recipes').then(m => m.listRecipes({ status: 'discovered', page: 1 }))
    discoveredCount.value = result.total
  } catch {
    // Silently ignore — badge is non-critical
  }
}

function cancelDiscover() {
  if (discoverAbort) {
    discoverAbort.abort()
    discoverAbort = null
  }
  discovering.value = false
  toast.dismiss('discover-progress')
  toast.info('Hledání zrušeno')
}

async function handleDiscover() {
  discovering.value = true
  discoverAbort = new AbortController()
  toast.info('Hledám nové recepty...', { timeout: false, id: 'discover-progress' })
  try {
    const result = await discover({
      prompt: discoverPrompt.value || undefined,
      count: 5,
    }, discoverAbort.signal)
    toast.dismiss('discover-progress')

    const found = result.discovered.length
    const sk = result.skipped
    const skippedTotal = sk.duplicate + sk.restricted + sk.low_score + sk.similar_to_rejected + (sk.failed || 0)
    const parts: string[] = []
    if (sk.duplicate > 0) parts.push(`${sk.duplicate} duplicit`)
    if (sk.restricted > 0) parts.push(`${sk.restricted} omezení`)
    if (sk.low_score > 0) parts.push(`${sk.low_score} nízké skóre`)
    if (sk.similar_to_rejected > 0) parts.push(`${sk.similar_to_rejected} podobné odmítnutým`)
    if (sk.failed) parts.push(`${sk.failed} chyba zpracování`)

    if (found > 0) {
      let msg = `Nalezeno ${found} ${found === 1 ? 'nový recept' : found < 5 ? 'nové recepty' : 'nových receptů'}`
      if (skippedTotal > 0) msg += ` (přeskočeno ${skippedTotal}: ${parts.join(', ')})`
      toast.success(msg)
      activeTab.value = 'discovered'
      discoverPrompt.value = ''
      fetchRecipes()
    } else {
      let msg = 'Nenalezeno nic nového'
      if (skippedTotal > 0) msg += ` (přeskočeno ${skippedTotal}: ${parts.join(', ')})`
      toast.info(msg)
    }

    if (result.errors.length > 0) {
      const errorSites = result.errors.map(e => e.site).join(', ')
      toast.warning(`Chyby při scrapování: ${errorSites}`)
    }

    await fetchDiscoveredCount()
  } catch (e: any) {
    toast.dismiss('discover-progress')
    if (e.name !== 'AbortError') {
      toast.error(e.message || 'Nepodařilo se objevit recepty')
    }
  } finally {
    discovering.value = false
    discoverAbort = null
  }
}

async function handleStatusChange(id: string, status: string) {
  try {
    await updateRecipeStatus(id, status)
    await fetchRecipes()
    await fetchDiscoveredCount()
    if (status === 'saved') toast.success('Recept uložen')
    else if (status === 'discovered') toast.success('Recept obnoven')
    else toast.success('Recept odmítnut')
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se změnit stav')
  }
}

onMounted(() => {
  fetchRecipes()
  fetchDiscoveredCount()
})
</script>
