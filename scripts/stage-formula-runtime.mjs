import { cpSync, copyFileSync, existsSync, mkdirSync, readFileSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { join, resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')
const target = join(root, 'src-tauri', 'resources', 'formula-ocr')
const python = process.env.CRUCIBLEBOX_FORMULA_PORTABLE_PYTHON
const models = process.env.CRUCIBLEBOX_FORMULA_MODEL_DIRECTORY

if (!python || !models) {
  console.log(
    '[formula-runtime] No portable Python/model source configured; keeping existing staged resources.'
  )
  process.exit(0)
}

const sources = [
  ['PP-DocLayout-M.onnx', '34ecdc84e60d5f5822fab85e3e97f30ca73fdc6c7c0aa10b3bd7227742a574b5'],
  ['pp_formulanet_plus_s.onnx', '30998d10c94ccff1ad8981df0c71048cb1f3eec7b1e515b809767f1f72aebe3b']
]
const textSources = [
  ['ppocrv6_small_det.onnx', '090f04abcd9d9a7498bc4ebf677e4cb9bdce1fe4197ddb7e529f1ef44e1ff94f'],
  ['PP-OCRv5_mobile_rec.onnx', '5825fc7ebf84ae7a412be049820b4d86d77620f204a041697b0494669b1742c5'],
  ['ppocrv5_dict.txt', 'd1979e9f794c464c0d2e0b70a7fe14dd978e9dc644c0e71f14158cdf8342af1b'],
  [
    'en_PP-OCRv5_mobile_rec.onnx',
    'b5f833dfc5d0eb71da397b4efa06ebeee9b431b690a47d6af40d77d8eabc557f'
  ],
  ['en_ppocrv5_dict.txt', 'e025a66d31f327ba0c232e03f407ae8d105e1e709e7ccb3f408aa778c24e70d6']
]
const pythonExe = join(python, 'python.exe')
if (!existsSync(pythonExe)) throw new Error(`Missing portable Python: ${pythonExe}`)
for (const [name, expected] of sources) {
  const path = join(models, 'formula-onnx', name)
  if (!existsSync(path)) throw new Error(`Missing formula model: ${path}`)
  const actual = createHash('sha256').update(readFileSync(path)).digest('hex')
  if (actual !== expected) throw new Error(`Formula model hash mismatch: ${name}`)
}
for (const [name, expected] of textSources) {
  const path = join(models, name)
  if (!existsSync(path)) throw new Error(`Missing text model: ${path}`)
  const actual = createHash('sha256').update(readFileSync(path)).digest('hex')
  if (actual !== expected) throw new Error(`Text model hash mismatch: ${name}`)
}

mkdirSync(target, { recursive: true })
cpSync(python, join(target, 'python'), { recursive: true, force: true })
mkdirSync(join(target, 'models', 'formula-onnx'), { recursive: true })
for (const [name] of sources) {
  copyFileSync(join(models, 'formula-onnx', name), join(target, 'models', 'formula-onnx', name))
}
for (const [name] of textSources) {
  copyFileSync(join(models, name), join(target, 'models', name))
}
copyFileSync(
  join(root, 'scripts', 'formula-ocr-onnx-worker.py'),
  join(target, 'formula-ocr-onnx-worker.py')
)
console.log(`[formula-runtime] staged portable low-memory formula runtime at ${target}`)

const highPython = process.env.CRUCIBLEBOX_PADDLE_PORTABLE_PYTHON
const highModels = process.env.CRUCIBLEBOX_PADDLE_MODEL_DIRECTORY
if (highPython || highModels) {
  if (!highPython || !highModels || !existsSync(join(highPython, 'python.exe'))) {
    throw new Error('High-precision formula runtime source is incomplete')
  }
  const highArtifacts = [
    ['PP-DocLayout-M', 'f374bb0269d91ab2eed393a5a2da6d73d98896ac5eeae4f9f585eba19bcbba74'],
    ['PP-FormulaNet_plus-L', '4245c39c181d1d21e472bc85c7434df9b23f177be46552c0542bf153addbc355']
  ]
  for (const [name, expected] of highArtifacts) {
    const path = join(highModels, name, 'inference.pdiparams')
    if (!existsSync(path)) throw new Error(`Missing Paddle formula model: ${path}`)
    const actual = createHash('sha256').update(readFileSync(path)).digest('hex')
    if (actual !== expected) throw new Error(`Paddle formula model hash mismatch: ${name}`)
  }
  const highTarget = join(target, 'high')
  cpSync(highPython, join(highTarget, 'python'), { recursive: true, force: true })
  for (const [name] of highArtifacts) {
    cpSync(join(highModels, name), join(highTarget, 'models', 'paddle-formula', name), {
      recursive: true,
      force: true
    })
  }
  for (const [name] of textSources) {
    copyFileSync(join(models, name), join(highTarget, 'models', name))
  }
  copyFileSync(
    join(root, 'scripts', 'formula-ocr-worker.py'),
    join(highTarget, 'formula-ocr-worker.py')
  )
  console.log(`[formula-runtime] staged opt-in high-precision runtime at ${highTarget}`)
}
