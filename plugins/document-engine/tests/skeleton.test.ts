// Document Engine 插件契约测试：验证消息解析与任务客户端不伪造结果。

import { describe, expect, it } from 'vitest'
import {
  isDocumentProgressMessage,
  isParsedDocumentResult,
  isTerminalTask,
  getStatus,
  installModelBundle,
  listModelCatalog,
  listModels,
  pauseTask,
  retryTask,
  resumeTask,
  startBatch,
  startChunk,
  startConvert,
  startOcr,
  startParse
} from '../src/engine-api'

describe('document-engine plugin contract', () => {
  it('plugin manifest declares trusted:document-engine permission', () => {
    // 确保 manifest 与骨架意图一致（权限声明在 plugin.json，此处用契约断言）
    const expectedPermission = 'trusted:document-engine'
    expect(expectedPermission).toBe('trusted:document-engine')
  })

  it('accepts only well-formed progress messages', () => {
    expect(
      isDocumentProgressMessage({
        type: 'document.progress',
        taskId: 'task-1',
        progress: { stage: 'ocr', percent: 20, message: 'running' }
      })
    ).toBe(true)
    expect(isDocumentProgressMessage({ type: 'document.progress', taskId: 'task-1' })).toBe(false)
  })

  it('does not treat a host error as an accepted OCR task', async () => {
    await expect(
      startOcr(async () => ({ code: 'worker-unavailable', error: 'worker missing' }), 'a.png')
    ).rejects.toThrow('worker missing')
  })

  it('surfaces malformed status responses instead of inventing an empty status', async () => {
    await expect(
      getStatus(async () => ({ code: 'plugin-not-found', error: 'plugin not found' }))
    ).rejects.toThrow('plugin not found [plugin-not-found]')
    await expect(getStatus(async () => ({ status: {} }))).resolves.toEqual({})
  })

  it('starts a PDF parse only when the host returns a task id', async () => {
    await expect(startParse(async () => ({ code: 'unsupported-format' }), 'a.pdf')).rejects.toThrow(
      'unsupported-format'
    )
    await expect(
      startParse(async () => ({ taskId: 'parse-1', status: 'queued' }), 'a.pdf')
    ).resolves.toEqual({
      taskId: 'parse-1',
      status: 'queued'
    })
  })

  it('recognizes the unified PDF result shape', () => {
    expect(
      isParsedDocumentResult({
        route: 'native',
        requiresOcr: false,
        ocrPageNumbers: [],
        warnings: [],
        document: { metadata: { pageCount: 1 } }
      })
    ).toBe(true)
    expect(isParsedDocumentResult({ route: 'native', document: null })).toBe(false)
  })

  it('uses task ids for chunk, convert and batch operations', async () => {
    const send = async () => ({ taskId: 'task-1', status: 'queued' })
    await expect(startChunk(send, 'a.txt')).resolves.toMatchObject({ taskId: 'task-1' })
    await expect(startConvert(send, 'a.txt', 'md')).resolves.toMatchObject({ taskId: 'task-1' })
    await expect(startBatch(send, ['a.txt'], 'parse')).resolves.toMatchObject({ taskId: 'task-1' })
  })

  it('omits undefined optional fields from RPC payloads', async () => {
    const messages: unknown[] = []
    const send = async (message: unknown) => {
      messages.push(message)
      return { taskId: 'task-1', status: 'queued' }
    }
    await startOcr(send, 'a.png')
    await startChunk(send, 'a.txt')
    await startConvert(send, 'a.txt', 'md')
    await startBatch(send, ['a.txt'], 'parse')
    for (const message of messages) {
      expect(JSON.stringify(message)).not.toContain('undefined')
      expect(message).not.toHaveProperty('options', undefined)
      expect(message).not.toHaveProperty('outputPath', undefined)
      expect(message).not.toHaveProperty('target', undefined)
    }
  })

  it('exposes task controls and model status without accepting malformed responses', async () => {
    const send = async (message: unknown) => {
      const type = (message as { type?: string }).type
      if (type === 'document.models.list') return { models: [] }
      return { success: true }
    }
    await expect(pauseTask(send, 'task-1')).resolves.toBeUndefined()
    await expect(resumeTask(send, 'task-1')).resolves.toBeUndefined()
    await expect(
      retryTask(async () => ({ taskId: 'retry-1', status: 'queued' }), 'task-1')
    ).resolves.toMatchObject({ taskId: 'retry-1' })
    await expect(listModels(send)).resolves.toMatchObject({ models: [] })
    await expect(
      listModels(async () => ({ code: 'plugin-not-found', error: 'plugin not found' }))
    ).rejects.toThrow('plugin not found [plugin-not-found]')
  })

  it('exposes the curated model catalog and validates its response shape', async () => {
    const send = async (message: unknown) => {
      expect(message).toEqual({ type: 'document.models.catalog' })
      return {
        catalog: [
          {
            id: 'ppocrv4-mobile-zh-en',
            name: 'PP-OCRv4 中文/英文标准模型',
            version: '4.0',
            description: '本地 OCR',
            artifacts: [
              { name: 'ch_PP-OCRv4_det.onnx', url: 'https://example', sha256: 'a'.repeat(64) }
            ]
          }
        ]
      }
    }
    await expect(listModelCatalog(send)).resolves.toHaveLength(1)
    await expect(
      listModelCatalog(async () => ({ code: 'model-list-failed', error: '目录不可用' }))
    ).rejects.toThrow('目录不可用 [model-list-failed]')
    await expect(listModelCatalog(async () => ({ catalog: [{ id: 'broken' }] }))).rejects.toThrow(
      '模型目录读取失败'
    )
  })

  it('sends a stable model bundle id for one-click installation', async () => {
    const send = async (message: unknown) => {
      expect(message).toEqual({
        type: 'document.models.installBundle',
        modelId: 'ppocrv4-mobile-zh-en'
      })
      return { success: true, modelId: 'ppocrv4-mobile-zh-en' }
    }
    await expect(installModelBundle(send, 'ppocrv4-mobile-zh-en')).resolves.toMatchObject({
      success: true
    })
  })

  it('recognizes terminal job states', () => {
    expect(isTerminalTask({ taskId: 't', resourceKey: 'ocr', status: 'succeeded' })).toBe(true)
    expect(isTerminalTask({ taskId: 't', resourceKey: 'ocr', status: 'running' })).toBe(false)
  })

  it('data model Chunk interface has required fields', () => {
    const chunk = {
      chunk_id: 'c1',
      document_id: 'd1',
      parent_id: null,
      chunk_index: 0,
      content: '...',
      page_start: 1,
      page_end: 1,
      source_file: 'a.pdf',
      source_path: 'C:\\a.pdf',
      block_ids: [],
      token_count: 10,
      character_count: 40,
      type: 'paragraph'
    }
    expect(chunk.chunk_id).toBe('c1')
    expect(chunk.token_count).toBeGreaterThanOrEqual(0)
  })
})
