<template>
  <form @submit.prevent="handleSubmit" class="space-y-6">
    <!-- Title + emoji -->
    <div :class="isGuessed('title') && 'ring-2 ring-amber-400 rounded-lg'">
      <label class="block text-sm font-medium text-stone-600 mb-1">Název</label>
      <div class="flex gap-2">
        <div class="relative">
          <button type="button" @click="showEmojiPicker = !showEmojiPicker"
            class="w-12 h-12 border border-stone-300 rounded-lg text-2xl flex items-center justify-center hover:bg-stone-50"
            :title="form.emoji ? 'Změnit emoji' : 'Přidat emoji'">
            {{ form.emoji || '😀' }}
          </button>
          <div v-if="showEmojiPicker"
            class="absolute z-20 top-14 left-0 bg-white border border-stone-200 rounded-lg shadow-lg p-2 w-72">
            <input v-model="emojiSearch" ref="emojiSearchRef" placeholder="Hledat emoji..."
              class="w-full px-3 py-1.5 border border-stone-200 rounded-lg text-sm mb-2" />
            <div class="grid grid-cols-7 gap-1 max-h-48 overflow-y-auto">
              <button v-for="e in filteredEmojis" :key="e.emoji" type="button"
                @click="form.emoji = e.emoji; showEmojiPicker = false; emojiSearch = ''"
                class="w-8 h-8 text-xl flex items-center justify-center rounded hover:bg-orange-50"
                :title="e.name">
                {{ e.emoji }}
              </button>
            </div>
            <p v-if="filteredEmojis.length === 0" class="text-sm text-stone-400 text-center py-2">Nic nenalezeno</p>
          </div>
        </div>
        <input v-model="form.title" required class="flex-1 px-4 py-3 border border-stone-300 rounded-lg text-lg" />
      </div>
    </div>

    <!-- Description -->
    <div :class="isGuessed('description') && 'ring-2 ring-amber-400 rounded-lg'">
      <label class="block text-sm font-medium text-stone-600 mb-1">Popis
        <span v-if="isGuessed('description')" class="ml-1 text-amber-600 text-xs font-normal">odhadnuto</span>
      </label>
      <textarea v-model="form.description" rows="2" class="w-full px-4 py-3 border border-stone-300 rounded-lg" />
    </div>

    <!-- Servings -->
    <div :class="isGuessed('servings') && 'ring-2 ring-amber-400 rounded-lg'">
      <label class="block text-sm font-medium text-stone-600 mb-1">Porcí
        <span v-if="isGuessed('servings')" class="ml-1 text-amber-600 text-xs font-normal">odhadnuto</span>
      </label>
      <input v-model.number="form.servings" type="number" class="w-32 px-3 py-2 border border-stone-300 rounded-lg" />
    </div>

    <!-- Tags -->
    <div>
      <label class="block text-sm font-medium text-stone-600 mb-1">Tagy</label>
      <input v-model="tagsInput" placeholder="rychlý, vegetariánský, česká kuchyně"
        class="w-full px-4 py-2 border border-stone-300 rounded-lg" />
    </div>

    <!-- ─── SINGLE-SECTION MODE ─── -->
    <div v-if="!form.multiSection">
      <!-- Times in single-section mode -->
      <div class="grid grid-cols-2 gap-4 mb-4">
        <div :class="isGuessed('prep_time_min') && 'ring-2 ring-amber-400 rounded-lg'">
          <label class="block text-sm font-medium text-stone-600 mb-1">Příprava (min)
            <span v-if="isGuessed('prep_time_min')" class="ml-1 text-amber-600 text-xs font-normal">odhadnuto</span>
          </label>
          <input v-model.number="form.sections[0].prep_time_min" type="number"
            class="w-full px-3 py-2 border border-stone-300 rounded-lg" />
        </div>
        <div :class="isGuessed('cook_time_min') && 'ring-2 ring-amber-400 rounded-lg'">
          <label class="block text-sm font-medium text-stone-600 mb-1">Vaření (min)
            <span v-if="isGuessed('cook_time_min')" class="ml-1 text-amber-600 text-xs font-normal">odhadnuto</span>
          </label>
          <input v-model.number="form.sections[0].cook_time_min" type="number"
            class="w-full px-3 py-2 border border-stone-300 rounded-lg" />
        </div>
      </div>

      <!-- Ingredients in single-section mode -->
      <div :class="['mb-4', isGuessed('ingredients') && 'ring-2 ring-amber-400 rounded-lg p-3']">
        <div class="flex items-center gap-2 mb-2">
          <h3 class="text-sm font-medium text-stone-600">Ingredience</h3>
          <span v-if="isGuessed('ingredients')" class="text-amber-600 text-xs bg-amber-50 px-2 py-0.5 rounded-full">odhadnuto — zkontrolujte</span>
        </div>
        <draggable
          v-model="form.sections[0].ingredients"
          group="ingredients-0"
          item-key="_key"
          handle=".drag-handle"
          class="space-y-2"
        >
          <template #item="{ element: ing, index: i }">
            <div class="flex gap-2 items-center">
              <span class="drag-handle text-stone-300 cursor-grab hover:text-stone-500 select-none">⋮⋮</span>
              <input v-model="ing.name" placeholder="Název" class="flex-1 px-3 py-2 border border-stone-300 rounded-lg" />
              <input v-model.number="ing.amount" type="number" step="any" placeholder="Množství" class="w-24 px-3 py-2 border border-stone-300 rounded-lg" />
              <input v-model="ing.unit" placeholder="Jednotka" class="w-20 px-3 py-2 border border-stone-300 rounded-lg" />
              <button type="button" @click="form.sections[0].ingredients.splice(i, 1)" class="text-red-400 hover:text-red-600 px-2">✕</button>
            </div>
          </template>
        </draggable>
        <button type="button"
          @click="form.sections[0].ingredients.push({ _key: newKey(), name: '', amount: null, unit: '', note: '' })"
          class="text-orange-600 text-sm hover:underline mt-2">+ Přidat ingredienci</button>
      </div>

      <!-- Steps in single-section mode -->
      <div :class="['mb-4', isGuessed('steps') && 'ring-2 ring-amber-400 rounded-lg p-3']">
        <div class="flex items-center gap-2 mb-2">
          <h3 class="text-sm font-medium text-stone-600">Postup</h3>
          <span v-if="isGuessed('steps')" class="text-amber-600 text-xs bg-amber-50 px-2 py-0.5 rounded-full">odhadnuto — zkontrolujte</span>
        </div>
        <draggable
          v-model="form.sections[0].steps"
          group="steps-0"
          item-key="_key"
          handle=".drag-handle"
          class="space-y-2"
        >
          <template #item="{ element: step, index: i }">
            <div class="flex gap-2 items-start">
              <span class="drag-handle text-stone-300 cursor-grab hover:text-stone-500 select-none mt-2.5">⋮⋮</span>
              <span class="flex-shrink-0 w-8 h-8 rounded-full flex items-center justify-center text-sm mt-1"
                :class="isGuessed('steps') ? 'bg-amber-100 text-amber-700' : 'bg-stone-200'">{{ i + 1 }}</span>
              <textarea v-model="step.instruction" rows="2" class="flex-1 px-3 py-2 border rounded-lg"
                :class="isGuessed('steps') ? 'border-amber-300 bg-amber-50/50' : 'border-stone-300'" />
              <button type="button" @click="form.sections[0].steps.splice(i, 1)" class="text-red-400 hover:text-red-600 px-2 mt-1">✕</button>
            </div>
          </template>
        </draggable>
        <button type="button"
          @click="form.sections[0].steps.push({ _key: newKey(), step_order: form.sections[0].steps.length + 1, instruction: '' })"
          class="text-orange-600 text-sm hover:underline mt-2">+ Přidat krok</button>
      </div>

      <!-- Multi-section toggle (single → multi) -->
      <label class="flex items-center gap-2 py-3 border-t border-stone-200 cursor-pointer">
        <input type="checkbox" v-model="multiSectionModel" class="w-4 h-4 accent-orange-600" />
        <span class="text-sm text-stone-600">Recept má více částí</span>
      </label>
    </div>

    <!-- ─── MULTI-SECTION MODE ─── -->
    <div v-else class="space-y-4">
      <draggable
        v-model="form.sections"
        group="sections"
        item-key="_key"
        handle=".section-drag-handle"
        class="space-y-4"
      >
        <template #item="{ element: section, index: sIdx }">
          <div class="border border-stone-200 rounded-lg p-4 bg-stone-50">
            <!-- Section header row -->
            <div class="flex items-center gap-2 mb-3">
              <span class="section-drag-handle text-stone-400 cursor-grab hover:text-stone-600 select-none text-lg">⋮⋮</span>
              <input
                v-model="section.label"
                placeholder="Název části (např. Těsto)"
                class="flex-1 font-semibold border-b border-stone-300 bg-transparent focus:outline-none focus:border-orange-500 px-1 py-0.5"
              />
              <button type="button" @click="confirmDeleteSection(sIdx)"
                class="text-red-500 hover:text-red-700 text-sm px-2 py-1 rounded hover:bg-red-50">
                Smazat skupinu
              </button>
            </div>

            <!-- Section description -->
            <textarea
              v-model="section.description"
              placeholder="Volitelný popis části"
              rows="2"
              class="w-full text-sm text-stone-600 border border-stone-200 rounded p-2 mb-3"
            />

            <!-- Section times -->
            <div class="grid grid-cols-3 gap-3 mb-3">
              <label class="block">
                <span class="text-xs text-stone-500">Příprava (min)</span>
                <input v-model.number="section.prep_time_min" type="number"
                  class="w-full border border-stone-300 rounded p-1 mt-0.5" />
              </label>
              <label class="block">
                <span class="text-xs text-stone-500">Metoda</span>
                <select v-model="section.cook_method"
                  class="w-full border border-stone-300 rounded p-1 mt-0.5 text-sm">
                  <option :value="null">—</option>
                  <option value="cooking">Vaření</option>
                  <option value="baking">Pečení</option>
                  <option value="frying">Smažení</option>
                  <option value="steaming">Dušení</option>
                  <option value="other">Jiné</option>
                </select>
              </label>
              <label class="block">
                <span class="text-xs text-stone-500">Čas tepelné úpravy (min)</span>
                <input v-model.number="section.cook_time_min" type="number"
                  class="w-full border border-stone-300 rounded p-1 mt-0.5" />
              </label>
            </div>

            <!-- Section ingredients -->
            <div class="mb-3">
              <h4 class="font-semibold text-sm mb-2 text-stone-700">Ingredience</h4>
              <draggable
                v-model="section.ingredients"
                group="ingredients"
                item-key="_key"
                handle=".drag-handle"
                class="space-y-2"
              >
                <template #item="{ element: ing, index: i }">
                  <div class="flex gap-2 items-center">
                    <span class="drag-handle text-stone-300 cursor-grab hover:text-stone-500 select-none">⋮⋮</span>
                    <input v-model="ing.name" placeholder="Název" class="flex-1 px-2 py-1 border border-stone-300 rounded text-sm" />
                    <input v-model.number="ing.amount" type="number" step="any" placeholder="Množ." class="w-20 px-2 py-1 border border-stone-300 rounded text-sm" />
                    <input v-model="ing.unit" placeholder="Jed." class="w-16 px-2 py-1 border border-stone-300 rounded text-sm" />
                    <input v-model="ing.note" placeholder="Pozn." class="w-24 px-2 py-1 border border-stone-300 rounded text-sm" />
                    <button type="button" @click="section.ingredients.splice(i, 1)"
                      class="text-red-400 hover:text-red-600 px-1">✕</button>
                  </div>
                </template>
              </draggable>
              <button type="button"
                @click="section.ingredients.push({ _key: newKey(), name: '', amount: null, unit: '', note: '' })"
                class="text-sm text-orange-600 hover:text-orange-700 mt-2">+ ingredience</button>
            </div>

            <!-- Section steps -->
            <div>
              <h4 class="font-semibold text-sm mb-2 text-stone-700">Postup</h4>
              <draggable
                v-model="section.steps"
                group="steps"
                item-key="_key"
                handle=".drag-handle"
                class="space-y-2"
              >
                <template #item="{ element: step, index: i }">
                  <div class="flex gap-2 items-start">
                    <span class="drag-handle text-stone-300 cursor-grab hover:text-stone-500 select-none mt-2">⋮⋮</span>
                    <span class="flex-shrink-0 w-6 h-6 rounded-full bg-stone-200 flex items-center justify-center text-xs mt-1.5">{{ i + 1 }}</span>
                    <textarea v-model="step.instruction" rows="2"
                      class="flex-1 px-2 py-1 border border-stone-300 rounded text-sm" />
                    <button type="button" @click="section.steps.splice(i, 1)"
                      class="text-red-400 hover:text-red-600 px-1 mt-1">✕</button>
                  </div>
                </template>
              </draggable>
              <button type="button"
                @click="section.steps.push({ _key: newKey(), step_order: section.steps.length + 1, instruction: '' })"
                class="text-sm text-orange-600 hover:text-orange-700 mt-2">+ krok</button>
            </div>
          </div>
        </template>
      </draggable>

      <button type="button" @click="addSection"
        class="px-4 py-2 bg-orange-100 hover:bg-orange-200 text-orange-700 rounded font-medium">
        + Přidat skupinu
      </button>

      <!-- Multi-section toggle (multi → single, destructive) -->
      <label class="flex items-center gap-2 py-3 border-t border-stone-200 cursor-pointer">
        <input type="checkbox" v-model="multiSectionModel" class="w-4 h-4 accent-orange-600" />
        <span class="text-sm text-stone-600">Recept má více částí</span>
      </label>
    </div>

    <button type="submit" class="w-full py-3 bg-orange-600 text-white rounded-lg hover:bg-orange-700 font-medium text-lg">
      Uložit recept
    </button>
  </form>
