import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

/**
 * db-recovery-tool — 离线数据库读取/修复工具（sql.js，仅供排障）。
 *
 * 1.6.0 起生产仅使用 better-sqlite3（WAL）；sql.js 降级为测试与离线恢复工具。
 * 本工具用 sql.js 打开 openbox.db（SQLite 文件），执行只读查询或导出诊断快照，
 * 供宿主无法启动 / 数据库损坏排查时离线使用。不做生产 fallback。
 *
 * 用法：
 *   node scripts/db-recovery-tool.mjs <db-path> <sql>            # 执行只读查询
 *   node scripts/db-recovery-tool.mjs <db-path> --tables          # 列出表
 *   node scripts/db-recovery-tool.mjs <db-path> --export <out>    # 导出字节级副本
 */

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const require = createRequire(fileURLToPath(import.meta.url))

async function loadSqlJs() {
  const { default: initSqlJs } = await import('sql.js')
  // 优先加载本地 wasm 二进制，避免网络/路径问题
  const wasmPath = resolve(dirname(require.resolve('sql.js')), 'sql-wasm.wasm')
  if (existsSync(wasmPath)) {
    return initSqlJs({ wasmBinary: readFileSync(wasmPath) })
  }
  return initSqlJs()
}

async function main() {
  const [dbPath, command, arg] = process.argv.slice(2)
  if (!dbPath || !command) {
    throw new Error('Usage: db-recovery-tool.mjs <db-path> <sql> | --tables | --export <out>')
  }
  if (!existsSync(dbPath)) {
    throw new Error(`Database file does not exist: ${dbPath}`)
  }

  const sqlJsRuntime = await loadSqlJs()
  const database = new sqlJsRuntime.Database(readFileSync(dbPath))
  try {
    if (command === '--tables') {
      const rows = database.exec(
        "SELECT name FROM sqlite_master WHERE type IN ('table','index') ORDER BY name"
      )
      for (const row of rows) {
        for (const values of row.values ?? []) {
          console.log(String(values[0]))
        }
      }
      return
    }
    if (command === '--export') {
      if (!arg) throw new Error('--export requires an output path')
      writeFileSync(arg, Buffer.from(database.export()))
      console.log(`[db-recovery] exported ${dbPath} -> ${arg}`)
      return
    }
    // 只读查询（禁止写语句，防误伤）
    const upper = command.trim().toUpperCase()
    if (!/^SELECT|^PRAGMA|^EXPLAIN/u.test(upper)) {
      throw new Error('Only read-only queries are allowed (SELECT/PRAGMA/EXPLAIN)')
    }
    const result = database.exec(command)
    if (result.length === 0) {
      console.log('(no rows)')
      return
    }
    for (const statement of result) {
      const columns = statement.columns
      console.log(columns.join('\t'))
      for (const values of statement.values ?? []) {
        console.log(values.map((value) => String(value)).join('\t'))
      }
    }
  } finally {
    database.close()
  }
}

void main().catch((error) => {
  console.error(`[db-recovery] ${error.message}`)
  process.exit(1)
})
