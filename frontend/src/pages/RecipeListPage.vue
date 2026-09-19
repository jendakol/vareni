<template>
  <div>
    <div class="flex items-center justify-between mb-6">
      <h1 class="text-2xl font-bold text-stone-800">Recepty</h1>
      <div class="flex gap-2">
        <router-link to="/recipes/discover"
          class="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 font-medium">
          ✨ Objevit
        </router-link>
        <router-link to="/recipes/new"
          class="px-4 py-2 bg-orange-600 text-white rounded-lg hover:bg-orange-700 font-medium">
          + Nový recept
        </router-link>
      </div>
    </div>

    <!-- Tab toggle -->
    <div class="flex gap-2 mb-4">
      <button v-for="tab in tabs" :key="tab.key" @click="activeTab = tab.key; fetchRecipes()"
        class="px-4 py-2 rounded-full text-sm"
        :class="activeTab === tab.key ? 'bg-orange-600 text-white' : 'bg-stone-100 text-stone-600 hover:bg-stone-200'">
        {{ tab.label }}
        <span v-if="tab.key === 'discovered' && discoveredCount > 0"
          class="ml-1 bg-blue-200 text-blue-800 rounded-full px-2 text-xs">{{ discoveredCount }}</span>
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

    <div v-if="store.loading && store.recipes.length === 0" class="text-center text-stone-500 py-8">Načítám...</div>
    <div v-else-if="store.error && store.recipes.length === 0" class="text-center py-8">
      <p class="text-red-600 font-medium">Chyba při načítání receptů</p>
      <button @click="fetchRecipes()" class="mt-2 text-orange-600 hover:underline text-sm">Zkusit znovu</button>
    </div>
    <div v-else-if="!store.loading && store.recipes.length === 0" class="text-center text-stone-400 py-8">
      {{ activeTab === 'mine' ? 'Žádné recepty' : activeTab === 'discovered' ? 'Žádné objevené recepty' : 'Žádné odmítnuté recepty' }}
    </div>
    <div v-else class="space-y-3">
      <RecipeCard v-for="recipe in store.recipes" :key="recipe.id" :recipe="recipe"
        @status="handleStatusChange" />
      <div ref="sentinel" class="h-px"></div>
      <p v-if="store.loadingMore" class="text-center text-stone-500 py-4">Načítám…</p>
      <div v-else-if="store.moreError" class="text-center py-4">
        <p class="text-red-600 text-sm">Další recepty se nepodařilo načíst</p>
        <button @click="store.fetchMore()" class="mt-1 text-orange-600 hover:underline text-sm">Zkusit znovu</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
// Required for <KeepAlive :include="['RecipeListPage']"> in App.vue to match.
defineOptions({ name: 'RecipeListPage' })

import { ref, watch, nextTick, onMounted, onActivated, onUnmounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useToast } from 'vue-toastification'
import { useRecipeStore } from '../stores/recipes'
import { updateRecipeStatus } from '../api/recipes'
import RecipeCard from '../components/RecipeCard.vue'

const route = useRoute()
const router = useRouter()
const store = useRecipeStore()
const toast = useToast()
const search = ref('')
const sort = ref('recent')
let debounceTimer: ReturnType<typeof setTimeout>

const validTabs = ['mine', 'discovered', 'rejected'] as const
type Tab = typeof validTabs[number]
const initialTab = validTabs.includes(route.query.tab as Tab) ? (route.query.tab as Tab) : 'mine'
const activeTab = ref<Tab>(initialTab)
const discoveredCount = ref(0)

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

const sentinel = ref<HTMLElement | null>(null)
let observer: IntersectionObserver | null = null

// Re-observe whenever the sentinel is created or destroyed by a v-if branch flip.
watch(sentinel, (el, prev) => {
  if (prev) observer?.unobserve(prev)
  if (el) observer?.observe(el)
})

watch(activeTab, (tab) => {
  router.replace({ query: tab === 'mine' ? {} : { tab } })
})

function statusForTab(): string | undefined {
  switch (activeTab.value) {
    case 'mine': return 'saved,tested'
    case 'discovered': return 'discovered'
    case 'rejected': return 'rejected,rejected_similar'
  }
}

function fetchRecipes() {
  return store.fetch({
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

async function handleStatusChange(id: string, status: string) {
  try {
    const updated = await updateRecipeStatus(id, status)
    // Patch or drop the single card. A refetch here would reset the scroll
    // position, which is the one thing this page exists to preserve.
    if (statusForTab()?.split(',').includes(status)) {
      store.patchRecipe(id, updated)
    } else {
      store.removeRecipe(id)
    }
    await fetchDiscoveredCount()
    if (status === 'saved') toast.success('Recept uložen')
    else if (status === 'discovered') toast.success('Recept obnoven')
    else toast.success('Recept odmítnut')
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se změnit stav')
  }
}

onMounted(() => {
  observer = new IntersectionObserver(
    (entries) => { if (entries[0].isIntersecting) store.fetchMore() },
    { rootMargin: '400px' },
  )
  if (sentinel.value) observer.observe(sentinel.value)
  fetchRecipes()
})

// KeepAlive means setup() never runs again, so the tab encoded in the URL has to
// be re-read here — otherwise /recipes?tab=discovered changes the address bar and
// nothing else, and the URL then describes a screen the user is not looking at.
onActivated(() => {
  const tab = validTabs.includes(route.query.tab as Tab) ? (route.query.tab as Tab) : 'mine'
  if (tab !== activeTab.value) {
    activeTab.value = tab
    fetchRecipes()
  } else if (store.stale) {
    // A recipe was created elsewhere; its position under the current sort is
    // unknown, so this is the one mutation that cannot be patched in — the list
    // reloads from page 1 and the earlier scroll offset stops meaning anything.
    //
    // The router restores that offset on a nextTick after this hook, and the
    // reload empties the list inside the same flush, so the restore would clamp
    // against a one-line spinner and dump the user at the top anyway. Go there
    // deliberately once the new first page has rendered, rather than arriving by
    // accident mid-restore.
    fetchRecipes().then(() => nextTick()).then(() => window.scrollTo({ top: 0 }))
  }
  fetchDiscoveredCount()
})

onUnmounted(() => {
  observer?.disconnect()
  observer = null
})
</script>
