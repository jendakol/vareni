import { apiFetch } from './client'
import type { Recipe } from './recipes'

export interface DiscoverRequest {
  prompt?: string
  count?: number
  planning_for?: 'both' | 'me'
}

export interface SkippedCounts {
  duplicate: number
  restricted: number
  low_score: number
  similar_to_rejected: number
  failed?: number
}

export interface SiteError {
  site: string
  error: string
}

export interface DiscoverResponse {
  discovered: Recipe[]
  skipped: SkippedCounts
  errors: SiteError[]
}

export async function discover(req: DiscoverRequest, signal?: AbortSignal): Promise<DiscoverResponse> {
  return apiFetch('/discover', {
    method: 'POST',
    body: JSON.stringify(req),
    signal,
  })
}
