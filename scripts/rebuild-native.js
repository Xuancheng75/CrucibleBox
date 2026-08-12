const { spawnSync } = require('child_process')
const { resolve } = require('path')

console.log('[rebuild-native] Running electron-rebuild for better-sqlite3...')

const projectRoot = resolve(__dirname, '..')
const cli = resolve(projectRoot, 'node_modules/@electron/rebuild/lib/cli.js')
const result = spawnSync(process.execPath, [cli, '-f', '-w', 'better-sqlite3'], {
  cwd: projectRoot,
  stdio: 'inherit'
})

if (result.error) {
  throw result.error
}

if (result.status !== 0) {
  process.exit(result.status ?? 1)
}

console.log('[rebuild-native] Done')
