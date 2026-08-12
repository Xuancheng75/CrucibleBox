import { mkdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { randomUUID } from 'node:crypto'

export interface DiagnosticRecord {
  timestamp: string
  level: 'info' | 'warn' | 'error' | 'fatal'
  event: string
  data?: unknown
}

export class DiagnosticLog {
  private readonly directory: string
  private readonly logPath: string
  private readonly markerPath: string
  private readonly maximumBytes: number

  constructor(directory: string, maximumBytes = 2 * 1024 * 1024) {
    this.directory = directory
    this.logPath = join(directory, 'main.jsonl')
    this.markerPath = join(directory, 'session.json')
    this.maximumBytes = maximumBytes
  }

  startSession(version: string): string {
    mkdirSync(this.directory, { recursive: true })
    try {
      const previous = JSON.parse(readFileSync(this.markerPath, 'utf8')) as {
        sessionId?: unknown
        startedAt?: unknown
      }
      this.write('warn', 'previous-session-unclean', {
        sessionId: String(previous.sessionId ?? 'unknown'),
        startedAt: String(previous.startedAt ?? 'unknown')
      })
    } catch {
      // A missing or malformed marker is replaced by the new session marker.
    }
    const sessionId = randomUUID()
    const marker = JSON.stringify({ sessionId, version, startedAt: new Date().toISOString() })
    const temporaryPath = `${this.markerPath}.tmp`
    writeFileSync(temporaryPath, marker)
    renameSync(temporaryPath, this.markerPath)
    this.write('info', 'session-started', { sessionId, version })
    return sessionId
  }

  finishSession(): void {
    this.write('info', 'session-finished')
    rmSync(this.markerPath, { force: true })
  }

  write(level: DiagnosticRecord['level'], event: string, data?: unknown): void {
    try {
      mkdirSync(this.directory, { recursive: true })
      this.rotateIfNeeded()
      const record: DiagnosticRecord = {
        timestamp: new Date().toISOString(),
        level,
        event,
        ...(data === undefined ? {} : { data })
      }
      writeFileSync(this.logPath, `${JSON.stringify(record)}\n`, { flag: 'a' })
    } catch (error) {
      console.error('[diagnostics] failed to write:', error)
    }
  }

  private rotateIfNeeded(): void {
    try {
      if (statSync(this.logPath).size < this.maximumBytes) return
      const backupPath = `${this.logPath}.1`
      rmSync(backupPath, { force: true })
      renameSync(this.logPath, backupPath)
    } catch {
      // The active log usually does not exist during the first write.
    }
  }
}