</template>

<script setup lang="ts">
import { reactive, ref, computed, watch } from 'vue'
import draggable from 'vuedraggable'
import type { CookMethod, Section } from '../api/recipes'

const props = defineProps<{ initial?: any }>()
const emit = defineEmits<{ save: [data: any] }>()

// ─── Guessed-fields highlight ───────────────────────────────────────────────

const guessedFields = computed(() => new Set(props.initial?.guessed_fields || []))
function isGuessed(field: string): boolean {
  return guessedFields.value.has(field)
}

// ─── Emoji picker ───────────────────────────────────────────────────────────

const showEmojiPicker = ref(false)
const emojiSearch = ref('')
const emojiSearchRef = ref<HTMLInputElement | null>(null)

const emojiList = [
  { emoji: '🍝', name: 'pasta spaghetti těstoviny' },
  { emoji: '🍕', name: 'pizza' },
  { emoji: '🥗', name: 'salát salad green' },
  { emoji: '🍲', name: 'polévka soup hrnec pot' },
  { emoji: '🥘', name: 'pánev pan paella' },
  { emoji: '🍜', name: 'nudle noodles ramen' },
  { emoji: '🍛', name: 'kari curry' },
  { emoji: '🥟', name: 'knedlík dumpling pierogi' },
  { emoji: '🫕', name: 'fondue sýr cheese zapečený' },
  { emoji: '🥦', name: 'brokolice broccoli zelenina' },
  { emoji: '🍶', name: 'omáčka sauce' },
  { emoji: '🐽', name: 'vepřové pork prase pig' },
  { emoji: '🐔', name: 'kuře chicken' },
  { emoji: '🐟', name: 'ryba fish losos salmon' },
  { emoji: '🥩', name: 'maso steak hovězí beef' },
  { emoji: '🍗', name: 'kuřecí stehno drumstick' },
  { emoji: '🥚', name: 'vejce egg' },
  { emoji: '🧀', name: 'sýr cheese' },
  { emoji: '🥕', name: 'mrkev carrot' },
  { emoji: '🍅', name: 'rajče tomato' },
  { emoji: '🌽', name: 'kukuřice corn' },
  { emoji: '🍄', name: 'houby mushroom žampion' },
  { emoji: '🥔', name: 'brambory potato' },
  { emoji: '🍆', name: 'lilek eggplant' },
  { emoji: '🫑', name: 'paprika pepper' },
  { emoji: '🧅', name: 'cibule onion' },
  { emoji: '🍋', name: 'citron lemon' },
  { emoji: '🥒', name: 'okurka cucumber' },
  { emoji: '🦐', name: 'krevety shrimp' },
  { emoji: '🦑', name: 'kalamáry squid' },
  { emoji: '🐙', name: 'chobotnice octopus' },
  { emoji: '🦆', name: 'kachna duck' },
  { emoji: '🪿', name: 'husa goose' },
  { emoji: '🎃', name: 'dýně pumpkin' },
  { emoji: '🥧', name: 'koláč pie dort' },
  { emoji: '🍰', name: 'dort cake zákusek' },
  { emoji: '🥞', name: 'lívanec pancake palačinka' },
  { emoji: '🍞', name: 'chleba bread pečivo' },
  { emoji: '🥖', name: 'bageta baguette' },
  { emoji: '🌮', name: 'taco tortilla mexické' },
  { emoji: '🌯', name: 'burrito wrap' },
  { emoji: '🍔', name: 'burger hamburger' },
  { emoji: '🥙', name: 'kebab falafel pita' },
  { emoji: '🍣', name: 'sushi japonské' },
  { emoji: '🫘', name: 'fazole beans luštěniny' },
  { emoji: '🍚', name: 'rýže rice rizoto' },
]

