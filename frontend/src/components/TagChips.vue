<template>
  <div class="flex flex-wrap gap-2">
    <span v-for="tag in tags" :key="tag"
      @click="$emit('select', tag)"
      class="px-3 py-1 rounded-full text-sm cursor-pointer"
      :class="selected === tag
        ? 'bg-orange-600 text-white'
        : 'bg-stone-200 text-stone-700 hover:bg-stone-300'">
      {{ tag }}
    </span>
  </div>
</template>

<script setup lang="ts">
defineProps<{ tags: string[]; selected?: string }>()
defineEmits<{ select: [tag: string] }>()
</script>
