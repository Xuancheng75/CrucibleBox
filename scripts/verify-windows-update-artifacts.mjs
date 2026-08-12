import { createHash } from 'node:crypto'
import { lstatSync, readFileSync, realpathSync } from 'node:fs'
import { basename, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'

const MAX_METADATA_BYTES = 1024 * 1024
const ALLOWED_METADATA = new Set(['latest.yml', 'beta.yml'])

function readScalar(source, key) {
  const match = source.match(new RegExp(`^${key}:\\s*(.+?)\\s*$`, 'mu'))
  if (!match) throw new Error(`Update metadata is missing ${key}`)
  const value = match[1].trim()
  if (
    (value.startsWith("'") && value.endsWith("'")) ||
    (value.startsWith('"') && value.endsWith('"'))
  ) {
    return value.slice(1, -1)
  }
  return value
}

function assertRegularFile(path, label) {
  const stats = lstatSync(path)
  if (stats.isSymbolicLink() || !stats.isFile()) throw new Error(`${label} must be a regular file`)
  return stats
}

function resolveArtifact(root, filename, label) {
  if (filename !== basename(filename) || filename.includes('\0')) {
    throw new Error(`${label} must be a file in the release root`)
  }
  const path = resolve(root, filename)
  const realRoot = `${realpathSync(root)}${sep}`
  const realPath = realpathSync(path)
  if (!realPath.startsWith(realRoot)) throw new Error(`${label} escapes the release root`)
  assertRegularFile(realPath, label)
  return realPath
}

export function verifyWindowsUpdateArtifacts(releaseDirectory, metadataName, expectedVersion) {
  if (!ALLOWED_METADATA.has(metadataName)) throw new Error('Unsupported update metadata channel')
  const root = realpathSync(resolve(releaseDirectory))
  const metadataPath = resolveArtifact(root, metadataName, 'Update metadata')
  const metadataStats = assertRegularFile(metadataPath, 'Update metadata')
  if (metadataStats.size > MAX_METADATA_BYTES)
    throw new Error('Update metadata is unexpectedly large')

  const metadata = readFileSync(metadataPath, 'utf8')
  const version = readScalar(metadata, 'version')
  const installerName = readScalar(metadata, 'path')
  const expectedSha512 = readScalar(metadata, 'sha512')
  if (version !== expectedVersion) {
    throw new Error(`Update metadata version ${version} does not match package ${expectedVersion}`)
  }
  if (!installerName.endsWith('-setup.exe')) throw new Error('Update path is not an NSIS installer')

  const installerPath = resolveArtifact(root, installerName, 'Windows installer')
  resolveArtifact(root, `${installerName}.blockmap`, 'Windows blockmap')
  const actualSha512 = createHash('sha512').update(readFileSync(installerPath)).digest('base64')
  if (actualSha512 !== expectedSha512)
    throw new Error('Windows installer SHA-512 does not match metadata')

  return { installerName, metadataName, version }
}

const entryPath = process.argv[1] ? resolve(process.argv[1]) : ''
if (entryPath === fileURLToPath(import.meta.url)) {
  const releaseDirectory = process.argv[2]
  const metadataName = process.argv[3]
  if (!releaseDirectory || !metadataName) {
    throw new Error(
      'Usage: verify-windows-update-artifacts.mjs <release-directory> <latest.yml|beta.yml>'
    )
  }
  const packageJson = JSON.parse(readFileSync(resolve('package.json'), 'utf8'))
  const result = verifyWindowsUpdateArtifacts(releaseDirectory, metadataName, packageJson.version)
  console.log(
    `[windows-update] verified ${result.metadataName} -> ${result.installerName} (${result.version})`
  )
}
