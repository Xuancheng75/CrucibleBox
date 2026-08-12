import { existsSync } from 'node:fs'
import { join, resolve } from 'node:path'

export function packagedExecutableCandidates(repositoryRoot, platform, arch) {
  if (platform !== 'win32' || arch !== 'x64') {
    throw new Error(`Packaged smoke supports only Windows x64, received ${platform}/${arch}`)
  }
  const releaseDirectory = resolve(repositoryRoot, 'release')
  return [join(releaseDirectory, 'win-unpacked', 'CrucibleBox.exe')]
}

export function resolvePackagedExecutable(
  repositoryRoot,
  platform = process.platform,
  arch = process.arch,
  pathExists = existsSync
) {
  const candidates = packagedExecutableCandidates(repositoryRoot, platform, arch)
  const executable = candidates.find((candidate) => pathExists(candidate))
  if (!executable) {
    throw new Error(`Packaged executable not found. Checked:\n${candidates.join('\n')}`)
  }
  return executable
}
