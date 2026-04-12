<template>
  <form @submit.prevent="$emit('save', form)" class="space-y-6">
    <div :class="isGuessed('title') && 'ring-2 ring-amber-400 rounded-lg'">
      <label class="block text-sm font-medium text-stone-600 mb-1">Název</label>
      <input v-model="form.title" required class="w-full px-4 py-3 border border-stone-300 rounded-lg text-lg" />
    </div>
    <div :class="isGuessed('description') && 'ring-2 ring-amber-400 rounded-lg'">
      <label class="block text-sm font-medium text-stone-600 mb-1">Popis
        <span v-if="isGuessed('description')" class="ml-1 text-amber-600 text-xs font-normal">odhadnuto</span>
      </label>
      <textarea v-model="form.description" rows="2" class="w-full px-4 py-3 border border-stone-300 rounded-lg" />
    </div>
    <div class="grid grid-cols-3 gap-4">
      <div :class="isGuessed('servings') && 'ring-2 ring-amber-400 rounded-lg'">
        <label class="block text-sm font-medium text-stone-600 mb-1">Porcí
          <span v-if="isGuessed('servings')" class="ml-1 text-amber-600 text-xs font-normal">odhadnuto</span>
        </label>
        <input v-model.number="form.servings" type="number" class="w-full px-3 py-2 border border-stone-300 rounded-lg" />
      </div>
      <div :class="isGuessed('prep_time_min') && 'ring-2 ring-amber-400 rounded-lg'">
        <label class="block text-sm font-medium text-stone-600 mb-1">Příprava (min)
          <span v-if="isGuessed('prep_time_min')" class="ml-1 text-amber-600 text-xs font-normal">odhadnuto</span>
        </label>
        <input v-model.number="form.prep_time_min" type="number" class="w-full px-3 py-2 border border-stone-300 rounded-lg" />
      </div>
      <div :class="isGuessed('cook_time_min') && 'ring-2 ring-amber-400 rounded-lg'">
        <label class="block text-sm font-medium text-stone-600 mb-1">Vaření (min)
          <span v-if="isGuessed('cook_time_min')" class="ml-1 text-amber-600 text-xs font-normal">odhadnuto</span>
        </label>
        <input v-model.number="form.cook_time_min" type="number" class="w-full px-3 py-2 border border-stone-300 rounded-lg" />
      </div>
    </div>

    <div>
      <label class="block text-sm font-medium text-stone-600 mb-1">Tagy</label>
      <input v-model="tagsInput" placeholder="rychlý, vegetariánský, česká kuchyně"
        class="w-full px-4 py-2 border border-stone-300 rounded-lg" />
    </div>

    <div :class="isGuessed('ingredients') && 'ring-2 ring-amber-400 rounded-lg p-3'">
      <div class="flex items-center gap-2 mb-2">
        <h3 class="text-sm font-medium text-stone-600">Ingredience</h3>
        <span v-if="isGuessed('ingredients')" class="text-amber-600 text-xs bg-amber-50 px-2 py-0.5 rounded-full">odhadnuto — zkontrolujte</span>
      </div>
      <div v-for="(ing, i) in form.ingredients" :key="i" class="flex gap-2 mb-2">
        <input v-model="ing.name" placeholder="Název" class="flex-1 px-3 py-2 border border-stone-300 rounded-lg" />
        <input v-model.number="ing.amount" type="number" step="any" placeholder="Množství" class="w-24 px-3 py-2 border border-stone-300 rounded-lg" />
        <input v-model="ing.unit" placeholder="Jednotka" class="w-20 px-3 py-2 border border-stone-300 rounded-lg" />
        <button type="button" @click="form.ingredients.splice(i, 1)" class="text-red-400 hover:text-red-600 px-2">✕</button>
      </div>
      <button type="button" @click="addIngredient" class="text-orange-600 text-sm hover:underline">+ Přidat ingredienci</button>
    </div>

    <div :class="isGuessed('steps') && 'ring-2 ring-amber-400 rounded-lg p-3'">
      <div class="flex items-center gap-2 mb-2">
        <h3 class="text-sm font-medium text-stone-600">Postup</h3>
        <span v-if="isGuessed('steps')" class="text-amber-600 text-xs bg-amber-50 px-2 py-0.5 rounded-full">odhadnuto — zkontrolujte</span>
      </div>
      <div v-for="(step, i) in form.steps" :key="i" class="flex gap-2 mb-2">
        <span class="flex-shrink-0 w-8 h-8 rounded-full flex items-center justify-center text-sm"
          :class="isGuessed('steps') ? 'bg-amber-100 text-amber-700' : 'bg-stone-200'">{{ i + 1 }}</span>
        <textarea v-model="step.instruction" rows="2" class="flex-1 px-3 py-2 border rounded-lg"
          :class="isGuessed('steps') ? 'border-amber-300 bg-amber-50/50' : 'border-stone-300'" />
        <button type="button" @click="form.steps.splice(i, 1)" class="text-red-400 hover:text-red-600 px-2">✕</button>
      </div>
      <button type="button" @click="addStep" class="text-orange-600 text-sm hover:underline">+ Přidat krok</button>
    </div>

    <button type="submit" class="w-full py-3 bg-orange-600 text-white rounded-lg hover:bg-orange-700 font-medium text-lg">
      Uložit recept
    </button>
  </form>
</template>

<script setup lang="ts">
import { reactive, computed, watch } from 'vue'

const props = defineProps<{ initial?: any }>()
defineEmits<{ save: [data: any] }>()

const guessedFields = computed(() => new Set(props.initial?.guessed_fields || []))

function isGuessed(field: string): boolean {
  return guessedFields.value.has(field)
}

const form = reactive({
  title: props.initial?.title || '',
  description: props.initial?.description || '',
  servings: props.initial?.servings || null,
  prep_time_min: props.initial?.prep_time_min || null,
  cook_time_min: props.initial?.cook_time_min || null,
  ingredients: props.initial?.ingredients || [],
  steps: props.initial?.steps || [],
  tags: props.initial?.tags || [],
  source_type: props.initial?.source_type || 'manual',
})

const tagsInput = computed({
  get: () => form.tags.join(', '),
  set: (v: string) => { form.tags = v.split(',').map(t => t.trim()).filter(Boolean) },
})

function addIngredient() {
  form.ingredients.push({ name: '', amount: null, unit: '', note: '' })
}
function addStep() {
  form.steps.push({ step_order: form.steps.length + 1, instruction: '' })
}

watch(() => props.initial, (v) => {
  if (v) Object.assign(form, v)
}, { deep: true })
</script>
