import { createHash } from 'node:crypto'
import {
  createReadStream,
  existsSync,
  lstatSync,
  readFileSync,
  readdirSync,
  writeFileSync
} from 'node:fs'
import { basename, dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

/**
 * build-release-manifest — 发布链收尾生成单一 release-manifest.json。
 *
 * 在 release:local / CI release 链的最终阶段调用（插件打包+签名+SBOM+安装器均已完成）。
 * 输出 artifacts/release-manifest.json，作为版本/插件/摘要/签名/SBOM/安装器/更新元数据的唯一清单。
 * 任何必需输入缺失即失败（fail-closed），不生成残缺清单。
 *
 * 用法：node scripts/build-release-manifest.mjs [--channel latest|beta]
 */

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const repositoryRoot = resolve(scriptDirectory, '..')
const artifactsDirectory = resolve(repositoryRoot, 'artifacts')
const pluginArtifactsDirectory = resolve(artifactsDirectory, 'plugins')
const sbomDirectory = resolve(artifactsDirectory, 'sbom')
const releaseDirectory = resolve(repositoryRoot, 'release')

const channelArg = process.argv.includes('--channel')
  ? process.argv[process.argv.indexOf('--channel') + 1]
  : undefined

function readJson(path, label) {
  if (!existsSync(path)) throw new Error(`${label} does not exist: ${path}`)
  return JSON.parse(readFileSync(path, 'utf8'))
}

function sha256File(path) {
  const hash = createHash('sha256')
  const stream = createReadStream(path)
  return new Promise((resolveHash, reject) => {
    stream.on('error', reject)
    stream.on('data', (chunk) => hash.update(chunk))
    stream.on('end', () => resolveHash(hash.digest('hex')))
  })
}

function assertRegularFile(path, label) {
  if (!existsSync(path)) throw new Error(`${label} does not exist: ${path}`)
  if (lstatSync(path).isSymbolicLink() || !lstatSync(path).isFile()) {
    throw new Error(`${label} must be a regular file: ${path}`)
  }
}

function resolveReleaseFile(filename, label) {
  if (filename !== basename(filename) || filename.includes('\0')) {
    throw new Error(`${label} must be a file in the release root`)
  }
  const path = resolve(releaseDirectory, filename)
  assertRegularFile(path, label)
  return path
}

async function main() {
  const hostPackage = readJson(resolve(repositoryRoot, 'package.json'), 'package.json')
  const pluginsManifest = readJson(
    resolve(pluginArtifactsDirectory, 'manifest.json'),
    'plugins artifact manifest.json'
  )

  const pluginSignaturePath = resolve(pluginArtifactsDirectory, 'manifest.sig.json')
  const pluginSignature = existsSync(pluginSignaturePath)
    ? readJson(pluginSignaturePath, 'plugins artifact manifest.sig.json')
    : null

  // SBOM 文件清单（release 必须完整 7 份）
  if (!existsSync(sbomDirectory)) {
    throw new Error(`SBOM directory does not exist: ${sbomDirectory}`)
  }
  const sbomFiles = readdirSync(sbomDirectory)
    .filter((name) => name.endsWith('.cdx.json'))
    .sort()
  if (sbomFiles.length !== 7) {
    throw new Error(`Expected 7 SBOM files (openbox + 6 plugins), found ${sbomFiles.length}`)
  }

  // 安装器 + 更新元数据
  const channel = channelArg ?? (hostPackage.version.includes('-') ? 'beta' : 'latest')
  if (channel !== 'latest' && channel !== 'beta') {
    throw new Error(`Unsupported channel: ${channel}`)
  }
  const metadataName = `${channel}.yml`
  const metadataPath = resolveReleaseFile(metadataName, 'update metadata')
  const metadataText = readFileSync(metadataPath, 'utf8')

  const readScalar = (source, key) => {
    const match = source.match(new RegExp(`^${key}:\\s*(.+?)\\s*$`, 'mu'))
    if (!match) throw new Error(`Update metadata is missing ${key}`)
    const value = match[1].trim()
    return (value.startsWith("'") && value.endsWith("'")) ||
      (value.startsWith('"') && value.endsWith('"'))
      ? value.slice(1, -1)
      : value
  }

  const metadataVersion = readScalar(metadataText, 'version')
  if (metadataVersion !== hostPackage.version) {
    throw new Error(
      `Update metadata version ${metadataVersion} does not match package ${hostPackage.version}`
    )
  }
  const installerName = readScalar(metadataText, 'path')
  const metadataSha512 = readScalar(metadataText, 'sha512')
  if (!installerName.endsWith('-setup.exe')) {
    throw new Error('Update path is not an NSIS installer')
  }

  const installerPath = resolveReleaseFile(installerName, 'Windows installer')
  const blockmapPath = resolveReleaseFile(`${installerName}.blockmap`, 'Windows blockmap')
  const installerSha256 = await sha256File(installerPath)
  const installerSha512 = createHash('sha512').update(readFileSync(installerPath)).digest('base64')
  if (installerSha512 !== metadataSha512) {
    throw new Error('Windows installer SHA-512 does not match update metadata')
  }

  const manifest = {
    schemaVersion: 1,
    generatedAt: new Date().toISOString(),
    application: { name: hostPackage.name, version: hostPackage.version },
    channel,
    updateMetadata: metadataName,
    plugins: pluginsManifest.plugins.map((plugin) => ({
      id: plugin.id,
      version: plugin.version,
      artifact: plugin.artifact,
      sha256: plugin.sha256,
      size: plugin.size,
      manifestVersion: plugin.manifestVersion,
      backend: plugin.backend,
      backendApiVersion: plugin.backendApiVersion,
      rendererApiVersion: plugin.rendererApiVersion,
      files: plugin.files
    })),
    pluginSignature,
    sbom: sbomFiles,
    installer: {
      filename: installerName,
      blockmap: `${installerName}.blockmap`,
      sha256: installerSha256,
      sha512: installerSha512
    }
  }

  const outputPath = resolve(artifactsDirectory, 'release-manifest.json')
  writeFileSync(outputPath, `${JSON.stringify(manifest, null, 2)}\n`)
  console.log(
    `[release-manifest] wrote ${outputPath} (${manifest.plugins.length} plugins, channel=${channel})`
  )
}

// 顶层 await 需要 ESM 直接执行；用 Promise 包装保持与其它脚本一致的 throw 行为
main().catch((error) => {
  console.error(`[release-manifest] ${error.message}`)
  process.exit(1)
})
