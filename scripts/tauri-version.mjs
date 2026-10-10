import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

/**
 * Tauri 是当前唯一的生产运行线。发布版本以 Tauri 配置为权威来源；
 * 根工程、前端与 Rust 包须由版本校验统一核对。
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
