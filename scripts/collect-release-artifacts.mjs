import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

/**
 * collect-release-artifacts — 把发布产物聚合到 artifacts/publish（与 CI release.yml 共用同一实现）。
 *
 * 输入（均已由发布链先期生成）：
 * - release/ 下的安装器、blockmap、更新元数据（latest.yml|beta.yml）
 * - artifacts/plugins/*.zip + manifest.json + manifest.sig.json
 * - artifacts/sbom/*.cdx.json（7 份）
 * - artifacts/release-manifest.json（由 build-release-manifest 生成）
 *
 * 用法：node scripts/collect-release-artifacts.mjs [--channel latest|beta]
 */

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const repositoryRoot = resolve(scriptDirectory, '..')
const publishDirectory = resolve(repositoryRoot, 'artifacts', 'publish')
const releaseDirectory = resolve(repositoryRoot, 'release')
const pluginArtifactsDirectory = resolve(repositoryRoot, 'artifacts', 'plugins')
const sbomDirectory = resolve(repositoryRoot, 'artifacts', 'sbom')

const channelArg = process.argv.includes('--channel')
  ? process.argv[process.argv.indexOf('--channel') + 1]
  : undefined

function deriveChannel() {
  const version = JSON.parse(readFileSync(resolve(repositoryRoot, 'package.json'), 'utf8')).version
  if (!version.includes('-')) return 'latest'
  const prerelease = version.split('-', 2)[1].split('.', 2)[0].toLowerCase()
  if (prerelease !== 'beta') {
    throw new Error(`Only beta prereleases are supported, received ${prerelease}`)
  }
  return 'beta'
}

function main() {
  const channel = channelArg ?? deriveChannel()
  if (channel !== 'latest' && channel !== 'beta') {
    throw new Error(`Unsupported channel: ${channel}`)
  }
  const metadataName = `${channel}.yml`

  if (!existsSync(releaseDirectory)) {
    throw new Error(`Release directory does not exist: ${releaseDirectory}`)
  }
  if (!existsSync(pluginArtifactsDirectory)) {
    throw new Error(`Plugin artifacts directory does not exist: ${pluginArtifactsDirectory}`)
  }
  if (!existsSync(sbomDirectory)) {
    throw new Error(`SBOM directory does not exist: ${sbomDirectory}`)
  }

  mkdirSync(publishDirectory, { recursive: true })

  const installers = readdirSync(releaseDirectory).filter((name) => name.endsWith('-setup.exe'))
  if (installers.length !== 1) {
    throw new Error(`Expected one Windows installer in release/, found ${installers.length}`)
  }
  const installerName = installers[0]
  const releaseFiles = [installerName, `${installerName}.blockmap`, metadataName]
  for (const name of releaseFiles) {
    const source = resolve(releaseDirectory, name)
    if (!existsSync(source)) throw new Error(`Release artifact missing: ${name}`)
    copyFileSync(source, resolve(publishDirectory, name))
  }

  for (const name of readdirSync(pluginArtifactsDirectory)) {
    if (!/\.(zip|json)$/u.test(name)) continue
    copyFileSync(resolve(pluginArtifactsDirectory, name), resolve(publishDirectory, name))
  }

  for (const name of readdirSync(sbomDirectory)) {
    if (!name.endsWith('.cdx.json')) continue
    copyFileSync(resolve(sbomDirectory, name), resolve(publishDirectory, name))
  }

  const manifestSource = resolve(repositoryRoot, 'artifacts', 'release-manifest.json')
  if (!existsSync(manifestSource)) {
    throw new Error('release-manifest.json is missing; run build:release-manifest first')
  }
  copyFileSync(manifestSource, resolve(publishDirectory, 'release-manifest.json'))

  const collected = readdirSync(publishDirectory).filter((name) => name !== 'SHA256SUMS')
  console.log(
    `[release-collect] copied ${collected.length} artifacts to ${publishDirectory} (channel=${channel})`
  )
}

main()
