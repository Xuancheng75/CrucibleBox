import AdmZip from 'adm-zip'
import { createHash } from 'node:crypto'
import { existsSync, readFileSync, statSync } from 'node:fs'
import { dirname, isAbsolute, normalize, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import {
  canonicalJson,
  sha256,
  verifyPluginArtifactManifest
} from './plugin-artifact-provenance.mjs'
import { readTauriVersion } from './tauri-version.mjs'

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const repositoryRoot = resolve(scriptDirectory, '..')
const catalog = JSON.parse(readFileSync(resolve(scriptDirectory, 'plugin-catalog.json'), 'utf8'))
const hostPackage = JSON.parse(readFileSync(resolve(repositoryRoot, 'package.json'), 'utf8'))
// 与 package-plugins.mjs 对齐：只有 Tauri 工作流显式传入 --tauri 才读取 Tauri 版本。
const applicationVersion = process.argv.includes('--tauri')
  ? readTauriVersion(repositoryRoot)
  : hostPackage.version
const expectedPlugins = []

for (const plugin of catalog) {
  const pluginDirectory = resolve(repositoryRoot, 'plugins', plugin.id)
  const packageJson = JSON.parse(readFileSync(resolve(pluginDirectory, 'package.json'), 'utf8'))
  const manifest = JSON.parse(readFileSync(resolve(pluginDirectory, 'plugin.json'), 'utf8'))

  if (packageJson.version !== manifest.version) {
    throw new Error(`${plugin.id}: package.json and plugin.json versions differ`)
  }
  if (manifest.name !== plugin.id) {
    throw new Error(`${plugin.id}: manifest name is ${manifest.name}`)
  }

  for (const entrypoint of [manifest.main, manifest.renderer]) {
    const normalized = normalize(entrypoint).replaceAll('\\', '/')
    if (isAbsolute(entrypoint) || normalized.startsWith('../') || normalized.includes('/../')) {
      throw new Error(`${plugin.id}: unsafe entrypoint ${entrypoint}`)
    }
    if (!plugin.runtimeFiles.includes(normalized)) {
      throw new Error(`${plugin.id}: entrypoint ${entrypoint} is absent from the package allowlist`)
    }
  }

  const artifactPath = resolve(
    repositoryRoot,
    'artifacts',
    'plugins',
    `${plugin.id}-${manifest.version}.zip`
  )
  const zip = new AdmZip(artifactPath)
  const actualEntries = zip
    .getEntries()
    .filter((entry) => !entry.isDirectory)
    .map((entry) => entry.entryName)
    .sort()

  const unsafeEntry = zip.getEntries().find((entry) => {
    const normalized = entry.entryName.replaceAll('\\', '/')
    return normalized.split('/').includes('node_modules') || normalized.split('/').includes('.pnpm')
  })
  if (unsafeEntry) {
    throw new Error(
      `${plugin.id}: artifact must not contain development dependency tree ${unsafeEntry.entryName}`
    )
  }
  const expectedEntries = [...plugin.runtimeFiles].sort()

  if (JSON.stringify(actualEntries) !== JSON.stringify(expectedEntries)) {
    throw new Error(
      `${plugin.id}: artifact entries differ\nexpected=${expectedEntries}\nactual=${actualEntries}`
    )
  }

  for (const runtimeFile of expectedEntries) {
    const packaged = zip.readFile(runtimeFile)
    const built = readFileSync(resolve(pluginDirectory, runtimeFile))
    if (!packaged?.equals(built)) {
      throw new Error(`${plugin.id}: packaged content differs for ${runtimeFile}`)
    }
  }

  const digest = createHash('sha256').update(readFileSync(artifactPath)).digest('hex')
  expectedPlugins.push({
    id: plugin.id,
    version: manifest.version,
    artifact: `${plugin.id}-${manifest.version}.zip`,
    sha256: digest,
    size: statSync(artifactPath).size,
    manifestVersion: manifest.manifestVersion ?? 1,
    backend: manifest.backend !== false,
    backendApiVersion: manifest.backend === false ? null : (manifest.backendApiVersion ?? 1),
    rendererApiVersion: manifest.rendererApiVersion ?? 1,
    files: expectedEntries.map((runtimeFile) => {
      const content = readFileSync(resolve(pluginDirectory, runtimeFile))
      return { path: runtimeFile, sha256: sha256(content), size: content.byteLength }
    })
  })
  console.log(`[plugins] verified ${plugin.id} ${manifest.version} sha256=${digest}`)
}

const artifactDirectory = resolve(repositoryRoot, 'artifacts', 'plugins')
const manifestPath = resolve(artifactDirectory, 'manifest.json')
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'))
const expectedManifest = {
  schemaVersion: 1,
  application: { name: hostPackage.name, version: applicationVersion },
  plugins: expectedPlugins
}
if (canonicalJson(manifest) !== canonicalJson(expectedManifest)) {
  throw new Error('Plugin artifact manifest does not match the packaged files')
}

const signaturePath = resolve(artifactDirectory, 'manifest.sig.json')
const publicKeyPath = process.env.OPENBOX_PLUGIN_VERIFY_KEY_FILE
const expectedKeyId = process.env.OPENBOX_PLUGIN_VERIFY_KEY_ID
const requireSignature = process.env.OPENBOX_REQUIRE_PLUGIN_SIGNATURE === '1'
if (publicKeyPath) {
  if (!existsSync(signaturePath)) throw new Error('Plugin artifact signature is missing')
  const signed = JSON.parse(readFileSync(signaturePath, 'utf8'))
  if (signed.algorithm !== 'Ed25519' || typeof signed.signature !== 'string') {
    throw new Error('Plugin artifact signature metadata is invalid')
  }
  if (expectedKeyId && signed.keyId !== expectedKeyId) {
    throw new Error(`Plugin artifact signature key is ${signed.keyId}; expected ${expectedKeyId}`)
  }
  if (
    !verifyPluginArtifactManifest(manifest, signed.signature, readFileSync(resolve(publicKeyPath)))
  ) {
    throw new Error('Plugin artifact signature verification failed')
  }
  console.log(`[plugins] verified artifact signature from key ${signed.keyId}`)
} else if (requireSignature) {
  throw new Error('OPENBOX_PLUGIN_VERIFY_KEY_FILE is required for release verification')
}
