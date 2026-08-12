import { spawnSync } from 'node:child_process'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const repositoryRoot = resolve(scriptDirectory, '..')
const outputDirectory = resolve(repositoryRoot, 'artifacts', 'sbom')
const catalog = JSON.parse(readFileSync(resolve(scriptDirectory, 'plugin-catalog.json'), 'utf8'))
const npmCli = process.env.npm_execpath
if (!npmCli) throw new Error('generate:sbom must be run through npm')

mkdirSync(outputDirectory, { recursive: true })

for (const target of [
  { id: 'openbox', directory: repositoryRoot },
  ...catalog.map((plugin) => ({
    id: plugin.id,
    directory: resolve(repositoryRoot, 'plugins', plugin.id)
  }))
]) {
  const result = spawnSync(process.execPath, [npmCli, 'sbom', '--sbom-format', 'cyclonedx'], {
    cwd: target.directory,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    windowsHide: true
  })
  if (result.status !== 0) {
    throw new Error(
      `${target.id}: npm sbom failed\n${result.error?.message ?? result.stderr ?? result.stdout}`
    )
  }
  const sbom = JSON.parse(result.stdout)
  if (sbom.bomFormat !== 'CycloneDX' || !sbom.metadata?.component?.name) {
    throw new Error(`${target.id}: npm returned an invalid CycloneDX SBOM`)
  }
  const outputPath = resolve(outputDirectory, `${target.id}.cdx.json`)
  writeFileSync(outputPath, `${JSON.stringify(sbom, null, 2)}\n`)
  console.log(`[sbom] wrote ${target.id}.cdx.json with ${sbom.components?.length ?? 0} components`)
}
