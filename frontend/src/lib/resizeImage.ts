// Downscale large photos client-side before upload; falls back to the original on any failure.
const JPEG_QUALITY = 0.9

// A failed decode can yield a transparent/black canvas; sample a few pixels to catch it.
export function looksBlank(ctx: OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D, w: number, h: number) {
  const n = 8
  for (let i = 0; i < n; i++) {
    for (let j = 0; j < n; j++) {
      const x = Math.min(w - 1, Math.floor(((i + 0.5) * w) / n))
      const y = Math.min(h - 1, Math.floor(((j + 0.5) * h) / n))
      const [r, g, b, a] = ctx.getImageData(x, y, 1, 1).data
      if (a > 0 && (r > 8 || g > 8 || b > 8)) return false
    }
  }
  return true
}

// JPEG has no alpha: put transparent pixels on white instead of black.
export function flattenOnWhite(ctx: OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D, w: number, h: number) {
  ctx.globalCompositeOperation = 'destination-over'
  ctx.fillStyle = '#fff'
  ctx.fillRect(0, 0, w, h)
}

export async function resizeForUpload(
  file: File,
  maxSide = 2500,
  minBytes = 3 * 1024 * 1024,
): Promise<Blob | File> {
  if (file.size < minBytes) return file
  try {
    const probe = await createImageBitmap(file)
    const { width, height } = probe
    probe.close()
    const scale = Math.min(1, maxSide / Math.max(width, height))
    const w = Math.max(1, Math.round(width * scale))
    const h = Math.max(1, Math.round(height * scale))

    const bitmap = await createImageBitmap(file, { resizeWidth: w, resizeHeight: h, resizeQuality: 'high' })
    try {
      let blob: Blob
      let ctx: OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D | null
      if (typeof OffscreenCanvas !== 'undefined') {
        const canvas = new OffscreenCanvas(w, h)
        ctx = canvas.getContext('2d')
        if (!ctx) return file
        ctx.drawImage(bitmap, 0, 0)
        if (looksBlank(ctx, w, h)) return file
        flattenOnWhite(ctx, w, h)
        blob = await canvas.convertToBlob({ type: 'image/jpeg', quality: JPEG_QUALITY })
      } else {
        const canvas = document.createElement('canvas')
        canvas.width = w
        canvas.height = h
        ctx = canvas.getContext('2d')
        if (!ctx) return file
        ctx.drawImage(bitmap, 0, 0)
        if (looksBlank(ctx, w, h)) return file
        flattenOnWhite(ctx, w, h)
        blob = await new Promise<Blob>((resolve, reject) =>
          canvas.toBlob((b) => (b ? resolve(b) : reject(new Error('toBlob'))), 'image/jpeg', JPEG_QUALITY),
        )
      }
      return blob.size < file.size ? blob : file
    } finally {
      bitmap.close()
    }
  } catch {
    return file
  }
}
