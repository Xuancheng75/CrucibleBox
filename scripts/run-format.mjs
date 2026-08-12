import { spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { readFileSync } from 'node:fs'

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const repositoryRoot = resolve(scriptDirectory, '..')
const require = createRequire(import.meta.url)
const prettierDirectory = dirname(require.resolve('prettier/package.json'))
const prettierCli = resolve(prettierDirectory, 'bin', 'prettier.cjs')
const targets = JSON.parse(readFileSync(resolve(scriptDirectory, 'format-targets.json'), 'utf8'))
const mode = process.argv[2] === '--check' ? '--check' : '--write'
const result = spawnSync(process.execPath, [prettierCli, mode, ...targets], {
  cwd: repositoryRoot,
  stdio: 'inherit',
  windowsHide: true
})

if (result.error) throw result.error
process.exitCode = result.status ?? 1
