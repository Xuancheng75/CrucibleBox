# Document Engine 统一数据模型（草案）

> Phase 0 产物。对应需求 `OCR.md` 第七阶段（统一 Document 结构）与第十四/十五阶段（Chunk 结构）。
> 此草案在 Phase 6（Unified Document Model）时落地为具体代码。
> **实施状态**：模型已在 Rust `document_parser`、`pdf_parser`、`document_chunker` 和 converter 中落地；最终实现与验收见
> [`document-engine-development-report.md`](document-engine-development-report.md)。

## 1. 设计原则

- 所有引擎（PaddleOCR / MinerU / Native Parser）输出统一转换为此模型
- 前端只感知 `Document Engine API`，不感知底层 OCR/解析引擎
- JSON 为中间交换格式，便于缓存、RAG 导出、批量处理

## 2. Document 结构

```typescript
interface Document {
  id: string
  source: Source
  metadata: Metadata
  pages: Page[]
  structure: DocumentStructure
}

interface Source {
  path: string
  mime: string
  size: number
  hash: string // 用于缓存 key
  engine: string // 'paddleocr' | 'mineru' | 'native'
  engineVersion: string
}

interface Metadata {
  title?: string
  author?: string
  createdAt?: string
  language?: string // 'zh' | 'en' | 'mixed'
  pageCount: number
  hasTextLayer: boolean
  isScanned: boolean
  hasTables: boolean
  hasFormulas: boolean
  hasImages: boolean
  encoding?: string
}

interface Page {
  number: number
  width: number
  height: number
  blocks: Block[]
}

type BlockType =
  | 'title'
  | 'heading'
  | 'paragraph'
  | 'text'
  | 'image'
  | 'table'
  | 'formula'
  | 'list'
  | 'code'
  | 'quote'
  | 'header'
  | 'footer'
  | 'caption'

interface Block {
  id: string
  type: BlockType
  bbox?: [number, number, number, number] // [x1, y1, x2, y2]，归一化或像素坐标
  content?: string // 文本/表格 markdown/公式 LaTeX
  confidence?: number // OCR 置信度 0-1
  level?: number // heading 层级 1-6
  children?: Block[] // 嵌套结构（列表项、表格单元格等）
  language?: string
}

interface DocumentStructure {
  outline: OutlineNode[] // 标题层级树
  readingOrder: string[] // block id 的阅读顺序
}

interface OutlineNode {
  id: string
  title: string
  level: number
  page: number
  children: OutlineNode[]
}
```

## 3. Chunk 结构

对应需求第十四阶段的字段要求：

```typescript
interface Chunk {
  chunk_id: string
  document_id: string
  parent_id: string | null
  chunk_index: number

  title?: string
  chapter?: string
  section?: string
  subsection?: string

  content: string

  page_start: number
  page_end: number

  source_file: string
  source_path: string

  block_ids: string[]

  language?: string

  token_count: number
  character_count: number

  type: BlockType

  ocr_confidence?: number
}
```

## 4. Chunk 策略

| 策略 | 说明 | 默认 |
|------|------|------|
| Structure | 按标题/章节/段落结构切分 | |
| Semantic | 语义边界 + token 限制 | |
| Hybrid | 结构优先，超长块再按语义/ token 拆分 | ✅ 默认 |

可配置参数：

```typescript
interface ChunkOptions {
  strategy: 'structure' | 'semantic' | 'hybrid'
  targetTokens: number // 目标 token 数
  maxTokens: number // 最大 token 数（超则强制拆分）
  overlap: number // 块间重叠 token 数
  minChunkSize: number // 最小块大小（字符）
}
```

数学文档保护（需求第十六阶段）：定理 + 证明 + 公式不随意拆分，优先保持完整性。

## 5. 转换矩阵（目标，Phase 8 实现）

| 源 \ 目标 | Markdown | DOCX | TXT | HTML | JSON | PDF |
|-----------|----------|------|-----|------|------|-----|
| PDF（文本） | ✅ | ✅ | ✅ | - | ✅ | - |
| PDF（扫描） | ✅ | ⚠️ | ✅ | - | ✅ | - |
| PDF（混合） | ✅ | ✅ | ✅ | - | ✅ | - |
| DOCX | ✅ | - | ✅ | - | ✅ | ✅ |
| Markdown | - | ✅ | ✅ | ✅ | ✅ | ✅ |
| HTML | ✅ | - | ✅ | - | ✅ | - |
| TXT | ✅ | - | - | - | ✅ | - |

图例：✅ 稳定 / ⚠️ 实验（复杂公式降级为 LaTeX + warning）

所有转换经过 Unified Document Model 中间层，避免 N×N 直接转换。

## 6. 导出格式

统一内部标准为 `Document JSON`，可导出：

- `Markdown`：标题层级 + 表格 + 公式（LaTeX）
- `JSON`：完整 Document 结构（含 bbox / confidence）
- `TXT`：纯文本（保留段落）
- `Chunks`：Chunk[] 数组（用于 RAG / Embedding）

## 7. 缓存 Key 生成

```typescript
function cacheKey(doc: {
  sourceHash: string
  engine: string
  engineVersion: string
  options: Record<string, unknown>
}): string {
  const payload = JSON.stringify({
    sourceHash: doc.sourceHash,
    engine: doc.engine,
    engineVersion: doc.engineVersion,
    options: doc.options,
  })
  return crypto.createHash('sha256').update(payload).digest('hex')
}
```
