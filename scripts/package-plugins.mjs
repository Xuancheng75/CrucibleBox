import AdmZip from 'adm-zip'
import { createHash } from 'node:crypto'
import { mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { sha256 } from './plugin-artifact-provenance.mjs'
import { readTauriVersion } from './tauri-version.mjs'

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const repositoryRoot = resolve(scriptDirectory, '..')
const outputDirectory = resolve(repositoryRoot, 'artifacts', 'plugins')
const catalog = JSON.parse(readFileSync(resolve(scriptDirectory, 'plugin-catalog.json'), 'utf8'))
const hostPackage = JSON.parse(readFileSync(resolve(repositoryRoot, 'package.json'), 'utf8'))
// Electron 发布链默认继续使用根 package.json（1.7.3 冻结线）；Tauri 工作流显式传入
// --tauri，避免两条发布线互相污染版本清单。
const applicationVersion = process.argv.includes('--tauri')
  ? readTauriVersion(repositoryRoot)
  : hostPackage.version
const fixedTimestamp = new Date(2000, 0, 1, 0, 0, 0)
const artifacts = []

mkdirSync(outputDirectory, { recursive: true })

for (const plugin of catalog) {
  const pluginDirectory = resolve(repositoryRoot, 'plugins', plugin.id)
  const packageJson = JSON.parse(readFileSync(resolve(pluginDirectory, 'package.json'), 'utf8'))
  const manifest = JSON.parse(readFileSync(resolve(pluginDirectory, 'plugin.json'), 'utf8'))

  if (packageJson.version !== manifest.version) {
    throw new Error(
      `${plugin.id}: package version ${packageJson.version} does not match manifest ${manifest.version}`
    )
  }

  const zip = new AdmZip()
  const files = []
  for (const runtimeFile of [...plugin.runtimeFiles].sort()) {
    const content = readFileSync(resolve(pluginDirectory, runtimeFile))
    zip.addFile(runtimeFile, content, '', 0o644)
    zip.getEntry(runtimeFile).header.time = fixedTimestamp
    files.push({ path: runtimeFile, sha256: sha256(content), size: content.byteLength })
  }

  const outputPath = resolve(outputDirectory, `${plugin.id}-${manifest.version}.zip`)
  zip.writeZip(outputPath)
  const digest = createHash('sha256').update(readFileSync(outputPath)).digest('hex')
  artifacts.push({
    id: plugin.id,
    version: manifest.version,
    artifact: `${plugin.id}-${manifest.version}.zip`,
    sha256: digest,
    size: statSync(outputPath).size,
    manifestVersion: manifest.manifestVersion ?? 1,
    backend: manifest.backend !== false,
    backendApiVersion: manifest.backend === false ? null : (manifest.backendApiVersion ?? 1),
    rendererApiVersion: manifest.rendererApiVersion ?? 1,
    files
  })
  console.log(`[plugins] packaged ${plugin.id} ${manifest.version} sha256=${digest}`)
}

writeFileSync(
  resolve(outputDirectory, 'manifest.json'),
  `${JSON.stringify(
    {
      schemaVersion: 1,
      application: { name: hostPackage.name, version: applicationVersion },
      plugins: artifacts
    },
    null,
    2
  )}\n`
)
rmSync(resolve(outputDirectory, 'manifest.sig.json'), { force: true })
