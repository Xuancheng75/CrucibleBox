// PoC 辅助：最小静态文件服务器，serving poc-tauri-frontend at :1430（带访问日志）
import { createServer } from 'node:http'
import { appendFileSync, readFileSync, existsSync, statSync } from 'node:fs'
import { extname, join, normalize } from 'node:path'

const root = process.argv[2] || 'poc-tauri-frontend'
const port = Number(process.argv[3] || 1430)
const mime = {
  '.html': 'text/html',
  '.js': 'application/javascript',
  '.mjs': 'application/javascript',
  '.css': 'text/css',
  '.json': 'application/json',
  '.svg': 'image/svg+xml',
  '.png': 'image/png'
}

createServer((req, res) => {
  const urlPath = decodeURIComponent((req.url || '/').split('?')[0])
  const logLine = `[serve] ${new Date().toISOString()} ${req.method} ${urlPath}`
  console.log(logLine)
  try {
    appendFileSync(join(process.env.TEMP || '.', 'poc-serve-access.log'), logLine + '\n')
  } catch {}
  let filePath = normalize(join(root, urlPath === '/' ? 'index.html' : urlPath))
  if (!filePath.startsWith(normalize(root))) {
    res.writeHead(403).end()
    return
  }
  if (!existsSync(filePath) || statSync(filePath).isDirectory()) {
    filePath = join(root, 'index.html')
  }
  try {
    const body = readFileSync(filePath)
    res.writeHead(200, { 'Content-Type': mime[extname(filePath)] || 'application/octet-stream' })
    res.end(body)
  } catch {
    res.writeHead(404).end()
  }
}).listen(port, () => console.log(`[poc-serve] http://localhost:${port} <- ${root}`))
