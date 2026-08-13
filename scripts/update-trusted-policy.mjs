import { createHash } from 'node:crypto'
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

/**
 * update-trusted-policy — 重算并写回 trusted-service-policies.json 的固定摘要。
 *
 * 与 scripts/verify-trusted-services.mjs 使用完全相同的算法：
 *   digest = sha256( filePath \0 content \0 ) 按排序文件集合累加
 *
 * 用途：unienv（或未来可信服务）插件源码/dist 重建后，运行本脚本把最新摘要写回策略文件，
 * 消灭"构建产物一变、门禁就红、必须人工重钉 digest"的手工环节。CI 的 verify 门禁保持不变。
 *
 * 用法：node scripts/update-trusted-policy.mjs
 * 行为：对每个策略条目，按当前 plugins/<name>/ 下的实际文件重算 digest 与 version，
 *       若与策略文件不同则更新并重写；打印新旧值。--dry-run 只打印不写。
 */

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const repositoryRoot = resolve(scriptDirectory, '..')
const policyPath = resolve(repositoryRoot, 'shared', 'trusted-service-policies.json')
const catalog = JSON.parse(readFileSync(resolve(scriptDirectory, 'plugin-catalog.json'), 'utf8'))

const dryRun = process.argv.includes('--dry-run')

function readJson(path, label) {
  return JSON.parse(readFileSync(path, 'utf8'))
}

function calculateDigest(pluginDirectory, files) {
  const hash = createHash('sha256')
  for (const file of [...files].sort()) {
    hash.update(file)
    hash.update('\0')
    hash.update(readFileSync(join(pluginDirectory, ...file.split('/'))))
    hash.update('\0')
  }
  return hash.digest('hex')
}

function main() {
  const policies = readJson(policyPath, 'trusted-service-policies.json')
  let changed = false

  for (const [serviceName, policy] of Object.entries(policies)) {
    const catalogEntry = catalog.find((entry) => entry.id === policy.name)
    if (!catalogEntry) throw new Error(`${serviceName}: plugin is missing from the catalog`)
    const expectedFiles = [...policy.files].sort()
    const runtimeFiles = [...catalogEntry.runtimeFiles].sort()
    if (JSON.stringify(expectedFiles) !== JSON.stringify(runtimeFiles)) {
      throw new Error(
        `${serviceName}: trusted policy and runtime catalog file sets differ; update policy.files first`
      )
    }

    const pluginDirectory = resolve(repositoryRoot, 'plugins', policy.name)
    const manifest = readJson(resolve(pluginDirectory, 'plugin.json'), 'plugin.json')

    const digest = calculateDigest(pluginDirectory, expectedFiles)
    const version = manifest.version

    if (digest !== policy.digest || version !== policy.version) {
      if (dryRun) {
        console.log(
          `[trusted-policy][dry-run] ${serviceName}: version ${policy.version}->${version}, digest ${policy.digest}->${digest}`
        )
      } else {
        console.log(
          `[trusted-policy] ${serviceName}: version ${policy.version}->${version}, digest ${policy.digest}->${digest}`
        )
        policy.version = version
        policy.digest = digest
        changed = true
      }
    } else {
      console.log(`[trusted-policy] ${serviceName}: unchanged (${version} ${digest.slice(0, 12)}…)`)
    }
  }

  if (changed && !dryRun) {
    writeFileSync(policyPath, `${JSON.stringify(policies, null, 2)}\n`)
    console.log(`[trusted-policy] wrote ${policyPath}`)
  } else if (!changed) {
    console.log('[trusted-policy] no changes required')
  }
}

main()
