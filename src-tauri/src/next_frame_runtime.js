// Experimental Next frame bootstrap. The native host owns session issuance.
;(() => {
  const root = document.getElementById('root')
  const nonce = root.dataset.sessionToken
  const wireVersion = 4
  let connected = false
  let disposed = false
  let port
  let unmount
  let appearance
  let validateFilesDropped
  const fileListeners = new Set()
  const visibilityListeners = new Set()
  let visible = true
  let validateAppearance
  let validateThemeStyle
  let themeStyleEnabled = false
  let styleElement
  let decorations
  function applyThemeStyle(value) {
    if (!themeStyleEnabled) throw new Error('THEME_STYLE_NOT_ENABLED')
    const checked = validateThemeStyle(JSON.stringify(value))
    styleElement?.remove()
    decorations?.remove()
    styleElement = undefined
    decorations = undefined
    delete document.documentElement.dataset.obThemeVersion
    if (!checked.css) return
    styleElement = document.createElement('style')
    styleElement.textContent = checked.css
    document.head.append(styleElement)
    decorations = document.createElement('div')
    decorations.className = 'cbx-theme-decorations'
    decorations.setAttribute('aria-hidden', 'true')
    for (const layer of checked.layers) {
      const item = document.createElement('div')
      item.dataset.layerId = layer.id
      item.dataset.layerScope = layer.scope
      item.textContent = layer.text
      decorations.append(item)
    }
    document.body.prepend(decorations)
    document.documentElement.dataset.obThemeVersion = '2'
    document.documentElement.dataset.obThemeSurface = 'plugin'
  }
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
  const activeClientIds = new Set()
  let requestSequence = 0
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
    activeClientIds.clear()
    appearanceListeners.clear()
    fileListeners.clear()
    visibilityListeners.clear()
    styleElement?.remove()
    decorations?.remove()
    delete document.documentElement.dataset.obThemeVersion
    unmount?.()
  }
  function exchange(request) {
    if (disposed) return Promise.reject(new Error('SESSION_DENIED'))
    if (pending.size >= Number(root.dataset.maxInflight)) return Promise.reject(new Error('BUSY'))
    const clientId = request.requestId
    if (
      typeof clientId !== 'string' ||
      !clientId.length ||
      Array.from(clientId).length > 128 ||
      /[\ud800-\udfff]/u.test(clientId)
    )
      return Promise.reject(new Error('INVALID_REQUEST'))
    if (activeClientIds.has(clientId)) return Promise.reject(new Error('REPLAY_DENIED'))
    if (requestSequence >= Number.MAX_SAFE_INTEGER)
      return Promise.reject(new Error('SESSION_EXHAUSTED'))
    const wireId = `renderer-${++requestSequence}`
    activeClientIds.add(clientId)
    return new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => {
          pending.delete(wireId)
          activeClientIds.delete(clientId)
          reject(new Error('TIMEOUT'))
        },
        Number(
          request.method === 'backend.call'
            ? root.dataset.backendTimeout
            : request.target?.kind === 'platform' && request.method === 'files.choose'
              ? root.dataset.interactiveTimeout
              : root.dataset.requestTimeout
        )
      )
      pending.set(wireId, { resolve, reject, timer, clientId })
      try {
        port.postMessage({ kind: 'next-request', request: { ...request, requestId: wireId } })
      } catch (error) {
        clearTimeout(timer)
        pending.delete(wireId)
        activeClientIds.delete(clientId)
        reject(error)
      }
    })
  }
  function receiveResponse(response) {
    const item = pending.get(response?.requestId)
    if (!item) return // A late wire reply cannot resolve a later client call.
    pending.delete(response.requestId)
    activeClientIds.delete(item.clientId)
    clearTimeout(item.timer)
    item.resolve({ ...response, requestId: item.clientId }) // SDK correlation remains unchanged.
  }
  async function connect(event) {
    const data = event.data
    if (
      connected ||
      disposed ||
      event.source !== parent ||
      data?.kind !== 'next-connect' ||
      data.nonce !== nonce ||
      data.wireVersion !== wireVersion ||
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
      if (data?.kind === 'next-visibility') {
        if (typeof data.visible !== 'boolean') {
          close('INVALID_EVENT')
          return
        }
        document.documentElement.dataset.obPageVisible = String(data.visible)
        visible = data.visible
        for (const listener of visibilityListeners) {
          try {
            listener(visible)
          } catch (error) {
            console.error(error)
          }
        }
        return
      }
      if (data?.kind === 'next-theme-style') {
        try {
          applyThemeStyle(data.style)
        } catch {
          close('INVALID_THEME_STYLE')
        }
        return
      }
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
      receiveResponse(data.response)
    }
    try {
      ;({ validateAppearance, validateFilesDropped, validateThemeStyle } = await import(
        new URL('./frame-validation.mjs', location.href).href
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
        enableThemeStyle(version) {
          if (version !== 2 || disposed) return false
          if (!themeStyleEnabled) {
            themeStyleEnabled = true
            port.postMessage({ kind: 'next-theme-style-subscribe', version: 2 })
          }
          return true
        },
        onFilesDropped(listener) {
          if (typeof listener !== 'function') throw new Error('INVALID_EVENT')
          fileListeners.add(listener)
          return () => fileListeners.delete(listener)
        },
        onVisibilityChanged(listener) {
          if (typeof listener !== 'function') throw new Error('INVALID_EVENT')
          visibilityListeners.add(listener)
          listener(visible)
          return () => visibilityListeners.delete(listener)
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
  parent.postMessage({ kind: 'next-ready', nonce, wireVersion }, '*')
})()