const filteredEmojis = computed(() => {
  const q = emojiSearch.value.toLowerCase().trim()
  if (!q) return emojiList
  return emojiList.filter(e => e.name.toLowerCase().includes(q) || e.emoji === q)
})

watch(showEmojiPicker, (v) => {
  if (v) setTimeout(() => emojiSearchRef.value?.focus(), 50)
})

// ─── Data model ─────────────────────────────────────────────────────────────

interface FormIngredient {
  _key: string  // client-side stable key for v-for / draggable
  name: string
  amount: number | null
  unit: string
  note: string
}

interface FormStep {
  _key: string  // client-side stable key for v-for / draggable
  step_order: number  // ignored on save; reassigned by index
  instruction: string
}

interface FormSection {
  _key: string        // client-side stable key for v-for / draggable
  id?: string         // server id (undefined for new)
  label: string | null
  description: string | null
  prep_time_min: number | null
  cook_time_min: number | null
  cook_method: CookMethod | null
  sort_order: number
  ingredients: FormIngredient[]
  steps: FormStep[]
}

interface FormState {
  title: string
  description: string
  servings: number | null
  emoji: string
  tags: string[]
  sections: FormSection[]
  multiSection: boolean
}

/** Stable client-side key for draggable rows — never sent to the backend. */
const newKey = (): string => crypto.randomUUID()

