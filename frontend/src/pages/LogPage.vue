<template>
  <div>
    <h1 class="text-2xl font-bold text-stone-800 mb-6">Co jsme dnes jedli?</h1>
    <div class="space-y-6">
      <div v-for="meal in ['lunch', 'dinner']" :key="meal" class="bg-white rounded-xl border border-stone-200 p-4">
        <h3 class="font-medium text-stone-700 mb-3">{{ meal === 'lunch' ? 'Oběd' : 'Večeře' }}</h3>
        <input v-model="logs[meal]" :placeholder="`Co jste měli k ${meal === 'lunch' ? 'obědu' : 'večeři'}?`"
          class="w-full px-4 py-3 border border-stone-300 rounded-lg text-lg" />
      </div>
      <button @click="saveLog" :disabled="saving"
        class="w-full py-3 bg-orange-600 text-white rounded-lg hover:bg-orange-700 font-medium text-lg disabled:opacity-50">
        {{ saving ? 'Ukládám...' : 'Uložit' }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { reactive, ref } from 'vue'
import { createPlanEntry } from '../api/plan'

const logs = reactive<Record<string, string>>({ lunch: '', dinner: '' })
const saving = ref(false)

async function saveLog() {
  saving.value = true
  const today = new Date().toISOString().slice(0, 10)
  try {
    for (const [meal, text] of Object.entries(logs)) {
      if (text.trim()) {
        await createPlanEntry({
          date: today,
          meal_type: meal,
          free_text: text,
          entry_type: 'logged',
        })
      }
    }
    logs.lunch = ''
    logs.dinner = ''
    alert('Uloženo!')
  } finally {
    saving.value = false
  }
}
</script>
