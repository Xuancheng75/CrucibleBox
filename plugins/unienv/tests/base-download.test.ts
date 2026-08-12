import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  DownloadIntegrityError,
  downloadWithFallback,
  downloadWithProgress
} from '../src/tools/base'

const ZERO_SHA256 = '0'.repeat(64)

function sha256(value: string): string {
  return createHash('sha256').update(value).digest('hex')
}

function responseFromChunks(
  chunks: readonly Uint8Array[],
  headers: Record<string, string> = {}
): Response {
  return new Response(
    new ReadableStream<Uint8Array>({
      start(controller) {
        for (const chunk of chunks) controller.enqueue(chunk)
        controller.close()
      }
    }),
    { status: 200, headers }
  )
}

describe('bounded downloads', () => {
  let testDir: string
  let destPath: string

  beforeEach(() => {
    testDir = mkdtempSync(join(tmpdir(), 'unienv-download-'))
    destPath = join(testDir, 'archive.bin')
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    rmSync(testDir, { force: true, recursive: true })
  })

  it('rejects non-HTTPS URLs before fetching', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)

    await expect(
      downloadWithProgress('http://example.test/archive', destPath, vi.fn(), 'test', {
        expectedSha256: ZERO_SHA256
      })
    ).rejects.toThrow('HTTPS')
    expect(fetchMock).not.toHaveBeenCalled()
    expect(existsSync(destPath)).toBe(false)
  })

  it('rejects an invalid expected digest before fetching', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)

    await expect(
      downloadWithProgress('https://example.test/archive', destPath, vi.fn(), 'test', {
        expectedSha256: 'not-a-sha256'
      })
    ).rejects.toThrow('expectedSha256')
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('rejects an oversized Content-Length before creating a partial file', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => responseFromChunks([new Uint8Array([1])], { 'content-length': '5' }))
    )

    await expect(
      downloadWithProgress('https://example.test/archive', destPath, vi.fn(), 'test', {
        expectedSha256: ZERO_SHA256,
        maxBytes: 4
      })
    ).rejects.toThrow('上限')
    expect(existsSync(destPath)).toBe(false)
    expect(existsSync(`${destPath}.part`)).toBe(false)
  })

  it('enforces the cumulative byte limit and cleans the partial file', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => responseFromChunks([new Uint8Array([1, 2, 3]), new Uint8Array([4, 5])]))
    )

    await expect(
      downloadWithProgress('https://example.test/archive', destPath, vi.fn(), 'test', {
        expectedSha256: ZERO_SHA256,
        maxBytes: 4
      })
    ).rejects.toThrow('上限')
    expect(existsSync(destPath)).toBe(false)
    expect(existsSync(`${destPath}.part`)).toBe(false)
  })

  it('writes a bounded stream to .part and atomically renames it on success', async () => {
    const first = new TextEncoder().encode('hel')
    const second = new TextEncoder().encode('lo')
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => responseFromChunks([first, second], { 'content-length': '5' }))
    )

    await downloadWithProgress('https://example.test/archive', destPath, vi.fn(), 'test', {
      expectedSha256: sha256('hello'),
      maxBytes: 5
    })

    expect(readFileSync(destPath, 'utf8')).toBe('hello')
    expect(existsSync(`${destPath}.part`)).toBe(false)
  })

  it('cancels a pending body read and cleans .part when aborted', async () => {
    let streamCancelled = false
    const body = new ReadableStream<Uint8Array>({
      cancel() {
        streamCancelled = true
      }
    })
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(body, { status: 200 }))
    )
    const controller = new AbortController()
    const operation = downloadWithProgress(
      'https://example.test/archive',
      destPath,
      vi.fn(),
      'test',
      {
        expectedSha256: ZERO_SHA256,
        signal: controller.signal,
        maxBytes: 16
      }
    )

    await vi.waitFor(() => expect(existsSync(`${destPath}.part`)).toBe(true))
    controller.abort()

    await expect(operation).rejects.toMatchObject({ name: 'AbortError' })
    expect(streamCancelled).toBe(true)
    expect(existsSync(destPath)).toBe(false)
    expect(existsSync(`${destPath}.part`)).toBe(false)
  })

  it('times out a body that stalls after headers and cleans .part', async () => {
    let streamCancelled = false
    const body = new ReadableStream<Uint8Array>({
      cancel() {
        streamCancelled = true
      }
    })
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(body, { status: 200 }))
    )

    await expect(
      downloadWithProgress('https://example.test/archive', destPath, vi.fn(), 'test', {
        expectedSha256: ZERO_SHA256,
        maxBytes: 16,
        idleTimeoutMs: 15
      })
    ).rejects.toMatchObject({ name: 'DownloadIdleTimeoutError' })
    expect(streamCancelled).toBe(true)
    expect(existsSync(destPath)).toBe(false)
    expect(existsSync(`${destPath}.part`)).toBe(false)
  })

  it('rejects a digest mismatch before the atomic rename and cleans .part', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => responseFromChunks([new TextEncoder().encode('tampered')]))
    )

    await expect(
      downloadWithProgress('https://mirror.example/archive', destPath, vi.fn(), 'test', {
        expectedSha256: sha256('official')
      })
    ).rejects.toBeInstanceOf(DownloadIntegrityError)
    expect(existsSync(destPath)).toBe(false)
    expect(existsSync(`${destPath}.part`)).toBe(false)
  })

  it('falls back after a mirror digest mismatch and accepts matching bytes', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(responseFromChunks([new TextEncoder().encode('tampered')]))
      .mockResolvedValueOnce(responseFromChunks([new TextEncoder().encode('official')]))
    vi.stubGlobal('fetch', fetchMock)

    await downloadWithFallback(
      [
        { url: 'https://mirror.example/archive', label: 'mirror' },
        { url: 'https://official.example/archive', label: 'official' }
      ],
      destPath,
      vi.fn(),
      sha256('official')
    )

    expect(fetchMock).toHaveBeenCalledTimes(2)
    expect(readFileSync(destPath, 'utf8')).toBe('official')
  })

  it('fails closed when both mirror and official bytes mismatch', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(responseFromChunks([new TextEncoder().encode('mirror-tampered')]))
      .mockResolvedValueOnce(responseFromChunks([new TextEncoder().encode('official-tampered')]))
    vi.stubGlobal('fetch', fetchMock)

    await expect(
      downloadWithFallback(
        [
          { url: 'https://mirror.example/archive', label: 'mirror' },
          { url: 'https://official.example/archive', label: 'official' }
        ],
        destPath,
        vi.fn(),
        sha256('official')
      )
    ).rejects.toBeInstanceOf(DownloadIntegrityError)

    expect(fetchMock).toHaveBeenCalledTimes(2)
    expect(existsSync(destPath)).toBe(false)
    expect(existsSync(`${destPath}.part`)).toBe(false)
  })
})