function blankSection(sortOrder = 0): FormSection {
  return {
    _key: newKey(),
    label: null,
    description: null,
    prep_time_min: null,
    cook_time_min: null,
    cook_method: null,
    sort_order: sortOrder,
    ingredients: [],
    steps: [],
  }
}

function fromInitial(initial?: any): FormState {
  if (initial?.sections && Array.isArray(initial.sections) && initial.sections.length > 0) {
    const sections: FormSection[] = initial.sections.map((s: Section, i: number) => ({
      _key: newKey(),
      id: s.id,
      label: s.label,
      description: s.description,
      prep_time_min: s.prep_time_min,
      cook_time_min: s.cook_time_min,
      cook_method: s.cook_method ?? null,
      sort_order: s.sort_order ?? i,
      ingredients: (s.ingredients || []).map(ing => ({
        _key: newKey(),
        name: ing.name,
        amount: ing.amount,
        unit: ing.unit ?? '',
        note: ing.note ?? '',
      })),
      steps: (s.steps || []).map(st => ({
        _key: newKey(),
        step_order: st.step_order,
        instruction: st.instruction,
      })),
    }))
    const multiSection = sections.length > 1 || sections.some(s => s.label !== null && s.label !== '')
    return {
      title: initial.title ?? '',
      description: initial.description ?? '',
      servings: initial.servings ?? null,
      emoji: initial.emoji ?? '',
      tags: initial.tags ?? [],
      sections,
      multiSection,
    }
  }

  // No sections in initial (new recipe or blank form)
  const section0: FormSection = blankSection(0)
  return {
    title: initial?.title ?? '',
    description: initial?.description ?? '',
    servings: initial?.servings ?? null,
    emoji: initial?.emoji ?? '',
    tags: initial?.tags ?? [],
    sections: [section0],
    multiSection: false,
  }
}

