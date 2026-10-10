import {
  existsSync,
  mkdirSync,
  readdirSync,
  rmSync,
  statSync,
  copyFileSync,
  writeFileSync
} from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnFileSync } from './spawn-file.mjs'

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const outputDirectory = resolve(repositoryRoot, 'src-tauri', 'resources', '7zip')
const version = '26.03'
const archiveName = `7z${version.replace('.', '')}-extra.7z`
const url = `https://www.7-zip.org/a/${archiveName}`
const bootstrapUrl = 'https://www.7-zip.org/a/7zr.exe'

async function download(url, destination) {
  let lastError
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    try {
      const response = await fetch(url, { redirect: 'follow', signal: AbortSignal.timeout(30_000) })
      if (!response.ok) throw new Error(`HTTP ${response.status}`)
      writeFileSync(destination, Buffer.from(await response.arrayBuffer()))
      return
    } catch (error) {
      lastError = error
    }
  }
  throw new Error(`Failed to download ${url} after 3 attempts: ${lastError}`)
}

function findFile(root, name) {
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const path = join(root, entry.name)
    if (entry.isFile() && entry.name.toLowerCase() === name.toLowerCase()) return path
    if (entry.isDirectory()) {
      const found = findFile(path, name)
      if (found) return found
    }
  }
  return null
}

async function extractorPath() {
  const candidates = [
    process.env.SEVENZIP_EXTRACTOR,
    'C:\\Program Files\\7-Zip\\7z.exe',
    'C:\\Program Files (x86)\\7-Zip\\7z.exe'
  ].filter(Boolean)
  const found = candidates.find(
    (candidate) => existsSync(candidate) && statSync(candidate).isFile()
  )
  if (found) return found
  const bootstrapPath = join(tmpdir(), 'cruciblebox-7zr.exe')
  if (!existsSync(bootstrapPath)) await download(bootstrapUrl, bootstrapPath)
  return bootstrapPath
}

const archivePath = join(tmpdir(), archiveName)
const extractionDirectory = join(tmpdir(), `cruciblebox-${archiveName}`)
if (!existsSync(archivePath)) {
  await download(url, archivePath)
}
rmSync(extractionDirectory, { recursive: true, force: true })
mkdirSync(extractionDirectory, { recursive: true })
spawnFileSync(await extractorPath(), ['x', '-y', `-o${extractionDirectory}`, archivePath])
const sevenZipExe = join(extractionDirectory, 'x64', '7za.exe')
const sevenZipDll = join(extractionDirectory, 'x64', '7za.dll')
const license = findFile(extractionDirectory, 'License.txt')
if (!existsSync(sevenZipExe) || !existsSync(sevenZipDll) || !license) {
  throw new Error(
    'The official 7z extra archive did not contain the x64 7za runtime and License.txt'
  )
}

rmSync(outputDirectory, { recursive: true, force: true })
mkdirSync(outputDirectory, { recursive: true })
copyFileSync(sevenZipExe, join(outputDirectory, '7za.exe'))
copyFileSync(sevenZipDll, join(outputDirectory, '7za.dll'))
copyFileSync(license, join(outputDirectory, 'License.txt'))
console.log(`[7zip] staged ${version} resources in ${outputDirectory}`)
