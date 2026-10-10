import { createHash } from 'node:crypto'
import { createReadStream, existsSync } from 'node:fs'
import {
  mkdir,
  readdir,
  lstat,
  realpath,
  copyFile,
  readFile,
  writeFile,
  open,
  rename,
  unlink
} from 'node:fs/promises'
import { join, resolve, relative, isAbsolute, dirname, basename } from 'node:path'
import { backup, DatabaseSync } from 'node:sqlite'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
const inside = (parent, child) => {
  const rel = relative(parent, child)
  return (
    rel === '' ||
    (!isAbsolute(rel) && rel !== '..' && !rel.startsWith('..\\') && !rel.startsWith('../'))
  )
}
async function canonicalOutput(path) {
  let existing = resolve(path)
  const suffix = []
  while (!existsSync(existing)) {
    suffix.unshift(basename(existing))
    const parent = dirname(existing)
    if (parent === existing) return existing
    existing = parent
  }
  return join(await realpath(existing), ...suffix)
}
async function hash(file) {
  const digest = createHash('sha256')
  for await (const bytes of createReadStream(file)) digest.update(bytes)
  return digest.digest('hex')
}
async function plain(file, directory) {
  const info = await lstat(file)
  if (info.isSymbolicLink() || (directory ? !info.isDirectory() : !info.isFile()))
    throw Error('Linked or non-regular rollback input: ' + file)
  if ((await realpath(file)) !== resolve(file)) throw Error('Redirected rollback input: ' + file)
  return info
}
async function tree(root) {
  await plain(root, true)
  const files = []
  const directories = []
  const visit = async (dir, depth) => {
    if (depth > 64) throw Error('Rollback depth exceeded')
    for (const entry of (await readdir(dir, { withFileTypes: true })).sort((a, b) =>
      a.name.localeCompare(b.name)
    )) {
      const file = join(dir, entry.name)
      await plain(file, entry.isDirectory())
      const name = relative(root, file).replaceAll('\\', '/')
      if (entry.isDirectory()) {
        directories.push(name)
        await visit(file, depth + 1)
      } else files.push(name)
      if (files.length + directories.length > 500000) throw Error('Rollback entry limit exceeded')
    }
  }
  await visit(root, 0)
  return { files, directories }
}
async function stopped(program) {
  if (process.platform !== 'win32') return
  const script =
    '$root=[Console]::In.ReadToEnd() | ConvertFrom-Json; $prefix=$root.TrimEnd([char]92)+[char]92; $found=Get-CimInstance Win32_Process -ErrorAction Stop | Where-Object { $_.ExecutablePath -and ($_.ExecutablePath.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)) }; if($found){[Console]::Error.WriteLine("Program or worker is running");exit 2}'
  const result = spawnSync(
    'powershell.exe',
    ['-NoProfile', '-NonInteractive', '-Command', script],
    { input: JSON.stringify(program), encoding: 'utf8', windowsHide: true, timeout: 20000 }
  )
  if (result.error || result.status !== 0)
    throw Error(
      'Close the paired program and its workers before capture: ' +
        (result.error?.message || result.stderr)
    )
}
async function sqlite(file) {
  const handle = await open(file, 'r')
  try {
    const header = Buffer.alloc(16)
    await handle.read(header, 0, 16, 0)
    return header.toString('binary') === 'SQLite format 3\0'
  } finally {
    await handle.close()
  }
}
function dbInfo(file) {
  const db = new DatabaseSync(file, { readOnly: true })
  try {
    if (db.prepare('PRAGMA integrity_check').get().integrity_check !== 'ok')
      throw Error('Invalid rollback database')
    return { schema: db.prepare('PRAGMA user_version').get().user_version }
  } finally {
    db.close()
  }
}
async function copyTree(source, target, coherent) {
  const inventory = await tree(source)
  await mkdir(target, { recursive: true })
  for (const name of inventory.directories) await mkdir(join(target, name), { recursive: true })
  const databases = new Set()
  if (coherent)
    for (const name of inventory.files) if (await sqlite(join(source, name))) databases.add(name)
  const records = []
  for (const name of inventory.files) {
    if (
      coherent &&
      ['-wal', '-shm', '-journal'].some(
        (suffix) => name.endsWith(suffix) && databases.has(name.slice(0, -suffix.length))
      )
    )
      continue
    const from = join(source, name),
      to = join(target, name)
    const before = await plain(from, false)
    await mkdir(dirname(to), { recursive: true })
    let database
    if (databases.has(name)) {
      const db = new DatabaseSync(from, { readOnly: true })
      try {
        await backup(db, to)
      } finally {
        db.close()
      }
      // A standalone backup must not create a new WAL/SHM merely when verified.
      const portable = new DatabaseSync(to)
      try {
        portable.exec('PRAGMA journal_mode=DELETE')
      } finally {
        portable.close()
      }
      database = dbInfo(to)
    } else {
      const prior = await hash(from)
      await copyFile(from, to)
      if (
        prior !== (await hash(to)) ||
        prior !== (await hash(from)) ||
        (await lstat(from)).mtimeMs !== before.mtimeMs
      )
        throw Error('Rollback input changed: ' + name)
    }
    const handle = await open(to, 'r+')
    try {
      await handle.sync()
    } finally {
      await handle.close()
    }
    records.push({
      path: name,
      bytes: (await lstat(to)).size,
      sha256: await hash(to),
      ...(database ? { database } : {})
    })
  }
  const after = await tree(source)
  // Read-only SQLite backup can create WAL/SHM coordination files for a closed WAL database.
  // They are neither independent user files nor part of the coherent restored database.
  const stableTree = (value) => ({
    directories: value.directories,
    files: value.files.filter(
      (name) =>
        !(
          coherent &&
          ['-wal', '-shm', '-journal'].some(
            (suffix) => name.endsWith(suffix) && databases.has(name.slice(0, -suffix.length))
          )
        )
    )
  })
  if (JSON.stringify(stableTree(after)) !== JSON.stringify(stableTree(inventory)))
    throw Error('Rollback source tree changed')
  return { files: records, directories: inventory.directories }
}
function paths(manifest) {
  if (!manifest || manifest.version !== 1 || !manifest.program || !manifest.data)
    throw Error('Invalid paired rollback manifest')
  for (const section of ['program', 'data']) {
    if (!Array.isArray(manifest[section].files) || !Array.isArray(manifest[section].directories))
      throw Error('Invalid rollback inventory')
    const seen = new Set()
    for (const name of [
      ...manifest[section].files.map((item) => item.path),
      ...manifest[section].directories
    ]) {
      if (
        typeof name !== 'string' ||
        isAbsolute(name) ||
        name.includes('\\') ||
        name.includes(':') ||
        name.split('/').some((part) => !part || part === '.' || part === '..') ||
        seen.has(name.toLowerCase())
      )
        throw Error('Unsafe rollback inventory')
      seen.add(name.toLowerCase())
    }
  }
}
export async function verifyPair(snapshot) {
  snapshot = await realpath(snapshot)
  await plain(join(snapshot, 'pair.json'), false)
  const manifest = JSON.parse(await readFile(join(snapshot, 'pair.json'), 'utf8'))
  paths(manifest)
  for (const section of ['program', 'data']) {
    const root = join(snapshot, section),
      actual = await tree(root)
    const expected = manifest[section]
    if (
      JSON.stringify([...actual.files].sort()) !==
        JSON.stringify(expected.files.map((item) => item.path).sort()) ||
      JSON.stringify([...actual.directories].sort()) !==
        JSON.stringify([...expected.directories].sort())
    )
      throw Error('Rollback inventory differs')
    for (const item of expected.files) {
      const file = join(root, item.path)
      if ((await plain(file, false)).size !== item.bytes || (await hash(file)) !== item.sha256)
        throw Error('Rollback hash mismatch: ' + item.path)
      if (item.database && dbInfo(file).schema !== item.database.schema)
        throw Error('Rollback database schema mismatch')
    }
  }
  return manifest
}
export async function capturePair(program, data, output) {
  program = await realpath(program)
  data = await realpath(data)
  output = await canonicalOutput(output)
  if (
    inside(program, output) ||
    inside(data, output) ||
    inside(program, data) ||
    inside(data, program) ||
    existsSync(output)
  )
    throw Error('Pair capture requires disjoint inputs and a new external output')
  await stopped(program)
  if (!existsSync(join(program, 'cruciblebox.exe')))
    throw Error('Paired CrucibleBox executable missing')
  await mkdir(output)
  await writeFile(join(output, 'INCOMPLETE'), 'Do not restore until pair.json is finalized', {
    flag: 'wx'
  })
  const manifest = {
    version: 1,
    capturedAt: new Date().toISOString(),
    source: { program, data },
    program: await copyTree(program, join(output, 'program'), false),
    data: await copyTree(data, join(output, 'data'), true)
  }
  await stopped(program)
  await writeFile(join(output, 'pair.pending.json'), JSON.stringify(manifest, null, 2) + '\n', {
    flag: 'wx'
  })
  const manifestFile = await open(join(output, 'pair.pending.json'), 'r+')
  try {
    await manifestFile.sync()
  } finally {
    await manifestFile.close()
  }
  await rename(join(output, 'pair.pending.json'), join(output, 'pair.json'))
  await verifyPair(output)
  await unlink(join(output, 'INCOMPLETE'))
  return manifest
}
export async function restorePair(snapshot, output) {
  snapshot = await realpath(snapshot)
  output = await canonicalOutput(output)
  if (inside(snapshot, output) || inside(output, snapshot) || existsSync(output))
    throw Error('Restore requires a new external directory')
  const manifest = await verifyPair(snapshot)
  await mkdir(output)
  for (const section of ['program', 'data'])
    await copyTree(join(snapshot, section), join(output, section), false)
  await writeFile(join(output, 'pair.json'), JSON.stringify(manifest, null, 2) + '\n', {
    flag: 'wx'
  })
  await verifyPair(output)
  return { program: join(output, 'program'), data: join(output, 'data') }
}
function summary(manifest) {
  return {
    version: manifest.version,
    capturedAt: manifest.capturedAt,
    programFiles: manifest.program.files.length,
    dataFiles: manifest.data.files.length,
    inventory: 'pair.json',
    verified: true
  }
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, ...args] = process.argv.slice(2)
  if (command === 'capture' && args.length === 3)
    console.log(JSON.stringify(summary(await capturePair(...args))))
  else if (command === 'restore' && args.length === 2)
    console.log(JSON.stringify(await restorePair(...args)))
  else if (command === 'verify' && args.length === 1)
    console.log(JSON.stringify(summary(await verifyPair(...args))))
  else
    throw Error(
      'Usage: next-paired-rollback capture <stopped-program-dir> <user-data-dir> <new-pair-dir> | verify <pair-dir> | restore <pair-dir> <new-dir>'
    )
}
