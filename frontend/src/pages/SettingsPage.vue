<template>
  <div>
    <h1 class="text-2xl font-bold text-stone-800 mb-6">Nastavení</h1>

    <section class="bg-white rounded-xl border border-stone-200 p-4 mb-6">
      <h2 class="font-semibold text-stone-700 mb-3">Dietní omezení</h2>
      <div class="flex flex-wrap gap-2 mb-3">
        <span v-for="r in restrictions" :key="r"
          class="px-3 py-1 bg-orange-100 text-orange-700 rounded-full text-sm flex items-center gap-1">
          {{ r }}
          <button @click="removeRestriction(r)" class="hover:text-red-600">✕</button>
        </span>
      </div>
      <div class="flex gap-2">
        <input v-model="newRestriction" placeholder="Např. vegetarián, bezlepkové..."
          class="flex-1 px-3 py-2 border border-stone-300 rounded-lg" @keyup.enter="addRestriction" />
        <button @click="addRestriction" class="px-4 py-2 bg-orange-600 text-white rounded-lg">Přidat</button>
      </div>
    </section>

    <section class="bg-white rounded-xl border border-stone-200 p-4 mb-6">
      <h2 class="font-semibold text-stone-700 mb-3">Preference</h2>
      <div class="flex flex-wrap gap-2 mb-3">
        <span v-for="p in preferences" :key="p"
          class="px-3 py-1 bg-green-100 text-green-700 rounded-full text-sm flex items-center gap-1">
          {{ p }}
          <button @click="removePreference(p)" class="hover:text-red-600">✕</button>
        </span>
      </div>
      <div class="flex gap-2">
        <input v-model="newPreference" placeholder="Např. ryby, lehká jídla, česká kuchyně..."
          class="flex-1 px-3 py-2 border border-stone-300 rounded-lg" @keyup.enter="addPreference" />
        <button @click="addPreference" class="px-4 py-2 bg-green-600 text-white rounded-lg">Přidat</button>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useToast } from 'vue-toastification'
import { useAuthStore } from '../stores/auth'
import { apiFetch } from '../api/client'

const authStore = useAuthStore()
const toast = useToast()
const restrictions = ref<string[]>([])
const preferences = ref<string[]>([])
const newRestriction = ref('')
const newPreference = ref('')

async function loadSettings() {
  try {
    await authStore.fetchMe()
    restrictions.value = authStore.user?.dietary_restrictions || []
    preferences.value = authStore.user?.food_preferences || []
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se načíst nastavení')
  }
}

async function addRestriction() {
  if (!newRestriction.value.trim()) return
  try {
    await apiFetch('/settings/restrictions', {
      method: 'POST',
      body: JSON.stringify({ restriction: newRestriction.value.trim() }),
    })
    newRestriction.value = ''
    await loadSettings()
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se přidat omezení')
  }
}

async function removeRestriction(r: string) {
  try {
    await apiFetch('/settings/restrictions', {
      method: 'DELETE',
      body: JSON.stringify({ restriction: r }),
    })
    await loadSettings()
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se odebrat omezení')
  }
}

async function addPreference() {
  if (!newPreference.value.trim()) return
  try {
    await apiFetch('/settings/preferences', {
      method: 'POST',
      body: JSON.stringify({ preference: newPreference.value.trim() }),
    })
    newPreference.value = ''
    await loadSettings()
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se přidat preferenci')
  }
}

async function removePreference(p: string) {
  try {
    await apiFetch('/settings/preferences', {
      method: 'DELETE',
      body: JSON.stringify({ preference: p }),
    })
    await loadSettings()
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se odebrat preferenci')
  }
}

onMounted(loadSettings)
</script>
