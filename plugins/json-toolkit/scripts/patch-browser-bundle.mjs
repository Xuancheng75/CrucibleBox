import { readFile, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'

const outputPath = resolve('dist/renderer.js')
const output = await readFile(outputPath, 'utf8')
// @iarna/toml probes Node's util.inspect through eval. It is cosmetic and is
// not valid in the browser plugin runtime, so remove only that exact probe.
await writeFile(outputPath, output.replace(/eval\("require\('util'\)\.inspect"\)/g, 'undefined'))