const form = reactive<FormState>(fromInitial(props.initial))

// ─── Tags computed binding ───────────────────────────────────────────────────

const tagsInput = computed({
  get: () => form.tags.join(', '),
  set: (v: string) => { form.tags = v.split(',').map(t => t.trim()).filter(Boolean) },
})

// ─── Multi-section toggle handlers ──────────────────────────────────────────

function enableMultiSection() {
  form.multiSection = true
  // Give the single anonymous section an empty label so the user can rename it
  if (form.sections[0].label === null) form.sections[0].label = ''
}

const multiSectionModel = computed({
  get: () => form.multiSection,
  set: (v: boolean) => {
    if (v) {
      enableMultiSection()
    } else {
      tryDisableMultiSection()
    }
  },
})

function tryDisableMultiSection() {
  if (form.sections.length === 1) {
    // Already logically single — just hide multi-section UI
    form.multiSection = false
    form.sections[0].label = null
    form.sections[0].description = null
    return
  }

  const totalIng = form.sections.reduce(
    (n, s) => n + s.ingredients.filter(i => i.name.trim() !== '').length,
    0,
  )
  const totalSteps = form.sections.reduce(
    (n, s) => n + s.steps.filter(st => st.instruction.trim() !== '').length,
    0,
  )

  const ok = confirm(
    `⚠️ Sloučit ${form.sections.length} skupin do jedné?\n\n` +
    `Zachová se obsah všech skupin v pořadí (${totalIng} ingrediencí a ${totalSteps} kroků), ` +
    `ALE NÁZVY A POPISY VŠECH SKUPIN BUDOU NENÁVRATNĚ ODSTRANĚNY. ` +
    `Per-section časy se sečtou do jednoho.\n\n` +
    // TODO: spec asked for a red destructive button here, but native confirm() dialogs
    // cannot be styled. Add a proper modal component in a future iteration.
    `Pokračovat?`,
  )

  if (!ok) {
    // User cancelled — do NOT mutate form.multiSection.
    // The computed setter's set was rejected; next render re-syncs checkbox to true.
    return
  }

  // Merge sections in order
  const merged: FormSection = {
    _key: form.sections[0]._key,
    id: form.sections[0].id,
    label: null,
    description: null,
    prep_time_min: form.sections.reduce((sum, s) => sum + (s.prep_time_min ?? 0), 0) || null,
    cook_time_min: form.sections.reduce((sum, s) => sum + (s.cook_time_min ?? 0), 0) || null,
    cook_method: null,
    sort_order: 0,
    ingredients: form.sections.flatMap(s => s.ingredients),
    steps: [],
  }
  let stepCounter = 1
  for (const s of form.sections) {
    for (const st of s.steps) {
      merged.steps.push({ _key: newKey(), step_order: stepCounter++, instruction: st.instruction })
    }
  }
  form.sections = [merged]
  form.multiSection = false
}

