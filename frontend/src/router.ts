import { createRouter, createWebHistory } from 'vue-router'
import { useAuthStore } from './stores/auth'
import { ApiError } from './api/client'

const routes = [
  { path: '/', redirect: '/recipes' },
  { path: '/login', component: () => import('./pages/LoginPage.vue') },
  { path: '/recipes', component: () => import('./pages/RecipeListPage.vue') },
  { path: '/recipes/new', component: () => import('./pages/RecipeNewPage.vue') },
  { path: '/recipes/discover', component: () => import('./pages/DiscoverPage.vue') },
  { path: '/recipes/:id', component: () => import('./pages/RecipeDetailPage.vue') },
  { path: '/plan', component: () => import('./pages/PlanPage.vue') },
  { path: '/log', component: () => import('./pages/LogPage.vue') },
  { path: '/settings', component: () => import('./pages/SettingsPage.vue') },
  { path: '/r/:slug', component: () => import('./pages/PublicRecipePage.vue') },
]

export const router = createRouter({
  history: createWebHistory(),
  routes,
  // The layout scrolls the window (App.vue has no inner overflow container), so
  // the browser's saved offset is meaningful — provided KeepAlive has kept every
  // loaded batch in the DOM.
  scrollBehavior(_to, _from, savedPosition) {
    return savedPosition ?? { top: 0 }
  },
})

// Navigation guard: require a valid session for non-public pages
router.beforeEach(async (to) => {
  if (to.path === '/login' || to.path.startsWith('/r/')) return

  const auth = useAuthStore()
  if (auth.user) return

  try {
    await auth.fetchMe()
  } catch (e) {
    // Only a real 401 means logged out; outages must not look like logout
    const error = e instanceof ApiError && e.status === 401 ? undefined : 'server_unavailable'
    return { path: '/login', query: { ...(error && { error }), redirect: to.fullPath } }
  }
})
