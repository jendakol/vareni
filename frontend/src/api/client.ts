import { ref } from 'vue'

const BASE = '/api'

// CSRF token from /auth/me, sent on every state-changing request
export const csrfToken = ref('')

export class ApiError extends Error {
  constructor(message: string, public status: number) {
    super(message)
  }
}

export const SERVER_DOWN = 'Server nedostupný'

// Public pages must never bounce to login
function isPublicPath(path: string) {
  return path === '/login' || path.startsWith('/r/')
}

function redirectToLogin() {
  if (isPublicPath(window.location.pathname)) return
  const back = encodeURIComponent(window.location.pathname + window.location.search)
  window.location.href = `/login?error=session_expired&redirect=${back}`
}

async function rawFetch(path: string, options: RequestInit): Promise<Response> {
  const headers: Record<string, string> = {
    ...(options.headers as Record<string, string> || {}),
  }
  const method = (options.method || 'GET').toUpperCase()

  if (method !== 'GET' && method !== 'HEAD' && csrfToken.value) {
    headers['X-CSRF-Token'] = csrfToken.value
  }
  if (options.body && typeof options.body === 'string') {
    headers['Content-Type'] = 'application/json'
  }

  try {
    return await fetch(`${BASE}${path}`, { ...options, headers, credentials: 'same-origin' })
  } catch {
    // Network failure is not a logout
    throw new ApiError(SERVER_DOWN, 0)
  }
}

// Refresh the CSRF token; false if the session is gone
async function refreshCsrf(): Promise<boolean> {
  try {
    const resp = await rawFetch('/auth/me', {})
    if (!resp.ok) return false
    csrfToken.value = (await resp.json()).csrf_token || ''
    return !!csrfToken.value
  } catch {
    return false
  }
}

export async function apiFetch<T>(
  path: string,
  options: RequestInit = {},
  redirectOn401 = true,
): Promise<T> {
  let resp = await rawFetch(path, options)

  // CSRF failure: refetch the token once and retry
  if (resp.status === 403 && (await refreshCsrf())) {
    resp = await rawFetch(path, options)
  }

  if (resp.status === 401) {
    if (redirectOn401) redirectToLogin()
    throw new ApiError('Přihlášení vypršelo', 401)
  }

  if (!resp.ok) {
    const err = await resp.json().catch(() => ({ error: resp.statusText }))
    const msg = [502, 503, 504].includes(resp.status) ? SERVER_DOWN : err.error || resp.statusText
    throw new ApiError(msg, resp.status)
  }

  const text = await resp.text()
  if (!text) return undefined as T
  return JSON.parse(text)
}
