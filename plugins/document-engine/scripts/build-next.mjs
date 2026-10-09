import { spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'
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
const require = createRequire(join(project, 'package.json'))
for (const args of [
  [join(project, 'vendor/plugin-ui/scripts/build.mjs')],
  [
    require.resolve('typescript/bin/tsc'),
    '--project',
    join(project, 'vendor/plugin-ui/tsconfig.json')
  ]
]) {
  const built = spawnSync(process.execPath, args, { stdio: 'inherit', windowsHide: true })
  if (built.error) throw built.error
  if (built.status !== 0) process.exit(built.status ?? 1)
}
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
