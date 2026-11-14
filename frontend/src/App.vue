<template>
  <div class="min-h-screen bg-stone-50">
    <nav v-if="isAuthenticated" class="bg-white border-b border-stone-200 px-4 py-3">
      <div class="max-w-3xl mx-auto flex items-center justify-between">
        <div class="flex gap-6">
          <router-link to="/recipes" class="text-stone-700 font-medium hover:text-orange-600">Recepty</router-link>
          <router-link to="/plan" class="text-stone-700 font-medium hover:text-orange-600">Plan</router-link>
          <router-link to="/log" class="text-stone-700 font-medium hover:text-orange-600">Log</router-link>
        </div>
        <div ref="menuRoot" class="relative" @keydown.esc="menuOpen = false">
          <button type="button" class="flex items-center gap-1 text-stone-500 text-sm hover:text-orange-600"
            aria-haspopup="menu" :aria-expanded="menuOpen" @click="menuOpen = !menuOpen">
            <svg class="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
            </svg>
            <span class="truncate max-w-[6rem] sm:max-w-[11rem]">{{ authStore.user?.name }}</span>
          </button>
          <div v-if="menuOpen" role="menu"
            class="absolute right-0 mt-2 w-40 bg-white border border-stone-200 rounded-lg shadow-lg py-1 z-20">
            <router-link to="/settings" role="menuitem"
              class="block px-4 py-2 text-sm text-stone-700 hover:bg-stone-50 hover:text-orange-600">Nastavení</router-link>
            <form method="post" action="/api/auth/logout">
              <input type="hidden" name="csrf_token" :value="authStore.csrf" />
              <button type="submit" role="menuitem"
                class="block w-full text-left px-4 py-2 text-sm text-stone-700 hover:bg-stone-50 hover:text-red-600">Odhlásit</button>
            </form>
          </div>
        </div>
      </div>
    </nav>
    <main class="max-w-3xl mx-auto px-4 py-6">
      <router-view v-slot="{ Component }">
        <KeepAlive :include="['RecipeListPage']">
          <component :is="Component" />
        </KeepAlive>
      </router-view>
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useAuthStore } from './stores/auth'

const authStore = useAuthStore()
const isAuthenticated = computed(() => !!authStore.user)

const menuOpen = ref(false)
const menuRoot = ref<HTMLElement | null>(null)

function closeOnOutsideClick(e: MouseEvent) {
  if (menuRoot.value && !menuRoot.value.contains(e.target as Node)) menuOpen.value = false
}

onMounted(() => document.addEventListener('click', closeOnOutsideClick))
onBeforeUnmount(() => document.removeEventListener('click', closeOnOutsideClick))
watch(() => useRoute().fullPath, () => { menuOpen.value = false })
</script>
