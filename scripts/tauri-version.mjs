import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

/**
 * Tauri 是当前唯一的生产运行线。根 package.json 保留 1.7.3 仅供冻结
 * Electron 工具链使用，因此发布脚本不得从根 package.json 推导版本。
 */
export function readTauriVersion(repositoryRoot) {
  const configPath = resolve(repositoryRoot, 'src-tauri', 'tauri.conf.json')
  const config = JSON.parse(readFileSync(configPath, 'utf8'))
  if (
    typeof config.version !== 'string' ||
    !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/u.test(config.version)
  ) {
    throw new Error(`Invalid Tauri version in ${configPath}`)
  }
  return config.version
}
