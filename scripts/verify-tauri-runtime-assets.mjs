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
const ortCandidates = staged
  ? [
      join(target, 'src-tauri', 'binaries', 'onnxruntime.dll'),
      join(target, 'src-tauri', 'binaries', 'onnxruntime_providers_shared.dll')
    ]
  : [
      join(target, 'onnxruntime.dll'),
      join(target, 'resources', 'onnxruntime.dll'),
      join(target, 'binaries', 'onnxruntime.dll')
    ]

const worker = firstFile(workerCandidates)
const pdfium = firstFile(pdfiumCandidates)
const ort = ortCandidates.map((candidate) => firstFile([candidate]))
if (!worker || !pdfium || ort.some((candidate) => !candidate)) {
  const missing = [
    !worker && 'ocr-worker.exe',
    !pdfium && 'pdfium.dll',
    !ort[0] && 'onnxruntime.dll',
    !ort[1] && 'onnxruntime_providers_shared.dll'
  ]
    .filter(Boolean)
    .join(', ')
  throw new Error(
    `Tauri runtime assets missing: ${missing}\n` +
      `checked root: ${target}\n` +
      `worker candidates: ${workerCandidates.join(', ')}\n` +
      `pdfium candidates: ${pdfiumCandidates.join(', ')}\n` +
      `ONNX Runtime candidates: ${ortCandidates.join(', ')}`
  )
}

console.log(`[tauri-assets] OCR Worker: ${worker}`)
console.log(`[tauri-assets] PDFium: ${pdfium}`)
console.log(`[tauri-assets] ONNX Runtime: ${ort[0]}`)
console.log(`[tauri-assets] ONNX Runtime providers: ${ort[1]}`)
