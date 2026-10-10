import { mkdtemp, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { exportSource, independentNpmEnvironment } from './export-next-plugin-source.mjs'
const npm = process.env.npm_execpath
if (!npm) throw Error('Run through npm so the exact npm CLI is available')
const output = await mkdtemp(join(tmpdir(), 'cruciblebox-next-independent-'))
const catalog = JSON.parse(await readFile(new URL('./next-plugin-catalog.json', import.meta.url)))
const results = []
for (const { id, runtimeFiles } of catalog) {
  const destination = join(output, id)
  await exportSource(id, destination)
  const steps = []
  for (const [label, args] of [
    ['install', [npm, 'ci', '--ignore-scripts', '--no-audit', '--no-fund']],
    ['typecheck', [npm, 'run', 'typecheck']],
    ['test', [npm, 'test']],
    ['build', [npm, 'run', 'build']],
    ['pack-a', ['vendor/plugin-build/cli.mjs', 'pack', '.', join(destination, id + '-a.zip')]],
    ['pack-b', ['vendor/plugin-build/cli.mjs', 'pack', '.', join(destination, id + '-b.zip')]]
  ]) {
    const run = spawnSync(process.execPath, args, {
      cwd: destination,
      env: independentNpmEnvironment(),
      encoding: 'utf8',
      windowsHide: true,
      timeout: 180000,
      maxBuffer: 8 * 1024 * 1024
    })
    await writeFile(join(destination, label + '.log'), (run.stdout || '') + (run.stderr || ''))
    steps.push({ label, exitCode: run.status })
    if (run.error || run.status !== 0)
      throw Error(id + ' ' + label + ' failed; logs: ' + destination)
  }
  const first = await readFile(join(destination, id + '-a.zip'))
  if (!first.equals(await readFile(join(destination, id + '-b.zip'))))
    throw Error(id + ' pack differs')
  for (const file of runtimeFiles) {
    if (
      !(await readFile(join(destination, file))).equals(
        await readFile(new URL('../plugins/' + id + '/' + file, import.meta.url))
      )
    )
      throw Error(id + ' independently built runtime differs: ' + file)
  }
  results.push({ id, steps, repeatPackIdentical: true, canonicalRuntimeIdentical: true })
}
await writeFile(join(output, 'results.json'), JSON.stringify(results, null, 2) + '\n')
if (process.env.GITHUB_ENV)
  await writeFile(process.env.GITHUB_ENV, 'CRUCIBLEBOX_NEXT_OFFICIAL_PACKAGES=' + output + '\n', {
    flag: 'a'
  })
console.log('Seven independent plugin sources verified: ' + output)
