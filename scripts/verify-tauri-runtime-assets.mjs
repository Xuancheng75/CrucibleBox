import { existsSync, statSync } from 'node:fs'
import { resolve, join } from 'node:path'

const args = process.argv.slice(2)
const staged = args.includes('--staged')
const targetArg = args.find((arg) => !arg.startsWith('--'))
const target = resolve(targetArg ?? '.')

function firstFile(candidates) {
  return candidates.find((candidate) => {
    try {
      return existsSync(candidate) && statSync(candidate).isFile()
    } catch {
      return false
    }
  })
}

const workerCandidates = staged
  ? [
      join(target, 'src-tauri', 'binaries', 'ocr-worker-x86_64-pc-windows-msvc.exe'),
      join(target, 'src-tauri', 'binaries', 'ocr-worker.exe')
    ]
  : [
      join(target, 'ocr-worker.exe'),
      join(target, 'resources', 'ocr-worker.exe'),
      join(target, 'ocr-worker-x86_64-pc-windows-msvc.exe'),
      join(target, 'resources', 'ocr-worker-x86_64-pc-windows-msvc.exe')
    ]
const pdfiumCandidates = staged
  ? [join(target, 'src-tauri', 'resources', 'pdfium.dll')]
  : [join(target, 'pdfium.dll'), join(target, 'resources', 'pdfium.dll')]
const ortDirectories = staged
  ? [join(target, 'src-tauri', 'binaries')]
  : [target, join(target, 'resources'), join(target, 'binaries')]

const worker = firstFile(workerCandidates)
const pdfium = firstFile(pdfiumCandidates)
const ortDirectory = ortDirectories.find(
  (directory) =>
    firstFile([join(directory, 'onnxruntime.dll')]) &&
    firstFile([join(directory, 'onnxruntime_providers_shared.dll')])
)
const ort = ortDirectory
  ? [join(ortDirectory, 'onnxruntime.dll'), join(ortDirectory, 'onnxruntime_providers_shared.dll')]
  : [undefined, undefined]
const sevenZipDirectory = staged
  ? join(target, 'src-tauri', 'resources', '7zip')
  : join(target, 'resources', '7zip')
const sevenZip = [
  firstFile([join(sevenZipDirectory, '7za.exe')]),
  firstFile([join(sevenZipDirectory, '7za.dll')]),
  firstFile([join(sevenZipDirectory, 'License.txt')])
]
if (!worker || !pdfium || !ortDirectory || sevenZip.some((file) => !file)) {
  const missing = [
    !worker && 'ocr-worker.exe',
    !pdfium && 'pdfium.dll',
    !ort[0] && 'onnxruntime.dll',
    !ort[1] && 'onnxruntime_providers_shared.dll',
    !sevenZip[0] && '7zip/7za.exe',
    !sevenZip[1] && '7zip/7za.dll',
    !sevenZip[2] && '7zip/License.txt'
  ]
    .filter(Boolean)
    .join(', ')
  throw new Error(
    `Tauri runtime assets missing: ${missing}\n` +
      `checked root: ${target}\n` +
      `worker candidates: ${workerCandidates.join(', ')}\n` +
      `pdfium candidates: ${pdfiumCandidates.join(', ')}\n` +
      `ONNX Runtime directories: ${ortDirectories.join(', ')}\n` +
      `7-Zip directory: ${sevenZipDirectory}`
  )
}

console.log(`[tauri-assets] OCR Worker: ${worker}`)
console.log(`[tauri-assets] PDFium: ${pdfium}`)
console.log(`[tauri-assets] ONNX Runtime: ${ort[0]}`)
console.log(`[tauri-assets] ONNX Runtime providers: ${ort[1]}`)
console.log(`[tauri-assets] 7-Zip: ${sevenZip[0]}`)
