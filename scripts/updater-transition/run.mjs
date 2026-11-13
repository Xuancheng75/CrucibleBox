// Runs installers only after explicit approval, on a disposable GitHub Windows VM.
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { createHash } from 'node:crypto'
import { createServer } from 'node:http'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { DatabaseSync } from 'node:sqlite'

assert.equal(process.platform, 'win32')
assert.equal(process.env.GITHUB_ACTIONS, 'true')
assert.equal(process.env.RUNNER_ENVIRONMENT, 'github-hosted')
assert.equal(process.env.CRUCIBLEBOX_INSTALLER_TEST_APPROVED, 'true')
const root = resolve('.workbench/updater-transition')
const installed = join(root, 'installed'),
  appdata = join(root, 'appdata')
const executable = join(installed, 'cruciblebox.exe')
const delay = (ms) => new Promise((done) => setTimeout(done, ms))
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex')
await mkdir(root, { recursive: true })
await mkdir(appdata, { recursive: true })
const configuration = JSON.parse(await readFile('src-tauri/tauri.conf.json', 'utf8'))
const retainedArtifact = process.env.CRUCIBLEBOX_UPDATER_TEST_PLUGIN_ARTIFACT
assert.match(retainedArtifact, /^[a-z0-9][a-z0-9._-]*\.zip$/)
const report = { passed: false, from: '2.1.1', to: configuration.version, checks: [] }
const environment = {
  ...process.env,
  APPDATA: appdata,
  LOCALAPPDATA: join(root, 'local'),
  WEBVIEW2_USER_DATA_FOLDER: join(root, 'webview'),
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=19430 --no-first-run',
  CRUCIBLEBOX_UPDATER_TEST_ROOT: root
}
function run(file, args, env = environment) {
  return new Promise((done, reject) => {
    const child = spawn(file, args, { env, windowsHide: true, stdio: 'inherit' })
    const timer = setTimeout(() => reject(Error('Timed out: ' + file)), 180000)
    child.once('error', (error) => {
      clearTimeout(timer)
      reject(error)
    })
    child.once('exit', (code) => {
      clearTimeout(timer)
      code === 0 ? done() : reject(Error(file + ' exited ' + code))
    })
  })
}
async function connect() {
  for (const started = Date.now(); Date.now() - started < 60000;) {
    let targets
    try {
      targets = await (await fetch('http://127.0.0.1:19430/json/list')).json()
    } catch {
      await delay(250)
      continue
    }
    const target = targets.find(
      (item) => item.type === 'page' && item.url === 'http://tauri.localhost/'
    )
    if (!target) {
      await delay(250)
      continue
    }
    const socket = new WebSocket(target.webSocketDebuggerUrl)
    await new Promise((done, reject) => {
      socket.addEventListener('open', done, { once: true })
      socket.addEventListener('error', reject, { once: true })
    })
    let sequence = 0
    const pending = new Map()
    socket.addEventListener('message', (event) => {
      const data = JSON.parse(event.data),
        request = pending.get(data.id)
      if (!request) return
      pending.delete(data.id)
      clearTimeout(request.timer)
      if (data.error || data.result?.exceptionDetails) request.reject(Error(JSON.stringify(data)))
      else request.done(data.result.result.value)
    })
    const evaluate = (expression) =>
      new Promise((done, reject) => {
        const id = ++sequence,
          timer = setTimeout(() => {
            pending.delete(id)
            reject(Error('IPC timeout'))
          }, 30000)
        pending.set(id, { done, reject, timer })
        socket.send(
          JSON.stringify({
            id,
            method: 'Runtime.evaluate',
            params: { expression, awaitPromise: true, returnByValue: true }
          })
        )
      })
    return {
      evaluate,
      close: () => socket.close(),
      invoke: (name, args = {}) =>
        evaluate(
          'window.__TAURI_INTERNALS__.invoke(' +
            JSON.stringify(name) +
            ',' +
            JSON.stringify(args) +
            ',{headers:{Origin:window.location.origin}})'
        )
    }
  }
  throw Error('Installed application did not expose its own test IPC')
}
// Only the PIDs started by this script on the disposable runner are terminated.
async function stop(child) {
  if (child.exitCode === null) await run('taskkill.exe', ['/PID', String(child.pid), '/T', '/F'])
  await delay(1000)
}
const dbPath = join(appdata, 'cruciblebox/data/openbox.db')
function savedData() {
  const db = new DatabaseSync(dbPath, { readOnly: true })
  try {
    return {
      plugins: db
        .prepare('SELECT id,name,version,config_data,installed_path FROM plugins ORDER BY id')
        .all(),
      storage: db
        .prepare('SELECT plugin_id,key,value FROM plugin_storage ORDER BY plugin_id,key')
        .all(),
      migrations: db
        .prepare(
          'SELECT plugin_id,migration FROM plugin_storage_migrations ORDER BY plugin_id,migration'
        )
        .all()
    }
  } finally {
    db.close()
  }
}
let oldApp, server
try {
  const oldInstaller = join(root, 'old/CrucibleBox_2.1.1_x64-setup.exe')
  const sums = await readFile(join(root, 'old/SHA256SUMS.txt'), 'utf8')
  for (const name of ['CrucibleBox_2.1.1_x64-setup.exe', retainedArtifact]) {
    const line = sums.split(/\r?\n/).find((line) => line.endsWith('  ' + name))
    assert(line, 'Old release checksum missing: ' + name)
    assert.equal(hash(await readFile(join(root, 'old', name))), line.slice(0, 64).toLowerCase())
  }
  await run(oldInstaller, ['/S', '/D=' + installed])
  oldApp = spawn(executable, [], {
    env: environment,
    cwd: installed,
    windowsHide: true,
    stdio: 'inherit'
  })
  let page = await connect()
  assert.equal(await page.invoke('app_get_version'), '2.1.1')
  assert.equal(await page.invoke('settings_set', { key: 'updateChannel', value: 'stable' }), true)
  const pluginPath = join(root, 'old', retainedArtifact)
  await page.invoke('plugin_register_import_path', { path: pluginPath })
  const preview = await page.invoke('plugin_install_preview', {
    source: { type: 'zip', path: pluginPath }
  })
  assert.equal(preview.success, true)
  const imported = await page.invoke('plugin_install_commit', {
    token: preview.installToken,
    approval: {
      manifestSha256: preview.data.next2.manifestSha256,
      acceptUnsignedLocal: true,
      acceptPermissions: true,
      acceptNativeCode: false
    }
  })
  assert.equal(imported.success, true)
  const oldPlugins = await page.invoke('plugin_list')
  assert.equal(oldPlugins.length, 1)
  const oldPlugin = oldPlugins[0]
  assert.equal((await page.invoke('plugin_enable', { id: oldPlugin.name })).success, true)
  const descriptor = await page.invoke('create_next_renderer_session', { id: oldPlugin.name })
  await page.evaluate(
    'new Promise((done,reject)=>{const frame=document.createElement("iframe");frame.hidden=true;frame.sandbox="allow-scripts";frame.onload=()=>done(true);frame.onerror=reject;frame.src=' +
      JSON.stringify(descriptor.indexUrl) +
      ';document.body.append(frame)})'
  )
  const response = await page.invoke('next_renderer_request', {
    raw: JSON.stringify({
      wireVersion: 4,
      requestId: 'updater-seed',
      session: descriptor.token,
      target: { kind: 'platform' },
      method: 'storage.set',
      params: { key: 'updater-retained', value: { text: '旧版真实 KV 保留🌏' } }
    })
  })
  assert.equal(response.ok, true)
  page.close()
  await stop(oldApp)
  oldApp = undefined
  const db = new DatabaseSync(dbPath)
  const plugin = db.prepare('SELECT id FROM plugins WHERE name=?').get(oldPlugin.name)
  db.prepare('INSERT INTO plugin_storage_migrations(plugin_id,migration) VALUES(?,?)').run(
    plugin.id,
    'updater-retained-fixture'
  )
  db.close()
  const userFile = join(appdata, 'cruciblebox/plugin-data', oldPlugin.name, 'updater-user.txt')
  await mkdir(resolve(userFile, '..'), { recursive: true })
  const userBytes = Buffer.from('用户文件保留；签名更新安装切换验收。🌏')
  await writeFile(userFile, userBytes)
  const before = savedData()
  report.checks.push(
    'official-2.1.1-installed-and-started',
    'old-plugin-import-and-real-public-storage-write'
  )
  const setupName = `CrucibleBox_${configuration.version}_x64-setup.exe`
  const setup = await readFile(join(root, 'new', setupName))
  const signature = (await readFile(join(root, 'new', setupName + '.sig'), 'utf8')).trim()
  const damaged = Buffer.from(setup)
  damaged[damaged.length - 1] ^= 1
  server = createServer((request, response) => {
    if (request.url === '/setup.exe' || request.url === '/tampered.exe') {
      const bytes = request.url === '/setup.exe' ? setup : damaged
      response.writeHead(200, {
        'Content-Type': 'application/octet-stream',
        'Content-Length': bytes.length
      })
      response.end(bytes)
      return
    }
    if (request.url !== '/install.json' && request.url !== '/reject.json') {
      response.writeHead(404)
      response.end()
      return
    }
    const port = server.address().port,
      artifact = request.url === '/reject.json' ? 'tampered.exe' : 'setup.exe'
    response.setHeader('Content-Type', 'application/json')
    response.end(
      JSON.stringify({
        version: configuration.version,
        platforms: { 'windows-x86_64': { signature, url: `http://127.0.0.1:${port}/${artifact}` } }
      })
    )
  })
  await new Promise((done) => server.listen(0, '127.0.0.1', done))
  const harness = resolve(
    '.workbench/updater-transition-target/debug/cruciblebox-updater-transition.exe'
  )
  for (const mode of ['reject', 'install'])
    await run(harness, [], {
      ...environment,
      CRUCIBLEBOX_UPDATER_TEST_MODE: mode,
      CRUCIBLEBOX_UPDATER_TEST_ENDPOINT: `http://127.0.0.1:${server.address().port}/${mode}.json`
    })
  assert.equal(
    JSON.parse(await readFile(join(root, 'tampered-rejected.json'), 'utf8')).passed,
    true
  )
  assert.equal(
    JSON.parse(await readFile(join(root, 'signature-verified.json'), 'utf8')).passed,
    true
  )
  const expected = await readFile(join(root, 'new/portable/CrucibleBox/cruciblebox.exe'))
  for (const started = Date.now(); ;) {
    try {
      if (hash(await readFile(executable)) === hash(expected)) break
    } catch (error) {
      if (error.code !== 'ENOENT' && error.code !== 'EBUSY') throw error
    }
    if (Date.now() - started > 120000) throw Error('Installer did not switch the executable')
    await delay(500)
  }
  // /R starts the updated app. Connect to that actual installed application.
  page = await connect()
  assert.equal(await page.invoke('app_get_version'), configuration.version)
  assert.equal(await page.invoke('settings_get', { key: 'updateChannel' }), 'stable')
  const plugins = await page.invoke('plugin_list'),
    retained = plugins.find((item) => item.name === oldPlugin.name)
  assert.equal(retained.version, oldPlugin.version)
  assert.equal(retained.enabled, false)
  page.close()
  assert.deepEqual(savedData(), before)
  assert.deepEqual(await readFile(userFile), userBytes)
  for (const name of ['cruciblebox.exe', 'cruciblebox-plugin-host.exe'])
    assert.equal(
      hash(await readFile(join(installed, name))),
      hash(await readFile(join(root, 'new/portable/CrucibleBox', name)))
    )
  report.checks.push(
    'tampered-update-rejected-by-minisign',
    'valid-update-download-signature-and-NSIS-install',
    'installed-executables-match-release-portable',
    'updated-2.2.0-startup',
    'settings-records-config-KV-migration-marks-user-files-retained',
    'old-API6-disabled-with-data-retained'
  )
  report.installerSha256 = hash(setup)
  report.executableSha256 = hash(expected)
  report.passed = true
} finally {
  if (oldApp) await stop(oldApp)
  if (server) await new Promise((done) => server.close(done))
  await writeFile(join(root, 'transition.json'), JSON.stringify(report, null, 2) + '\n')
}
