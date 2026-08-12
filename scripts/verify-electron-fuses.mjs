import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { verifyElectronFuses } from './electron-fuses.mjs'
import { resolvePackagedExecutable } from './packaged-paths.mjs'

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const repositoryRoot = resolve(scriptDirectory, '..')
const executable = resolvePackagedExecutable(repositoryRoot)

await verifyElectronFuses(executable)
console.log(`Electron fuses verified: ${executable}`)
