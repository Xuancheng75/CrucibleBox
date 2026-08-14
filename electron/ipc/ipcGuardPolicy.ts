// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
export function isAllowedHostRendererUrl(
  candidate: string,
  configuredDevelopmentUrl = process.env['ELECTRON_RENDERER_URL'],
  configuredPackagedUrl?: string
): boolean {
  let parsed: URL
  try {
    parsed = new URL(candidate)
  } catch {
    return false
  }

  if (configuredDevelopmentUrl) {
    try {
      return parsed.origin === new URL(configuredDevelopmentUrl).origin
    } catch {
      return false
    }
  }

  if (!configuredPackagedUrl) return false
  try {
    const packaged = new URL(configuredPackagedUrl)
    return (
      parsed.protocol === packaged.protocol &&
      parsed.hostname === packaged.hostname &&
      parsed.port === packaged.port &&
      parsed.username === '' &&
      parsed.password === '' &&
      parsed.pathname === packaged.pathname &&
      parsed.search === ''
    )
  } catch {
    return false
  }
}
