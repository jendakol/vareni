<template>
  <div class="fixed inset-0 z-50 flex justify-end sm:bg-black/50">
    <div class="w-full sm:w-96 bg-white h-full flex flex-col shadow-xl">
      <div class="flex items-center justify-between p-4 border-b border-stone-200">
        <h3 class="font-semibold text-stone-800">Upravit recept</h3>
        <button @click="$emit('close')" class="p-2 text-stone-400 hover:text-stone-700">✕</button>
      </div>

      <div ref="messagesEl" class="flex-1 overflow-y-auto p-4 space-y-4">
        <div v-for="(msg, i) in messages" :key="i"
          :class="msg.role === 'user' ? 'ml-8' : 'mr-8'">
          <div :class="msg.role === 'user'
            ? 'bg-orange-50 text-stone-800 rounded-2xl rounded-tr-sm px-4 py-3'
            : 'bg-stone-100 text-stone-800 rounded-2xl rounded-tl-sm px-4 py-3'">
            {{ msg.text }}
          </div>
        </div>
        <div v-if="streaming" class="mr-8">
          <div class="bg-stone-100 text-stone-800 rounded-2xl rounded-tl-sm px-4 py-3">
            {{ streamText }}<span class="animate-pulse">|</span>
          </div>
        </div>
      </div>

      <div v-if="hasUpdates" class="px-4 py-2 border-t bg-orange-50">
        <button @click="saveChanges"
          class="w-full py-2 bg-orange-600 text-white rounded-lg hover:bg-orange-700 font-medium">
          Uložit změny
        </button>
      </div>

      <form @submit.prevent="send" class="p-4 border-t border-stone-200">
        <div class="flex gap-2">
          <input v-model="input" placeholder="Např. přidej víc česneku..."
            class="flex-1 px-4 py-3 border border-stone-300 rounded-lg text-lg"
            :disabled="streaming" />
          <button type="submit" :disabled="!input.trim() || streaming"
            class="px-4 py-3 bg-orange-600 text-white rounded-lg hover:bg-orange-700 disabled:opacity-50">
            →
          </button>
        </div>
      </form>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, nextTick } from 'vue'
import { updateRecipe } from '../api/recipes'

const props = defineProps<{ recipeId: string }>()
const emit = defineEmits<{ close: []; update: [] }>()

const input = ref('')
const messages = ref<{ role: string; text: string }[]>([])
const streaming = ref(false)
const streamText = ref('')
const hasUpdates = ref(false)
const pendingUpdate = ref<any>(null)
const messagesEl = ref<HTMLElement>()

async function send() {
  const text = input.value.trim()
  if (!text) return
  input.value = ''
  messages.value.push({ role: 'user', text })
  streaming.value = true
  streamText.value = ''

  try {
    const token = localStorage.getItem('token')
    const resp = await fetch(`/api/chat/${props.recipeId}`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${token}`,
      },
      body: JSON.stringify({ message: text }),
    })

    const reader = resp.body?.getReader()
    const decoder = new TextDecoder()
    let toolJson = ''
    let inToolUse = false

    while (reader) {
      const { done, value } = await reader.read()
      if (done) break

      const chunk = decoder.decode(value, { stream: true })
      const lines = chunk.split('\n')

      for (const line of lines) {
        if (!line.startsWith('data: ')) continue
        const data = line.slice(6)
        if (data === '[DONE]') continue

        try {
          const event = JSON.parse(data)

          if (event.type === 'content_block_start' && event.content_block?.type === 'tool_use') {
            inToolUse = true
            toolJson = ''
          } else if (event.type === 'content_block_delta') {
            if (inToolUse && event.delta?.partial_json) {
              toolJson += event.delta.partial_json
            } else if (event.delta?.text) {
              streamText.value += event.delta.text
            }
          } else if (event.type === 'content_block_stop' && inToolUse) {
            inToolUse = false
            try {
              pendingUpdate.value = JSON.parse(toolJson)
              hasUpdates.value = true
            } catch { /* ignore parse errors */ }
          }
        } catch { /* ignore non-JSON lines */ }
      }
    }

    if (streamText.value) {
      messages.value.push({ role: 'assistant', text: streamText.value })
    }
  } catch (e: any) {
    messages.value.push({ role: 'assistant', text: `Chyba: ${e.message}` })
  } finally {
    streaming.value = false
    streamText.value = ''
    await nextTick()
    messagesEl.value?.scrollTo({ top: messagesEl.value.scrollHeight })
  }
}

async function saveChanges() {
  if (!pendingUpdate.value) return
  try {
    await updateRecipe(props.recipeId, pendingUpdate.value)
    pendingUpdate.value = null
    hasUpdates.value = false
    emit('update')
  } catch (e: any) {
    messages.value.push({ role: 'assistant', text: `Chyba při ukládání: ${e.message}` })
  }
}
</script>
