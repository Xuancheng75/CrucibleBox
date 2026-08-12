import { queryOne, queryAll, execute } from '../index'

export const SettingsRepository = {
  get(key: string): string | null {
    const row = queryOne<{ value: string }>(
      'SELECT value FROM settings WHERE key = ?',
      [key]
    )
    return row?.value ?? null
  },

  set(key: string, value: string): void {
    const existing = queryOne<{ key: string }>(
      'SELECT key FROM settings WHERE key = ?',
      [key]
    )
    if (existing) {
      execute('UPDATE settings SET value = ? WHERE key = ?', [value, key])
    } else {
      execute('INSERT INTO settings (key, value) VALUES (?, ?)', [key, value])
    }
  },

  getAll(): Record<string, string> {
    const rows = queryAll<{ key: string; value: string }>('SELECT key, value FROM settings')
    const result: Record<string, string> = {}
    for (const row of rows) {
      result[row.key] = row.value
    }
    return result
  },

  delete(key: string): void {
    execute('DELETE FROM settings WHERE key = ?', [key])
  }
}
