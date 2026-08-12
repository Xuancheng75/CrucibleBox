import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { DiagnosticLog } from '../electron/DiagnosticLog'

const directories: string[] = []

afterEach(() => {
  for (const directory of directories.splice(0)) {
    rmSync(directory, { recursive: true, force: true })
  }
})

function temporaryDirectory(): string {
  const directory = mkdtempSync(join(tmpdir(), 'openbox-diagnostics-'))
  directories.push(directory)
  return directory
}

describe('DiagnosticLog', () => {
  it('writes structured records and clears a clean session marker', () => {
    const directory = temporaryDirectory()
    const diagnostics = new DiagnosticLog(directory)
    const sessionId = diagnostics.startSession('1.0.0')
    diagnostics.write('error', 'renderer-gone', { reason: 'crashed' })
    diagnostics.finishSession()

    const records = readFileSync(join(directory, 'main.jsonl'), 'utf8')
      .trim()
      .split('\n')
      .map((line) => JSON.parse(line))
    expect(records).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          event: 'session-started',
          data: { sessionId, version: '1.0.0' }
        }),
        expect.objectContaining({ event: 'renderer-gone', data: { reason: 'crashed' } }),
        expect.objectContaining({ event: 'session-finished' })
      ])
    )
    expect(existsSync(join(directory, 'session.json'))).toBe(false)
  })

  it('reports a marker left by an unclean previous session', () => {
    const directory = temporaryDirectory()
    writeFileSync(
      join(directory, 'session.json'),
      JSON.stringify({ sessionId: 'old-session', startedAt: 'yesterday' })
    )
    const diagnostics = new DiagnosticLog(directory)
    diagnostics.startSession('2.0.0')

    expect(readFileSync(join(directory, 'main.jsonl'), 'utf8')).toContain(
      'previous-session-unclean'
    )
  })

  it('rotates a bounded log before appending a new record', () => {
    const directory = temporaryDirectory()
    writeFileSync(join(directory, 'main.jsonl'), 'x'.repeat(64))
    const diagnostics = new DiagnosticLog(directory, 32)
    diagnostics.write('info', 'after-rotation')

    expect(readFileSync(join(directory, 'main.jsonl.1'), 'utf8')).toBe('x'.repeat(64))
    expect(readFileSync(join(directory, 'main.jsonl'), 'utf8')).toContain('after-rotation')
  })
})
