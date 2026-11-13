import assert from 'node:assert/strict'
import { readFileSync, readdirSync, existsSync } from 'node:fs'
import { resolve, relative, isAbsolute, extname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import ts from 'typescript'

const repository = fileURLToPath(new URL('../', import.meta.url))
const inside = (root, file) => {
  const path = relative(root, file)
  return (
    path === '' ||
    (!isAbsolute(path) && path !== '..' && !path.startsWith('..\\') && !path.startsWith('../'))
  )
}
const skipped = new Set(['node_modules', 'target', 'dist', '.git', '.workbench', '.release'])
function files(directory) {
  if (!existsSync(directory)) return []
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    if (entry.isSymbolicLink() || skipped.has(entry.name)) return []
    const path = join(directory, entry.name)
    return entry.isDirectory() ? files(path) : [path]
  })
}
function options(directory) {
  const config = ts.findConfigFile(directory, ts.sys.fileExists)
  if (!config || !inside(directory, config)) return { allowJs: true }
  const value = ts.readConfigFile(config, ts.sys.readFile)
  assert(!value.error, `Cannot read ${config}`)
  return ts.parseJsonConfigFileContent(value.config, ts.sys, directory).options
}
export function checkImports(directory, { plugin = false, root = repository } = {}) {
  const compiler = options(directory)
  let count = 0
  for (const file of files(directory).filter((file) => /\.[cm]?[jt]sx?$/.test(extname(file)))) {
    const source = readFileSync(file, 'utf8')
    for (const entry of ts.preProcessFile(source, true, true).importedFiles) {
      const specifier = entry.fileName
      const resolved = ts.resolveModuleName(specifier, file, compiler, ts.sys).resolvedModule
        ?.resolvedFileName
      const target =
        resolved ??
        (specifier.startsWith('file:')
          ? fileURLToPath(specifier)
          : specifier.startsWith('.')
            ? resolve(file, '..', specifier)
            : isAbsolute(specifier)
              ? specifier
              : null)
      const location = `${relative(root, file)} imports ${specifier}`
      assert(
        !/CrucibleBox_Plugins|插件源代码/i.test(specifier + (target ?? '')) || plugin,
        `${location}: core reaches business source`
      )
      if (target && plugin) {
        assert(
          !inside(root, target) || inside(join(root, 'packages'), target),
          `${location}: plugin reaches host private source`
        )
      }
      if (target && inside(join(root, 'packages'), file)) {
        assert(
          !inside(root, target) ||
            inside(join(root, 'packages'), target) ||
            inside(join(root, 'node_modules'), target),
          `${location}: public SDK reaches host private source`
        )
      }
      count++
    }
  }
  return count
}

export function checkSdkCopies(plugin, root = repository) {
  let checked = 0
  for (const [name, entries] of [
    ['api', ['package.json', 'README.md', 'LICENSE', 'src']],
    ['ui', ['package.json', 'README.md', 'LICENSE', 'src']],
    ['build', ['package.json', 'README.md', 'LICENSE', 'cli.mjs']]
  ]) {
    const copy = join(plugin, 'vendor', `next2-${name}`)
    if (!existsSync(copy)) continue
    const original = join(root, 'packages', `cruciblebox-next2-${name}`)
    assert.equal(
      JSON.parse(readFileSync(join(copy, 'package.json'))).version,
      JSON.parse(readFileSync(join(original, 'package.json'))).version,
      `${copy}: SDK version differs`
    )
    for (const entry of entries) {
      const paths =
        extname(entry) || entry === 'LICENSE'
          ? [join(original, entry)]
          : files(join(original, entry))
      for (const path of paths) {
        const destination = join(copy, relative(original, path))
        assert(
          readFileSync(path).equals(readFileSync(destination)),
          `${destination}: public SDK copy drift`
        )
        checked++
      }
    }
  }
  for (const manifest of files(join(plugin, 'native/vendor')).filter((file) =>
    file.endsWith('Cargo.toml')
  )) {
    const text = readFileSync(manifest, 'utf8')
    const name = text.match(/^name\s*=\s*"([^"]+)"/m)?.[1]
    if (!name?.startsWith('cruciblebox-')) continue
    assert.equal(
      name,
      'cruciblebox-next2-native',
      `${manifest}: host task infrastructure copied into plugin`
    )
    const copy = resolve(manifest, '..'),
      original = join(root, 'packages/cruciblebox-next2-native')
    for (const entry of ['Cargo.toml', 'Cargo.lock', 'LICENSE', 'src']) {
      for (const path of extname(entry) || entry === 'LICENSE'
        ? [join(original, entry)]
        : files(join(original, entry))) {
        const destination = join(copy, relative(original, path))
        assert(
          readFileSync(path).equals(readFileSync(destination)),
          `${destination}: native SDK copy drift`
        )
        checked++
      }
    }
  }
  return checked
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  let imports = 0,
    copies = 0
  for (const directory of ['packages', 'shared', 'tauri-frontend/src'])
    imports += checkImports(join(repository, directory))
  const args = process.argv.slice(2)
  while (args.length) {
    assert.equal(args.shift(), '--plugin-root', 'Expected --plugin-root <independent project>')
    const input = args.shift()
    assert(input, 'Missing independent plugin project')
    const plugin = resolve(input)
    imports += checkImports(plugin, { plugin: true })
    copies += checkSdkCopies(plugin)
  }
  console.log(`Source boundaries checked: ${imports} imports, ${copies} public SDK files`)
}
