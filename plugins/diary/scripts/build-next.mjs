import { spawnSync } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'
const project = dirname(dirname(fileURLToPath(import.meta.url)))
const workspace = dirname(dirname(project))
const metadata = join(workspace, 'package.json')
const manifest = JSON.parse(readFileSync(join(project, 'plugin.json'), 'utf8'))
const useWorkspace =
  existsSync(metadata) &&
  JSON.parse(readFileSync(metadata, 'utf8')).workspaces?.includes('plugins/*') &&
  relative(workspace, project).replaceAll('\\', '/') === 'plugins/' + manifest.id
const result = spawnSync(
  process.execPath,
  [
    join(project, 'vendor/plugin-build/cli.mjs'),
    'build',
    project,
    ...(useWorkspace ? ['--workspace-root=' + workspace] : [])
  ],
  { stdio: 'inherit', windowsHide: true }
)
if (result.error) throw result.error
process.exit(result.status ?? 1)
