// Experimental Next frame bootstrap. The native host owns session issuance.
;(() => {
  const root = document.getElementById('root')
  const nonce = root.dataset.sessionToken
  let connected = false
  let disposed = false
  let port
  let unmount
  let appearance
  let validateFilesDropped
  const fileListeners = new Set()
  let validateAppearance
  const appearanceListeners = new Set()
  function applyAppearance(value) {
    const checked = validateAppearance(JSON.stringify(value))
    for (const [key, item] of Object.entries(checked.cssVars))
      document.documentElement.style.setProperty(key, item)
    document.documentElement.style.setProperty(
      'background',
      checked.cssVars['--ob-color-bg-layout']
    )
    document.documentElement.style.colorScheme = checked.mode
    document.documentElement.dataset.obTheme = checked.cssVars['--ob-theme-id']
    appearance = Object.freeze({ ...checked, cssVars: Object.freeze({ ...checked.cssVars }) })
    for (const listener of appearanceListeners) {
      try {
        listener(appearance)
      } catch (error) {
        console.error(error)
      }
    }
  }
  const pending = new Map()
  function close(reason) {
    if (disposed) return
    disposed = true
    clearTimeout(handshakeTimer)
    removeEventListener('message', connect)
    port?.close()
    for (const item of pending.values()) {
      clearTimeout(item.timer)
      item.reject(new Error(reason))
    }
    pending.clear()
    appearanceListeners.clear()
    fileListeners.clear()
    unmount?.()
  }
  function exchange(request) {
    if (disposed) return Promise.reject(new Error('SESSION_DENIED'))
    if (pending.size >= Number(root.dataset.maxInflight)) return Promise.reject(new Error('BUSY'))
    if (pending.has(request.requestId)) return Promise.reject(new Error('REPLAY_DENIED'))
    return new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => {
          pending.delete(request.requestId)
          reject(new Error('TIMEOUT'))
        },
        Number(
          request.method === 'backend.call'
            ? root.dataset.backendTimeout
            : root.dataset.requestTimeout
        )
      )
      pending.set(request.requestId, { resolve, reject, timer })
      try {
        port.postMessage({ kind: 'next-request', request })
      } catch (error) {
        clearTimeout(timer)
        pending.delete(request.requestId)
        reject(error)
      }
    })
  }
  async function connect(event) {
    const data = event.data
    if (
      connected ||
      disposed ||
      event.source !== parent ||
      data?.kind !== 'next-connect' ||
      data.nonce !== nonce ||
      data.wireVersion !== 3 ||
      !/^[a-f0-9]{64}$/.test(data.session) ||
      event.ports.length !== 1
    )
      return
    connected = true
    clearTimeout(handshakeTimer)
    removeEventListener('message', connect)
    port = event.ports[0]
    port.onmessageerror = () => close('INVALID_RESPONSE')
    const handlePortMessage = ({ data }) => {
      if (data?.kind === 'next-files-dropped') {
        try {
          const paths = validateFilesDropped(JSON.stringify(data.paths))
          for (const listener of fileListeners) {
            try {
              listener(paths.slice())
            } catch (error) {
              console.error(error)
            }
          }
        } catch {
          close('INVALID_EVENT')
        }
        return
      }
      if (data?.kind === 'next-appearance') {
        try {
          applyAppearance(data.appearance)
        } catch {
          close('INVALID_APPEARANCE')
        }
        return
      }
      if (data?.kind !== 'next-response') {
        close('INVALID_RESPONSE')
        return
      }
      const item = pending.get(data.response?.requestId)
      if (!item) return // Timed-out responses cannot resolve a different request.
      pending.delete(data.response.requestId)
      clearTimeout(item.timer)
      item.resolve(data.response) // The bundled SDK validates shape, correlation and budgets.
    }
    try {
      ;({ validateAppearance, validateFilesDropped } = await import(
        new URL('./next-api.mjs', location.href).href
      ))
      if (disposed) return
      applyAppearance(data.appearance)
      port.onmessage = handlePortMessage
      port.start()
      const renderer = await import(new URL('./renderer.js', location.href).href)
      if (typeof renderer.mount !== 'function') throw new Error('Missing mount export')
      if (disposed) return
      const cleanup = await renderer.mount({
        root,
        session: data.session,
        exchange,
        appearance,
        onFilesDropped(listener) {
          if (typeof listener !== 'function') throw new Error('INVALID_EVENT')
          fileListeners.add(listener)
          return () => fileListeners.delete(listener)
        },
        onAppearanceChanged(listener) {
          if (typeof listener !== 'function') throw new Error('INVALID_APPEARANCE')
          appearanceListeners.add(listener)
          return () => appearanceListeners.delete(listener)
        }
      })
      if (typeof cleanup === 'function') {
        if (disposed) cleanup()
        else unmount = cleanup
      }
      if (!disposed) port.postMessage({ kind: 'next-mounted' })
    } catch (error) {
      root.textContent = String(error)
      close('INTERNAL_ERROR')
    }
  }
  const handshakeTimer = setTimeout(() => {
    root.textContent = 'Next connection timed out'
    close('TIMEOUT')
  }, Number(root.dataset.handshakeTimeout))
  addEventListener('message', connect)
  addEventListener('pagehide', () => close('SESSION_DENIED'), { once: true })
  parent.postMessage({ kind: 'next-ready', nonce, wireVersion: 3 }, '*')
})()
