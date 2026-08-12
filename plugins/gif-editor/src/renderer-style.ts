export function retainDynamicStyle(document_: Document, id: string, css: string): () => void {
  let style = document_.getElementById(id) as HTMLStyleElement | null
  if (!style) {
    style = document_.createElement('style')
    style.id = id
    document_.head.appendChild(style)
  }

  style.textContent = css
  const refCount = Number(style.dataset.pluginRefCount ?? '0') + 1
  style.dataset.pluginRefCount = String(refCount)

  return () => {
    if (!style?.isConnected) return
    const remaining = Math.max(0, Number(style.dataset.pluginRefCount ?? '1') - 1)
    if (remaining === 0) style.remove()
    else style.dataset.pluginRefCount = String(remaining)
  }
}
