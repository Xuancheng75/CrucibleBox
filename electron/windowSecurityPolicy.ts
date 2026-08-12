import { isAllowedHostRendererUrl } from './ipc/ipcGuardPolicy'

export function isAllowedExternalUrl(candidate: string): boolean {
  let parsed: URL
  try {
    parsed = new URL(candidate)
  } catch {
    return false
  }

  return (
    (parsed.protocol === 'https:' || parsed.protocol === 'http:') &&
    parsed.username === '' &&
    parsed.password === ''
  )
}

export function isAllowedHostNavigation(
  candidate: string,
  configuredDevelopmentUrl = process.env['ELECTRON_RENDERER_URL'],
  configuredPackagedUrl?: string
): boolean {
  return isAllowedHostRendererUrl(candidate, configuredDevelopmentUrl, configuredPackagedUrl)
}

export function isChromiumPermissionAllowed(): boolean {
  return false
}
