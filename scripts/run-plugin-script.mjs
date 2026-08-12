import { spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const repositoryRoot = resolve(scriptDirectory, '..')
const catalog = JSON.parse(readFileSync(resolve(scriptDirectory, 'plugin-catalog.json'), 'utf8'))
const task = process.argv[2]
const allowedTasks = new Set(['clean', 'build', 'test', 'typecheck'])

if (!allowedTasks.has(task)) {
  throw new Error(`Unsupported plugin task: ${task ?? '<missing>'}`)
}

const npmCli = process.env.npm_execpath
if (!npmCli) {
  throw new Error('Run this command through an npm script so npm_execpath is available')
}

for (const plugin of catalog) {
  const pluginDirectory = resolve(repositoryRoot, 'plugins', plugin.id)
  const npmArguments = ['run', '--if-present', task]

  console.log(`[plugins] ${plugin.id}: npm ${npmArguments.join(' ')}`)
  const result = spawnSync(process.execPath, [npmCli, ...npmArguments], {
    cwd: pluginDirectory,
    env: { ...process.env, npm_config_update_notifier: 'false' },
    stdio: 'inherit'
  })

  if (result.error) {
    throw result.error
  }

  if (result.status !== 0) {
    process.exit(result.status ?? 1)
  }
}
