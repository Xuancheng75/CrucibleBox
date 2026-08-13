import { existsSync, readdirSync, rmSync } from 'node:fs'
import { dirname, relative, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'

/**
 * clean:all — 清理全部构建产物与依赖目录。
 *
 * 仅删除已知的构建缓存/产物/依赖安装目录，绝不触碰：
 * - 用户数据（%APPDATA% 下的 openbox 数据、openbox.db）
 * - .git、docs/、src/、plugin-system/、shared/、database/ 等源码目录
 *
 * 用法：node scripts/clean-all.mjs [--dry-run]
 */

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const projectRoot = resolve(scriptDirectory, '..')

const dryRun = process.argv.includes('--dry-run')

function assertInsideRoot(target) {
  const rel = relative(projectRoot, target)
  if (rel === '' || rel.startsWith('..') || rel.split(sep).includes('..')) {
    throw new Error(`Refusing to remove a path outside the project root: ${target}`)
  }
}

/** 已知的构建产物/依赖目录（相对项目根） */
const fixedTargets = [
  'out', // electron-vite 构建产物
  'release', // electron-builder 打包产物（安装器/win-unpacked/delivery）
  'artifacts', // 插件 zip / manifest / SBOM
  'dist', // 根级 dist（如有）
  'node_modules' // 根依赖
]

/** workspace 子目录里需要清理的条目 */
const workspaceCleanEntries = ['dist', 'node_modules']

/** workspace 根目录（相对于项目根） */
const workspaceRoots = ['plugins', 'packages']

/** 根目录 tsbuildinfo 等增量缓存 */
const rootCacheFiles = ['tsconfig.node.tsbuildinfo']

function collectTargets() {
  const targets = []

  for (const rel of fixedTargets) {
    const abs = resolve(projectRoot, rel)
    if (existsSync(abs)) targets.push(abs)
  }

  for (const ws of workspaceRoots) {
    const wsRoot = resolve(projectRoot, ws)
    if (!existsSync(wsRoot)) continue
    // workspace 根级自身（plugins/dist、plugins/node_modules）
    for (const entry of workspaceCleanEntries) {
      const abs = resolve(wsRoot, entry)
      if (existsSync(abs)) targets.push(abs)
    }
    // 每个插件/包子目录内的 dist 与 node_modules
    let entries
    try {
      entries = readdirSync(wsRoot, { withFileTypes: true })
    } catch {
      continue
    }
    for (const dirent of entries) {
      if (!dirent.isDirectory()) continue
      const child = resolve(wsRoot, dirent.name)
      if (!child.startsWith(`${wsRoot}${sep}`)) continue
      for (const entry of workspaceCleanEntries) {
        const abs = resolve(child, entry)
        if (existsSync(abs)) targets.push(abs)
      }
    }
  }

  for (const file of rootCacheFiles) {
    const abs = resolve(projectRoot, file)
    if (existsSync(abs)) targets.push(abs)
  }

  return targets
}

function main() {
  const targets = collectTargets()
  for (const target of targets) {
    assertInsideRoot(target)
    if (dryRun) {
      console.log(`[clean:all][dry-run] would remove ${relative(projectRoot, target)}`)
    } else {
      rmSync(target, { recursive: true, force: true })
      console.log(`[clean:all] removed ${relative(projectRoot, target)}`)
    }
  }
  const mode = dryRun ? 'dry-run' : 'done'
  console.log(`[clean:all] ${mode}: ${targets.length} targets`)
}

main()