// ─── Section management ──────────────────────────────────────────────────────

function addSection() {
  form.sections.push(blankSection(form.sections.length))
}

function confirmDeleteSection(idx: number) {
  const section = form.sections[idx]
  const ingCount = section.ingredients.filter(i => i.name.trim() !== '').length
  const stepCount = section.steps.filter(s => s.instruction.trim() !== '').length

  if (form.sections.length === 1) {
    alert('Recept musí mít alespoň jednu skupinu.')
    return
  }

  // Empty section — silent delete
  if (ingCount === 0 && stepCount === 0) {
    form.sections.splice(idx, 1)
    return
  }

  const others = form.sections.map((s, i) => ({ s, i })).filter(({ i }) => i !== idx)
  const targetIdxStr = prompt(
    `Skupina „${section.label || '(beze jména)'}" obsahuje ${ingCount} ingrediencí a ${stepCount} kroků.\n\n` +
    `Kam přesunout obsah? Zadej číslo skupiny (1–${others.length}), nebo nech prázdné a vše se SMAŽE.\n\n` +
    others.map(({ s }, i) => `${i + 1}. ${s.label || '(beze jména)'}`).join('\n'),
  )

  if (targetIdxStr === null) return // cancelled

  if (targetIdxStr.trim() === '') {
    if (!confirm(`Smazat skupinu „${section.label || '(beze jména)'}" včetně ${ingCount} ingrediencí a ${stepCount} kroků? Akce je nevratná.`)) {
      return
    }
    form.sections.splice(idx, 1)
    return
  }

  const targetN = parseInt(targetIdxStr.trim(), 10)
  if (isNaN(targetN) || targetN < 1 || targetN > others.length) {
    alert('Neplatné číslo skupiny.')
    return
  }

  const target = others[targetN - 1].s
  target.ingredients.push(...section.ingredients.filter(i => i.name.trim() !== ''))
  const baseStepOrder = target.steps.length
  target.steps.push(
    ...section.steps
      .filter(st => st.instruction.trim() !== '')
      .map((st, i) => ({ ...st, _key: newKey(), step_order: baseStepOrder + i + 1 })),
  )
  form.sections.splice(idx, 1)
}

