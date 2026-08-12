import { resolve, sep } from 'path'

export function resolvePluginAssetPath(
  pluginsDir: string,
  pluginName: string,
  pathname: string
): string | null {
  const base = resolve(pluginsDir)
  const pluginDir = resolve(base, pluginName)

  if (pluginDir !== base && !pluginDir.startsWith(base + sep)) {
    return null
  }

  const fullPath = resolve(pluginDir, '.' + pathname)

  if (fullPath !== pluginDir && !fullPath.startsWith(pluginDir + sep)) {
    return null
  }

  return fullPath
}