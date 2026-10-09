import { readFile, writeFile, mkdir, readdir, lstat, copyFile, realpath } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { dirname, resolve, join, relative, isAbsolute } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
const repository = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const metadataKeys = [
  'name',
  'version',
  'license',
  'dependencies',
  'devDependencies',
  'optionalDependencies',
  'peerDependencies',
  'peerDependenciesMeta',
  'bin',
  'engines'
]
const metadata = (object) =>
  Object.fromEntries(
    metadataKeys.filter((key) => object[key] !== undefined).map((key) => [key, object[key]])
  )
// npm run exports a global allow-scripts option as a project CLI option. Every
// install below already disables lifecycle scripts; do not inherit that option.
export function independentNpmEnvironment() {
  const env = { ...process.env }
  for (const key of Object.keys(env))
    if (key.toLowerCase() === 'npm_config_allow_scripts') delete env[key]
  return env
}
export async function exportSource(id, destination) {
  const catalog = JSON.parse(
    await readFile(join(repository, 'scripts/next-plugin-catalog.json'), 'utf8')
  )
  if (!catalog.some((entry) => entry.id === id)) throw Error('Not a Next official plugin')
  destination = resolve(destination)
  const source = await realpath(join(repository, 'plugins', id))
  const rel = relative(repository, destination)
  if (
    existsSync(destination) ||
    rel === '' ||
    (!isAbsolute(rel) && rel !== '..' && !rel.startsWith('../') && !rel.startsWith('..\\'))
  )
    throw Error('Export requires a new directory outside the repository')
  await mkdir(destination, { recursive: true })
  const inventory = []
  const copy = async (from, to) => {
    const info = await lstat(from)
    if (info.isSymbolicLink()) throw Error('Linked source is not independently exportable')
    if (info.isDirectory()) {
      await mkdir(to, { recursive: true })
      for (const name of await readdir(from))
        if (name !== 'node_modules') await copy(join(from, name), join(to, name))
    } else if (info.isFile()) {
      await copyFile(from, to)
      const bytes = await readFile(to)
      inventory.push({
        path: relative(destination, to).replaceAll('\\', '/'),
        bytes: bytes.length,
        sha256: createHash('sha256').update(bytes).digest('hex')
      })
    } else throw Error('Non-regular source input')
  }
  for (const name of await readdir(source))
    if (
      ['src', 'scripts', 'tests', 'vendor'].includes(name) ||
      /^(package|plugin|plugin.build|tsconfig[^/]*|vitest.config)\.(json|ts|mjs)$/.test(name) ||
      /^README|^LICENSE/.test(name)
    )
      await copy(join(source, name), join(destination, name))
  const pkg = JSON.parse(await readFile(join(source, 'package.json'), 'utf8'))
  const rootLock = JSON.parse(await readFile(join(repository, 'package-lock.json'), 'utf8'))
  const packages = { '': metadata(pkg) }
  // Reuse recorded registry URLs and integrity values; never resolve a newer transitive version.
  for (const [key, value] of Object.entries(rootLock.packages))
    if (key.startsWith('node_modules/') && !value.link) packages[key] = structuredClone(value)
  const prefix = 'plugins/' + id + '/node_modules/'
  for (const [key, value] of Object.entries(rootLock.packages))
    if (key.startsWith(prefix) && !value.link)
      packages['node_modules/' + key.slice(prefix.length)] = structuredClone(value)
  for (const [name, spec] of Object.entries({ ...pkg.dependencies, ...pkg.devDependencies })) {
    if (!spec.startsWith('file:')) continue
    const local = spec.slice(5)
    if (!/^vendor\/[a-z0-9-]+$/.test(local)) throw Error('Undeclared local dependency path')
    const localMetadata = JSON.parse(
      await readFile(join(destination, local, 'package.json'), 'utf8')
    )
    if (localMetadata.name !== name) throw Error('Local dependency identity mismatch')
    packages['node_modules/' + name] = { resolved: local, link: true }
    packages[local] = metadata(localMetadata)
  }
  const lock = {
    name: pkg.name,
    version: pkg.version,
    lockfileVersion: 3,
    requires: true,
    packages
  }
  await writeFile(join(destination, 'package-lock.json'), JSON.stringify(lock, null, 2) + '\n', {
    flag: 'wx'
  })
  const npm = process.env.npm_execpath
  if (!npm) throw Error('Run export through npm, or provide npm_execpath')
  const normalized = spawnSync(
    process.execPath,
    [
      npm,
      'install',
      '--package-lock-only',
      '--offline',
      '--ignore-scripts',
      '--no-audit',
      '--no-fund'
    ],
    {
      cwd: destination,
      env: independentNpmEnvironment(),
      encoding: 'utf8',
      windowsHide: true,
      timeout: 30000
    }
  )
  await writeFile(
    join(destination, 'lock-normalization.log'),
    (normalized.stdout || '') + (normalized.stderr || '')
  )
  if (normalized.error || normalized.status !== 0)
    throw Error(
      'Standalone lock normalization failed: ' + (normalized.error?.message || normalized.stderr)
    )
  for (const item of inventory) {
    const input = join(source, item.path)
    const stat = await lstat(input)
    if (
      !stat.isFile() ||
      stat.isSymbolicLink() ||
      stat.size !== item.bytes ||
      createHash('sha256')
        .update(await readFile(input))
        .digest('hex') !== item.sha256
    )
      throw Error('Source changed during independent export: ' + item.path)
  }
  const finalLock = await readFile(join(destination, 'package-lock.json'))
  const result = {
    id,
    source,
    lockSha256: createHash('sha256').update(finalLock).digest('hex'),
    sourceInventory: inventory
  }
  await writeFile(join(destination, 'source-export.json'), JSON.stringify(result, null, 2) + '\n', {
    flag: 'wx'
  })
  return result
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [id, destination] = process.argv.slice(2)
  if (!id || !destination)
    throw Error('Usage: export-next-plugin-source <official-id> <new-directory>')
  const result = await exportSource(id, destination)
  console.log(
    JSON.stringify({
      id: result.id,
      lockSha256: result.lockSha256,
      files: result.sourceInventory.length,
      destination: resolve(destination)
    })
  )
}
