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
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useAuthStore } from '../stores/auth'
import { apiFetch } from '../api/client'

const authStore = useAuthStore()
const restrictions = ref<string[]>([])
const newRestriction = ref('')

async function loadRestrictions() {
  await authStore.fetchMe()
  restrictions.value = authStore.user?.dietary_restrictions || []
}

async function addRestriction() {
  if (!newRestriction.value.trim()) return
  await apiFetch('/settings/restrictions', {
    method: 'POST',
    body: JSON.stringify({ restriction: newRestriction.value.trim() }),
  })
  newRestriction.value = ''
  await loadRestrictions()
}

async function removeRestriction(r: string) {
  await apiFetch('/settings/restrictions', {
    method: 'DELETE',
    body: JSON.stringify({ restriction: r }),
  })
  await loadRestrictions()
}

onMounted(loadRestrictions)
</script>