// ─── Submit handler ──────────────────────────────────────────────────────────

function handleSubmit() {
  // Note: source_type and source_url are intentionally omitted from this payload.
  // UpdateRecipeRequest (backend/src/models.rs) does not include those fields,
  // so they cannot be overwritten via PUT — the DB values are always preserved.
  const payload = {
    title: form.title,
    description: form.description || null,
    servings: form.servings,
    emoji: form.emoji || null,
    tags: form.tags,
    sections: form.sections.map((s, idx) => ({
      id: s.id,
      // _key is UI-only — not sent to backend
      label: s.label && s.label.length > 0 ? s.label : null,
      description: s.description || null,
      prep_time_min: s.prep_time_min,
      cook_time_min: s.cook_time_min,
      cook_method: s.cook_method,
      sort_order: idx,
      ingredients: s.ingredients
        .filter(i => i.name.trim() !== '')
        .map(i => ({
          // _key is UI-only — not sent to backend
          name: i.name,
          amount: i.amount,
          unit: i.unit || null,
          note: i.note || null,
        })),
      steps: s.steps
        .filter(st => st.instruction.trim() !== '')
        .map((st, i) => ({
          // _key is UI-only — not sent to backend
          step_order: i + 1,
          instruction: st.instruction,
        })),
    })),
  }
  emit('save', payload)
}

// ─── Watch for external initial prop changes ─────────────────────────────────
// Gate on id change only — a deep watch would clobber in-progress edits on every
// parent re-render. For the new-recipe path, props.initial is set once and the
// watcher never fires again.

watch(
  () => props.initial?.id,
  (newId, oldId) => {
    if (newId !== oldId) {
      Object.assign(form, fromInitial(props.initial))
    }
  },
)
</script>
