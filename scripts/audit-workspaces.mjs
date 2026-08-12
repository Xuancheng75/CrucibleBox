import { spawnSync } from 'node:child_process'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const repositoryRoot = resolve(scriptDirectory, '..')
const npmCli = process.env.npm_execpath
if (!npmCli) throw new Error('audit:dependencies must be run through npm')

const result = spawnSync(
  process.execPath,
  [npmCli, 'audit', '--workspaces', '--include-workspace-root', '--audit-level', 'high'],
  {
    cwd: repositoryRoot,
    encoding: 'utf8',
    windowsHide: true
  }
)

if (result.error) {
  throw result.error
}

if (result.stdout) process.stdout.write(result.stdout)
if (result.stderr) process.stderr.write(result.stderr)
process.exitCode = result.status ?? 1
