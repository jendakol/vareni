<template>
  <div>
    <div class="flex items-center justify-between mb-6">
      <h1 class="text-2xl font-bold text-stone-800">Objevit recepty</h1>
      <router-link to="/recipes" class="text-stone-500 hover:text-stone-700 text-sm">← Zpět na recepty</router-link>
    </div>

    <div class="space-y-2 sm:space-y-0 sm:flex sm:gap-2">
      <input v-model="discoverPrompt" placeholder="Najdi nové recepty... (např. 'ryba', 'těstoviny', 'rychlá večeře')"
        class="w-full sm:flex-1 px-4 py-2 border border-stone-300 rounded-lg"
        @keyup.enter="handleDiscover" />
      <button v-if="!discovering" @click="handleDiscover"
        class="w-full sm:w-auto px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700">
        Objevit nové
      </button>
      <button v-else @click="cancelDiscover"
        class="w-full sm:w-auto px-4 py-2 bg-red-500 text-white rounded-lg hover:bg-red-600">
        Zrušit
      </button>
    </div>

    <p v-if="discovering" class="mt-4 text-sm text-stone-500">
      Prohledávám recepty… může to trvat i minutu.
    </p>
  </div>
</template>

<script setup lang="ts">
import { ref, onBeforeUnmount } from 'vue'
import { useRouter } from 'vue-router'
import { useToast } from 'vue-toastification'
import { discover } from '../api/discover'
import { useRecipeStore } from '../stores/recipes'

const router = useRouter()
const toast = useToast()
const store = useRecipeStore()
const discoverPrompt = ref('')
const discovering = ref(false)
let discoverAbort: AbortController | null = null

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

    if (result.errors.length > 0) {
      const errorSites = result.errors.map(e => e.site).join(', ')
      toast.warning(`Chyby při scrapování: ${errorSites}`)
    }

    if (found > 0) {
      let msg = `Nalezeno ${found} ${found === 1 ? 'nový recept' : found < 5 ? 'nové recepty' : 'nových receptů'}`
      if (skippedTotal > 0) msg += ` (přeskočeno ${skippedTotal}: ${parts.join(', ')})`
      toast.success(msg)
      // Mark stale before navigating: the list re-reads the tab in onActivated,
      // but only refetches when the tab actually *changed*. A user who was
      // already on Objevene would otherwise land back on a stale list with a
      // badge counting recipes it does not show.
      store.markStale()
      router.push({ path: '/recipes', query: { tab: 'discovered' } })
    } else {
      let msg = 'Nenalezeno nic nového'
      if (skippedTotal > 0) msg += ` (přeskočeno ${skippedTotal}: ${parts.join(', ')})`
      toast.info(msg)
    }
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

// The progress toast has timeout:false and this component owns the only abort
// handle. Leaving the page without this would strand a sticky toast on unrelated
// screens with no visible way to cancel the run behind it.
onBeforeUnmount(() => {
  discoverAbort?.abort()
  discoverAbort = null
  toast.dismiss('discover-progress')
})
</script>
