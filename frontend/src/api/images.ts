import { apiFetch, ApiError, csrfToken, SERVER_DOWN, BASE } from './client'

export interface RecipeImage {
  id: string
  recipe_id: string
  kind: 'cover' | 'screenshot'
  width: number
  height: number
  created_at: string
}

export const ACCEPTED_TYPES = 'image/jpeg,image/png,image/webp'
/** Picker hint: a MIME list hides the camera on Android; types are checked after selection. */
export const PICKER_ACCEPT = 'image/*'

export function isAcceptedType(file: File): boolean {
  return ACCEPTED_TYPES.split(',').includes(file.type)
}

export function isHeic(file: File): boolean {
  return /^image\/hei[cf]$/.test(file.type) || /\.hei[cf]$/i.test(file.name)
}

/** HEIC/HEIF to JPEG: native decode where the browser can (Safari), else a lazily loaded wasm decoder. */
export async function heicToJpeg(file: File): Promise<File> {
  const name = file.name.replace(/\.hei[cf]$/i, '') + '.jpg'
  try {
    const bmp = await createImageBitmap(file)
    const canvas = document.createElement('canvas')
    canvas.width = bmp.width
    canvas.height = bmp.height
    canvas.getContext('2d')!.drawImage(bmp, 0, 0)
    bmp.close()
    const blob = await new Promise<Blob | null>((r) => canvas.toBlob(r, 'image/jpeg', 0.92))
    if (blob && blob.size > 0) return new File([blob], name, { type: 'image/jpeg' })
  } catch {
    // no native HEIC decoder
  }
  const { default: heic2any } = await import('heic2any')
  const out = await heic2any({ blob: file, toType: 'image/jpeg', quality: 0.92 })
  const blob = Array.isArray(out) ? out[0] : out
  return new File([blob], name, { type: 'image/jpeg' })
}

/** Converts HEIC, keeps accepted types, reports the rest via `onError`. */
export async function prepareFiles(files: File[], onError: (msg: string) => void): Promise<File[]> {
  const out: File[] = []
  for (const f of files) {
    let file = f
    if (isHeic(f)) {
      try {
        file = await heicToJpeg(f)
      } catch {
        onError('Fotku HEIC se nepodařilo převést, zkuste JPEG')
        continue
      }
    }
    if (isAcceptedType(file)) out.push(file)
    else onError('Podporované formáty jsou JPEG, PNG a WebP')
  }
  return out
}

export function imageUrl(recipeId: string, imageId: string, variant: 'full' | 'thumb'): string {
  return `${BASE}/recipes/${recipeId}/images/${imageId}/${variant}`
}

// No cache-buster needed: the server's ETag handles a replaced cover
export function publicCoverUrl(slug: string, variant: 'full' | 'thumb'): string {
  return `${BASE}/public/recipes/${slug}/cover/${variant}`
}

function buildForm(kind: string, files: (File | Blob)[]): FormData {
  const form = new FormData()
  form.append('kind', kind) // the server requires it before the files
  for (const f of files) form.append('image', f, f instanceof File ? f.name : 'image.jpg')
  return form
}

function uploadWithProgress(
  recipeId: string,
  form: FormData,
  onProgress: (fraction: number) => void,
): Promise<RecipeImage[]> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest()
    xhr.open('POST', `${BASE}/recipes/${recipeId}/images`)
    xhr.withCredentials = true
    if (csrfToken.value) xhr.setRequestHeader('X-CSRF-Token', csrfToken.value)
    xhr.upload.onprogress = (e) => {
      if (e.lengthComputable) onProgress(e.loaded / e.total)
    }
    xhr.onerror = () => reject(new ApiError(SERVER_DOWN, 0))
    xhr.onload = () => {
      let body: any = null
      try {
        body = xhr.responseText ? JSON.parse(xhr.responseText) : null
      } catch {
        // non-JSON error body
      }
      if (xhr.status >= 200 && xhr.status < 300) return resolve(body)
      // Never redirect here: the caller still holds the photo
      const msg = [502, 503, 504].includes(xhr.status)
        ? SERVER_DOWN
        : xhr.status === 401
          ? 'Přihlášení vypršelo'
          : body?.error || xhr.statusText
      reject(new ApiError(msg, xhr.status))
    }
    xhr.send(form)
  })
}

export function uploadImages(
  recipeId: string,
  kind: 'cover' | 'screenshot',
  files: File[] | Blob[],
  onProgress?: (fraction: number) => void,
): Promise<RecipeImage[]> {
  const form = buildForm(kind, files)
  if (onProgress) return uploadWithProgress(recipeId, form, onProgress)
  return apiFetch<RecipeImage[]>(`/recipes/${recipeId}/images`, { method: 'POST', body: form }, false)
}

export function deleteImage(recipeId: string, imageId: string): Promise<void> {
  return apiFetch<void>(`/recipes/${recipeId}/images/${imageId}`, { method: 'DELETE' })
}
