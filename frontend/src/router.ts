import { createRouter, createWebHistory } from 'vue-router'

const routes = [
  { path: '/', redirect: '/recipes' },
  { path: '/login', component: () => import('./pages/LoginPage.vue') },
  { path: '/recipes', component: () => import('./pages/RecipeListPage.vue') },
  { path: '/recipes/new', component: () => import('./pages/RecipeNewPage.vue') },
  { path: '/recipes/:id', component: () => import('./pages/RecipeDetailPage.vue') },
  { path: '/plan', component: () => import('./pages/PlanPage.vue') },
  { path: '/log', component: () => import('./pages/LogPage.vue') },
  { path: '/settings', component: () => import('./pages/SettingsPage.vue') },
  { path: '/r/:slug', component: () => import('./pages/PublicRecipePage.vue') },
]

export const router = createRouter({
  history: createWebHistory(),
  routes,
})

// Navigation guard: redirect to login if not authenticated
router.beforeEach((to) => {
  const publicPaths = ['/login', '/r/']
  const isPublic = publicPaths.some(p => to.path.startsWith(p))
  const token = localStorage.getItem('token')

  if (!isPublic && !token) {
    return '/login'
  }
})
