// Bug A 守卫：校验安装器内嵌应用 exe 已包含新图标（icons/icon.ico）的像素数据。
// 原理：解析 ICO 目录，取最大 PNG 条目（256px，PNG 压缩存储），在应用 exe 的
// PE 资源段中做字节搜索。tauri-build 把窗口图标作为 RT_ICON 资源原样嵌入，
// 因此字节级存在性等价于"图标已重新嵌入"。旧图标（8KB 单一 32x32 BMP）不含
// 该 PNG 载荷，可作新旧判据。
//
// 用法：node scripts/verify-tauri-icon.mjs --icon <path/to/icon.ico> --exe <path/to/app.exe>
// 退出码非 0 = 图标未嵌入（CI 门禁失败）。

import { readFileSync } from 'node:fs'

function parseArgs(argv) {
  const args = {}
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i]
    if (key === '--icon' || key === '--exe') args[key.slice(2)] = argv[i + 1]
  }
  return args
}

const { icon: iconPath, exe: exePath } = parseArgs(process.argv.slice(2))
if (!iconPath || !exePath) {
  console.error('usage: node scripts/verify-tauri-icon.mjs --icon <icon.ico> --exe <app.exe>')
  process.exit(2)
}

const PNG_SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])

function extractLargestPngPayload(ico) {
  if (ico.length < 6 || ico.readUInt16LE(0) !== 0 || ico.readUInt16LE(2) !== 1) {
    throw new Error('not a valid .ico file (bad ICONDIR)')
  }
  const count = ico.readUInt16LE(4)
  let best = null
  for (let i = 0; i < count; i++) {
    const entryOffset = 6 + i * 16
    const widthByte = ico[entryOffset] // 0 表示 256
    const size = ico.readUInt32LE(entryOffset + 8)
    const offset = ico.readUInt32LE(entryOffset + 12)
    if (offset + size > ico.length) throw new Error(`ico entry ${i} out of range`)
    const data = ico.subarray(offset, offset + size)
    if (!data.subarray(0, 8).equals(PNG_SIGNATURE)) continue
    const width = widthByte === 0 ? 256 : widthByte
    if (!best || width > best.width) best = { width, data: Buffer.from(data) }
  }
  if (!best) throw new Error('icon.ico contains no PNG-compressed entry (expected >=256px)')
  return best
}

function validateRequiredLayers(ico) {
  const count = ico.readUInt16LE(4)
  const layers = new Map()
  for (let i = 0; i < count; i += 1) {
    const entryOffset = 6 + i * 16
    const width = ico[entryOffset] === 0 ? 256 : ico[entryOffset]
    const height = ico[entryOffset + 1] === 0 ? 256 : ico[entryOffset + 1]
    if (width !== height) throw new Error(`icon.ico entry ${i} is not square (${width}x${height})`)
    layers.set(width, (layers.get(width) ?? 0) + 1)
  }
  const required = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
  const missing = required.filter((size) => !layers.has(size))
  if (missing.length > 0) {
    throw new Error(`icon.ico is missing Windows DPI layers: ${missing.join(', ')}`)
  }
}

const iconBuffer = readFileSync(iconPath)
if (iconBuffer.length < 6 || iconBuffer.readUInt16LE(0) !== 0 || iconBuffer.readUInt16LE(2) !== 1) {
  throw new Error('not a valid .ico file (bad ICONDIR)')
}
validateRequiredLayers(iconBuffer)
const payload = extractLargestPngPayload(iconBuffer)
const exeBuffer = readFileSync(exePath)

if (!exeBuffer.includes(payload.data)) {
  console.error(
    `FAIL: embedded icon payload (${payload.width}px PNG, ${payload.data.length} bytes) NOT found in ${exePath}. ` +
      'App exe still carries a stale icon (cargo/tauri-build cache?).'
  )
  process.exit(1)
}

console.log(
  `OK: ${payload.width}px PNG icon payload (${payload.data.length} bytes) found in ${exePath}`
)
