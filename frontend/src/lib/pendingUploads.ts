// Source screenshots whose upload failed after import; RecipeImages offers a retry.
const pending = new Map<string, File[]>()

export function setPending(recipeId: string, files: File[]): void {
  if (files.length) pending.set(recipeId, files)
  else pending.delete(recipeId)
}

export function getPending(recipeId: string): File[] {
  return pending.get(recipeId) ?? []
}

export function clearPending(recipeId: string): void {
  pending.delete(recipeId)
}
