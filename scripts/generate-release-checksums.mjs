import { createHash } from 'node:crypto'
import { createReadStream, existsSync, lstatSync, readdirSync, writeFileSync } from 'node:fs'
import { basename, dirname, relative, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'

const CHECKSUM_FILE_NAME = 'SHA256SUMS'

function collectFiles(rootDirectory, currentDirectory, outputPath) {
  const files = []
  for (const entry of readdirSync(currentDirectory, { withFileTypes: true })) {
    const absolutePath = resolve(currentDirectory, entry.name)
    const relativePath = relative(rootDirectory, absolutePath).split(sep).join('/')
    if (entry.isSymbolicLink())
      throw new Error(`Release artifacts cannot contain symlinks: ${relativePath}`)
    if (entry.isDirectory()) {
      files.push(...collectFiles(rootDirectory, absolutePath, outputPath))
      continue
    }
    if (!entry.isFile()) throw new Error(`Unsupported release artifact: ${relativePath}`)
    if (resolve(absolutePath) === outputPath || basename(absolutePath) === CHECKSUM_FILE_NAME)
      continue
    if (/\r|\n/u.test(relativePath)) throw new Error('Release artifact path contains a line break')
    files.push({ absolutePath, relativePath })
  }
  return files
}

function sha256File(path) {
  return new Promise((resolveDigest, reject) => {
    const hash = createHash('sha256')
    const stream = createReadStream(path)
    stream.on('error', reject)
    stream.on('data', (chunk) => hash.update(chunk))
    stream.on('end', () => resolveDigest(hash.digest('hex')))
  })
}

export async function generateReleaseChecksums(rootDirectory, outputFile = CHECKSUM_FILE_NAME) {
  const root = resolve(rootDirectory)
  if (!existsSync(root) || !lstatSync(root).isDirectory()) {
    throw new Error(`Release artifact directory does not exist: ${root}`)
  }
  const outputPath = resolve(root, outputFile)
  if (relative(root, outputPath).startsWith('..')) {
    throw new Error('Checksum output must stay inside the release artifact directory')
  }
  const artifacts = collectFiles(root, root, outputPath).sort((left, right) =>
    left.relativePath.localeCompare(right.relativePath, 'en')
  )
  if (artifacts.length === 0) throw new Error('No release artifacts were found')

  const lines = []
  for (const artifact of artifacts) {
    lines.push(`${await sha256File(artifact.absolutePath)}  ${artifact.relativePath}`)
  }
  writeFileSync(outputPath, `${lines.join('\n')}\n`)
  return { artifactCount: artifacts.length, outputPath }
}

const scriptPath = fileURLToPath(import.meta.url)
if (process.argv[1] && resolve(process.argv[1]) === scriptPath) {
  const repositoryRoot = resolve(dirname(scriptPath), '..')
  const rootDirectory = resolve(process.argv[2] ?? resolve(repositoryRoot, 'release-artifacts'))
  const result = await generateReleaseChecksums(
    rootDirectory,
    process.argv[3] ?? CHECKSUM_FILE_NAME
  )
  console.log(`[release] wrote ${result.outputPath} for ${result.artifactCount} artifacts`)
}
