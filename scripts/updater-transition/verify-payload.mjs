import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFile, readdir, writeFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

// tauri-utils 2.9.3 platform.rs: the bundler patches UNK to NSS for NSIS.
// Require the unique marker and preserve every other byte.
export function nsisHost(portable) {
  const marker = Buffer.from('__TAURI_BUNDLE_TYPE_VAR_UNK')
  const offset = portable.indexOf(marker)
  assert(offset >= 0, 'Portable bundle marker missing')
  assert.equal(portable.indexOf(marker, offset + 1), -1, 'Duplicate Portable bundle marker')
  const expected = Buffer.from(portable)
  expected.write('NSS', offset + marker.length - 3, 'ascii')
  return expected
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const root = resolve(process.argv[2] ?? '.workbench/updater-transition')
  const payload = join(root, 'new/installer-payload')
  const entries = await readdir(payload, { recursive: true, withFileTypes: true })
  const report = { passed: false, files: [] }
  try {
    for (const name of ['cruciblebox.exe', 'cruciblebox-plugin-host.exe']) {
      const matches = entries.filter((entry) => entry.isFile() && entry.name === name)
      assert.equal(matches.length, 1, 'Expected one installer payload: ' + name)
      const actual = await readFile(join(matches[0].parentPath, name))
      const portable = await readFile(join(root, 'new/portable/CrucibleBox', name))
      const expected = name === 'cruciblebox.exe' ? nsisHost(portable) : portable
      const hash = (bytes) => createHash('sha256').update(bytes).digest('hex')
      report.files.push({
        name,
        installerPayloadSha256: hash(actual),
        portableSha256: hash(portable),
        expectedInstallerSha256: hash(expected),
        transformation:
          name === 'cruciblebox.exe' ? 'Tauri bundle marker UNK -> NSS (3 bytes)' : 'none',
        equal: actual.equals(expected)
      })
      assert(actual.equals(expected), 'Installer payload differs beyond the Tauri marker: ' + name)
    }
    report.passed = true
  } finally {
    await writeFile(join(root, 'installer-payload.json'), JSON.stringify(report, null, 2) + '\n')
  }
}
