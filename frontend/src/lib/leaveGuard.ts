import { watch, onBeforeUnmount, type Ref } from 'vue'

// Warns before closing the tab while `active` is true (unsent photos).
export function useLeaveGuard(active: Ref<boolean>) {
  const handler = (e: BeforeUnloadEvent) => e.preventDefault()
  const stop = watch(
    active,
    (on) => {
      if (on) window.addEventListener('beforeunload', handler)
      else window.removeEventListener('beforeunload', handler)
    },
    { immediate: true },
  )
  onBeforeUnmount(() => {
    stop()
    window.removeEventListener('beforeunload', handler)
  })
}
