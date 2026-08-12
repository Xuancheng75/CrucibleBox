import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { PluginRenderProps } from 'openbox-plugin-api'
import {
  decodeGifFile,
  encodeGif,
  createBlankFrame,
  frameToDataURL,
  transforms,
  removeBackground,
  deleteRectRegion,
  deleteRegionByMask,
  polygonMask,
  floodFill,
  splitColorLayers,
  splitGrid,
  splitSubjectBackground,
  splitConnectedObjects,
  splitColorClusters,
  mergeLayers,
  splitByMask,
} from './utils/gif'
import type { ResidueReport } from './utils/gif'
import { isResidueWorkerAbortError, runResidueWorker } from './residue-worker'
import type { GifDocument, GifFrame, Rgb } from './types'
import {
  appendHistoryEntry,
  applyHistoryDelta,
  applyFilterValues,
  cloneHistoryEntry,
  createHistoryEntry,
  hasPendingFilters,
  reconcileThumbnailCache,
  type FilterValues,
  type HistoryEntry as RendererHistoryEntry,
  type ThumbnailCache,
} from './renderer-state'
import { retainDynamicStyle } from './renderer-style'
import {
  applyCanvasTransform,
  cropCanvasToFrameBounds,
  cropCanvasToUnionBounds,
} from './document-operations'
import {
  assertGifDocumentCanvasInvariant,
  validateGifEncodeBudget,
} from './utils/gif-validation'
import gifEditorCss from './gif-editor.css'

type ToolTab = 'geometry' | 'draw' | 'filter' | 'color' | 'smart' | 'select'
type DrawTool = 'brush' | 'eraser' | 'text'
type SelectMode = 'none' | 'rect' | 'wand' | 'lasso'
type LassoMode = 'poly' | 'free'
type LayerItem = { id: string; imageData: ImageData; visible: boolean }
type SplitMode = 'subject' | 'objects' | 'colors' | 'lasso'
type HistoryEntry = RendererHistoryEntry<ImageData>

const MAX_HISTORY_ENTRIES = 50
const MAX_HISTORY_BYTES = 256 * 1024 * 1024
const HISTORY_LIMITS = { maxEntries: MAX_HISTORY_ENTRIES, maxBytes: MAX_HISTORY_BYTES }
const STYLE_ID = 'gif-editor-plugin-styles'

export default function GifEditorPlugin({ api }: PluginRenderProps) {
  const [doc, setDoc] = useState<GifDocument | null>(null)

  useEffect(() => {
    return retainDynamicStyle(document, STYLE_ID, gifEditorCss)
  }, [])
  const [current, setCurrent] = useState(0)
  const [tab, setTab] = useState<ToolTab>('geometry')
  const [drawTool, setDrawTool] = useState<DrawTool>('brush')
  const [brushColor, setBrushColor] = useState('#e11d48')
  const [brushSize, setBrushSize] = useState(6)
  const [eraserSize, setEraserSize] = useState(16)
  const [textContent, setTextContent] = useState('')
  const [textFontSize, setTextFontSize] = useState(24)
  const [textColor, setTextColor] = useState('#ffffff')
  const [textEntryMode, setTextEntryMode] = useState(false)
  const [textPos, setTextPos] = useState<{ x: number; y: number } | null>(null)
  const [zoom, setZoom] = useState(1)
  const [zoomFit, setZoomFit] = useState(true)
  const [playing, setPlaying] = useState(false)
  const [cropMode, setCropMode] = useState(false)
  const [cropRect, setCropRect] = useState<{ x: number; y: number; w: number; h: number } | null>(null)
  const [targetColor, setTargetColor] = useState<Rgb>([0, 0, 0])
  const [replaceColor, setReplaceColor] = useState('#ff0000')
  const [tolerance, setTolerance] = useState(30)
  const [brightnessVal, setBrightnessVal] = useState(0)
  const [contrastVal, setContrastVal] = useState(0)
  const [saturationVal, setSaturationVal] = useState(0)
  const [smartTolerance, setSmartTolerance] = useState(32)
  const [gridRows, setGridRows] = useState(2)
  const [gridCols, setGridCols] = useState(2)
  const [colorLayers, setColorLayers] = useState(6)
  const [colorLayerTol, setColorLayerTol] = useState(32)
  const [splitMode, setSplitMode] = useState<SplitMode>('subject')
  const [clusterCount, setClusterCount] = useState(3)
  const [layerSession, setLayerSession] = useState<LayerItem[] | null>(null)
  const [activeLayerId, setActiveLayerId] = useState<string | null>(null)
  const [, setLayerUndoStack] = useState<LayerItem[][]>([])
  const [, setLayerRedoStack] = useState<LayerItem[][]>([])
  const [selectMode, setSelectMode] = useState<SelectMode>('none')
  const [selectRect, setSelectRect] = useState<{ x: number; y: number; w: number; h: number } | null>(null)
  const [wandTolerance, setWandTolerance] = useState(40)
  const [wandMask, setWandMask] = useState<Uint8Array | null>(null)
  const [lassoMode, setLassoMode] = useState<LassoMode>('poly')
  const [lassoPoints, setLassoPoints] = useState<{ x: number; y: number }[]>([])
  const [lassoPreview, setLassoPreview] = useState<{ x: number; y: number } | null>(null)
  const [lassoEdgeWidth, setLassoEdgeWidth] = useState(2)
  const [lassoMask, setLassoMask] = useState<Uint8Array | null>(null)
  const [busy, setBusy] = useState(false)
  const [undoStack, setUndoStack] = useState<HistoryEntry[]>([])
  const [redoStack, setRedoStack] = useState<HistoryEntry[]>([])
  const [showExportSettings, setShowExportSettings] = useState(false)
  const [repeat, setRepeat] = useState(0)
  const [delayMode, setDelayMode] = useState<'unified' | 'perframe'>('perframe')
  const [unifiedDelay, setUnifiedDelay] = useState(150)
  const [exportScale, setExportScale] = useState(100)
  const [exporting, setExporting] = useState(false)
  const [progress, setProgress] = useState(0)
  const [batchDelay, setBatchDelay] = useState(150)
  const [batchFrom, setBatchFrom] = useState(1)
  const [batchTo, setBatchTo] = useState(1)
  const [thumbCache, setThumbCache] = useState<ThumbnailCache<ImageData>>(new Map())
  const [dragOver, setDragOver] = useState(false)
  const [residueReport, setResidueReport] = useState<ResidueReport | null>(null)
  const [residueAnalyzing, setResidueAnalyzing] = useState(false)

  const canvasRef = useRef<HTMLCanvasElement>(null)
  const editCanvasRef = useRef<HTMLCanvasElement>(null)
  const previewCanvasRef = useRef<HTMLCanvasElement>(null)
  const lassoCanvasRef = useRef<HTMLCanvasElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const fileRef = useRef<File | null>(null)
  const docRef = useRef<GifDocument | null>(null)
  const currentRef = useRef(0)
  const playingRef = useRef(false)
  const playTimerRef = useRef<number | null>(null)
  const drawingRef = useRef(false)
  const lastPosRef = useRef<{ x: number; y: number } | null>(null)
  const cropStartRef = useRef<{ x: number; y: number } | null>(null)
  const selectStartRef = useRef<{ x: number; y: number } | null>(null)
  const lassoFreeRef = useRef(false)
  const containerRef = useRef<HTMLDivElement>(null)
  const residueAbortRef = useRef<AbortController | null>(null)

  docRef.current = doc
  currentRef.current = current

  const frame = doc?.frames[current] ?? null

  const totalDuration = useMemo(() => {
    if (!doc) return 0
    return doc.frames.reduce((sum, f) => sum + (f.delay || 0), 0)
  }, [doc])

  const filterValues = useMemo<FilterValues>(
    () => ({ brightness: brightnessVal, contrast: contrastVal, saturation: saturationVal }),
    [brightnessVal, contrastVal, saturationVal]
  )

  const pushHistory = useCallback((entry: HistoryEntry | null) => {
    if (!entry) return
    setUndoStack((stack) => {
      return appendHistoryEntry(stack, entry, HISTORY_LIMITS)
    })
    setRedoStack([])
  }, [])

  const snapshot = useCallback((source: GifDocument | null = docRef.current): HistoryEntry | null => {
    if (!source) return null
    return cloneHistoryEntry(source, cloneImageData, (imageData) => imageData.data.byteLength)
  }, [])

  const commit = useCallback(
    (mutator: (d: GifDocument) => GifDocument): boolean => {
      const prev = docRef.current
      if (!prev) return false
      try {
        assertGifDocumentCanvasInvariant(prev)
        const next = mutator(prev)
        if (next === prev) return false
        assertGifDocumentCanvasInvariant(next)
        validateGifEncodeBudget(next.frames, next.width, next.height)
        const historyEntry = createHistoryEntry(
          prev,
          next,
          cloneImageData,
          (imageData) => imageData.data,
          (imageData) => imageData.data.byteLength
        )
        if (!historyEntry) return false
        pushHistory(historyEntry)
        docRef.current = next
        setDoc(next)
        return true
      } catch (error) {
        api.notify('编辑失败', error instanceof Error ? error.message : String(error))
        return false
      }
    },
    [api, pushHistory]
  )

  const updateFrame = useCallback(
    (index: number, next: GifFrame) => {
      commit((d) => {
        const frames = d.frames.map((f) => (f.id === next.id ? next : f))
        return { ...d, frames }
      })
    },
    [commit]
  )

  const undo = useCallback(() => {
    const source = docRef.current
    if (!source || undoStack.length === 0) return
    const entry = undoStack[undoStack.length - 1]
    let restored: GifDocument
    if (entry.kind === 'delta') {
      const result = applyHistoryDelta(
        source,
        entry,
        'undo',
        (imageData) => imageData.data,
        (imageData, bytes) =>
          new ImageData(new Uint8ClampedArray(bytes), imageData.width, imageData.height)
      )
      restored = { ...source, frames: result.frames }
      setRedoStack((stack) => appendHistoryEntry(stack, entry, HISTORY_LIMITS))
    } else {
      const currentEntry = snapshot(source)
      if (currentEntry) {
        setRedoStack((stack) => appendHistoryEntry(stack, currentEntry, HISTORY_LIMITS))
      }
      restored = { ...source, frames: entry.frames, width: entry.width, height: entry.height }
    }
    setUndoStack(undoStack.slice(0, -1))
    assertGifDocumentCanvasInvariant(restored)
    validateGifEncodeBudget(restored.frames, restored.width, restored.height)
    docRef.current = restored
    setDoc(restored)
    setCurrent((index) => Math.max(0, Math.min(index, entry.frames.length - 1)))
  }, [snapshot, undoStack])

  const redo = useCallback(() => {
    const source = docRef.current
    if (!source || redoStack.length === 0) return
    const entry = redoStack[redoStack.length - 1]
    let restored: GifDocument
    if (entry.kind === 'delta') {
      const result = applyHistoryDelta(
        source,
        entry,
        'redo',
        (imageData) => imageData.data,
        (imageData, bytes) =>
          new ImageData(new Uint8ClampedArray(bytes), imageData.width, imageData.height)
      )
      restored = { ...source, frames: result.frames }
      setUndoStack((stack) => appendHistoryEntry(stack, entry, HISTORY_LIMITS))
    } else {
      const currentEntry = snapshot(source)
      if (currentEntry) {
        setUndoStack((stack) => appendHistoryEntry(stack, currentEntry, HISTORY_LIMITS))
      }
      restored = { ...source, frames: entry.frames, width: entry.width, height: entry.height }
    }
    setRedoStack(redoStack.slice(0, -1))
    assertGifDocumentCanvasInvariant(restored)
    validateGifEncodeBudget(restored.frames, restored.width, restored.height)
    docRef.current = restored
    setDoc(restored)
    setCurrent((index) => Math.max(0, Math.min(index, entry.frames.length - 1)))
  }, [redoStack, snapshot])

  const layerUndo = useCallback(
    (isRedo: boolean) => {
      if (layerSession && activeLayerId) {
        if (isRedo) {
          setLayerRedoStack((stack) => {
            if (stack.length === 0) return stack
            const entry = stack[stack.length - 1]
            setLayerUndoStack((us) => [...us, cloneLayers(layerSession)])
            setLayerSession(cloneLayers(entry))
            return stack.slice(0, -1)
          })
        } else {
          setLayerUndoStack((stack) => {
            if (stack.length === 0) return stack
            const entry = stack[stack.length - 1]
            setLayerRedoStack((rs) => [...rs, cloneLayers(layerSession)])
            setLayerSession(cloneLayers(entry))
            return stack.slice(0, -1)
          })
        }
      } else {
        if (isRedo) redo()
        else undo()
      }
    },
    [layerSession, activeLayerId, undo, redo]
  )

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && (e.key.toLowerCase() === 'z' || e.key.toLowerCase() === 'y')) {
        const isRedo = e.shiftKey || e.key.toLowerCase() === 'y'
        e.preventDefault()
        layerUndo(isRedo)
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [layerUndo])

  const fitZoom = useCallback(() => {
    const container = containerRef.current
    if (!container || !doc) return
    const pad = 24
    const z = Math.min((container.clientWidth - 288 - pad * 2) / doc.width, (container.clientHeight - 40 - pad * 2) / doc.height)
    setZoom(Math.max(0.05, Math.min(4, z)))
    setZoomFit(true)
  }, [doc])

  useEffect(() => {
    if (zoomFit && doc) fitZoom()
  }, [doc, zoomFit, fitZoom])

  useEffect(() => {
    const handleResize = () => {
      if (zoomFit && doc) fitZoom()
    }
    window.addEventListener('resize', handleResize)
    return () => window.removeEventListener('resize', handleResize)
  }, [zoomFit, doc, fitZoom])

  useEffect(() => {
    setThumbCache((previous) =>
      reconcileThumbnailCache(doc?.frames ?? [], previous, frameToDataURL)
    )
  }, [doc?.frames])

  useEffect(
    () => () => {
      residueAbortRef.current?.abort()
      residueAbortRef.current = null
    },
    []
  )

  const cancelResidueTask = useCallback(() => {
    const controller = residueAbortRef.current
    residueAbortRef.current = null
    controller?.abort()
    setResidueAnalyzing(false)
  }, [])

  const handleImport = async (file: File) => {
    if (file.type !== 'image/gif' && !/\.gif$/i.test(file.name)) {
      api.notify('导入失败', '请选择 GIF 文件')
      return
    }
    cancelResidueTask()
    setResidueReport(null)
    try {
      const decoded = await decodeGifFile(file)
      fileRef.current = file
      setDoc(decoded)
      setCurrent(0)
      setPlaying(false)
      setUndoStack([])
      setRedoStack([])
      setZoomFit(true)
      setProgress(0)
      setCropMode(false)
      setCropRect(null)
      setSelectMode('none')
      setSelectRect(null)
      setWandMask(null)
      setLassoMask(null)
      setLassoPoints([])
      setLassoPreview(null)
      setLayerSession(null)
      setActiveLayerId(null)
      setBatchFrom(1)
      setBatchTo(decoded.frames.length)
    } catch (err) {
      api.notify('导入失败', String(err))
    }
  }

  const onFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (file) void handleImport(file)
    e.target.value = ''
  }

  const onDrop = (e: React.DragEvent) => {
    e.preventDefault()
    setDragOver(false)
    const file = e.dataTransfer.files?.[0]
    if (file) void handleImport(file)
  }

  const drawFrameToCanvas = useCallback(
    (target: HTMLCanvasElement, data: ImageData, scale: number) => {
      const tctx = target.getContext('2d')
      if (!tctx) return
      target.width = Math.max(1, Math.round(data.width * scale))
      target.height = Math.max(1, Math.round(data.height * scale))
      const off = document.createElement('canvas')
      off.width = data.width
      off.height = data.height
      const octx = off.getContext('2d')
      if (!octx) return
      octx.putImageData(data, 0, 0)
      tctx.imageSmoothingEnabled = false
      tctx.clearRect(0, 0, target.width, target.height)
      tctx.drawImage(off, 0, 0, target.width, target.height)
    },
    []
  )

  const getEditImage = useCallback((): ImageData | null => {
    const d = docRef.current
    if (!d) return null
    if (layerSession && activeLayerId) {
      const layer = layerSession.find((l) => l.id === activeLayerId)
      return layer?.imageData ?? null
    }
    return d.frames[currentRef.current]?.imageData ?? null
  }, [layerSession, activeLayerId])

  const pushLayerHistory = useCallback(() => {
    if (layerSession) {
      setLayerUndoStack((stack) => [...stack, cloneLayers(layerSession)].slice(-50))
      setLayerRedoStack([])
    }
  }, [layerSession])

  const commitEdit = useCallback(
    (next: ImageData) => {
      if (layerSession && activeLayerId) {
        pushLayerHistory()
        setLayerSession((prev) =>
          prev ? prev.map((l) => (l.id === activeLayerId ? { ...l, imageData: next } : l)) : prev
        )
      } else {
        const f = docRef.current?.frames[currentRef.current]
        if (f) updateFrame(currentRef.current, { ...f, imageData: next })
      }
    },
    [layerSession, activeLayerId, pushLayerHistory, updateFrame]
  )

  const applyFilterPreview = useCallback(
    (source: ImageData) =>
      tab === 'filter' && hasPendingFilters(filterValues)
        ? applyFilterValues(source, filterValues, transforms)
        : source,
    [filterValues, tab]
  )

  const redrawMainCanvas = useCallback(() => {
    const canvas = canvasRef.current
    const d = docRef.current
    if (!canvas || !d) return
    const source = getEditImage()
    const data = source ? applyFilterPreview(source) : null
    if (data) drawFrameToCanvas(canvas, data, zoomRef.current)
  }, [applyFilterPreview, drawFrameToCanvas, getEditImage])

  const redrawLassoOverlay = useCallback(() => {
    const canvas = lassoCanvasRef.current
    const d = docRef.current
    if (!canvas || !d) {
      if (canvas) {
        canvas.width = 0
        canvas.height = 0
      }
      return
    }
    const scale = zoomRef.current
    canvas.width = Math.max(1, Math.round(d.width * scale))
    canvas.height = Math.max(1, Math.round(d.height * scale))
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    ctx.clearRect(0, 0, canvas.width, canvas.height)
    const pts = lassoPointsRef.current
    if (pts.length < 2) return
    const preview = lassoPreviewRef.current
    ctx.strokeStyle = 'var(--ob-color-primary, #2563eb)'
    ctx.lineWidth = 2
    ctx.setLineDash([6, 4])
    ctx.beginPath()
    ctx.moveTo(pts[0].x * scale, pts[0].y * scale)
    for (let i = 1; i < pts.length; i++) ctx.lineTo(pts[i].x * scale, pts[i].y * scale)
    if (preview) ctx.lineTo(preview.x * scale, preview.y * scale)
    else if (lassoClosedRef.current && pts.length >= 3) ctx.lineTo(pts[0].x * scale, pts[0].y * scale)
    ctx.stroke()
    ctx.setLineDash([])
    // vertices
    ctx.fillStyle = 'var(--ob-color-primary, #2563eb)'
    for (const p of pts) {
      ctx.beginPath()
      ctx.arc(p.x * scale, p.y * scale, 3, 0, Math.PI * 2)
      ctx.fill()
    }
  }, [])

  const zoomRef = useRef(1)
  zoomRef.current = zoom
  const lassoPointsRef = useRef<{ x: number; y: number }[]>([])
  const lassoPreviewRef = useRef<{ x: number; y: number } | null>(null)
  const lassoClosedRef = useRef(false)
  lassoPointsRef.current = lassoPoints
  lassoPreviewRef.current = lassoPreview
  lassoClosedRef.current = lassoMask !== null

  useEffect(() => {
    redrawMainCanvas()
    redrawLassoOverlay()
  }, [doc, current, zoom, redrawMainCanvas, redrawLassoOverlay, lassoPoints, lassoPreview, lassoMask, layerSession, activeLayerId])

  const startEditCanvas = useCallback(() => {
    const d = docRef.current
    const edit = editCanvasRef.current
    const data = getEditImage()
    if (!d || !data || !edit) return
    edit.width = d.width
    edit.height = d.height
    const octx = edit.getContext('2d')
    if (!octx) return
    octx.putImageData(data, 0, 0)
  }, [getEditImage])

  const syncEditToFrame = useCallback(() => {
    const d = docRef.current
    const edit = editCanvasRef.current
    if (!d || !edit) return
    const octx = edit.getContext('2d')
    if (!octx) return
    const imageData = octx.getImageData(0, 0, d.width, d.height)
    commitEdit(imageData)
  }, [commitEdit])

  const canvasToDocPoint = (clientX: number, clientY: number): { x: number; y: number } | null => {
    const canvas = canvasRef.current
    const d = docRef.current
    if (!canvas || !d) return null
    const rect = canvas.getBoundingClientRect()
    const x = ((clientX - rect.left) / rect.width) * d.width
    const y = ((clientY - rect.top) / rect.height) * d.height
    return { x: Math.max(0, Math.min(d.width - 1, Math.floor(x))), y: Math.max(0, Math.min(d.height - 1, Math.floor(y))) }
  }

  const handleMouseDown = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!doc || !frame || playing) return
    const pt = canvasToDocPoint(e.clientX, e.clientY)
    if (!pt) return
    if (cropMode) {
      cropStartRef.current = pt
      setCropRect({ x: pt.x, y: pt.y, w: 0, h: 0 })
      return
    }
    if (selectMode === 'rect') {
      selectStartRef.current = pt
      setSelectRect({ x: pt.x, y: pt.y, w: 0, h: 0 })
      setWandMask(null)
      return
    }
    if (selectMode === 'wand') {
      const d = docRef.current
      const f = d?.frames[currentRef.current]
      if (d && f) {
        const mask = floodFill(f.imageData, pt.x, pt.y, wandTolerance)
        setWandMask(mask)
      }
      return
    }
    if (selectMode === 'lasso') {
      if (lassoMask) return
      if (lassoMode === 'free') {
        lassoFreeRef.current = true
        setLassoPoints([pt])
        return
      }
      // poly: clicking near the first point closes the lasso
      if (lassoPoints.length > 0) {
        const first = lassoPoints[0]
        const dist = Math.hypot(pt.x - first.x, pt.y - first.y)
        if (dist <= 6) {
          closeLasso()
          return
        }
      }
      setLassoPoints((prev) => [...prev, pt])
      return
    }
    if (tab === 'color') {
      const data = getEditImage()
      if (!data) return
      const i = (pt.y * data.width + pt.x) * 4
      setTargetColor([data.data[i], data.data[i + 1], data.data[i + 2]])
      return
    }
    if (tab === 'draw') {
      if (drawTool === 'text') {
        setTextEntryMode(true)
        setTextPos(pt)
        return
      }
      startEditCanvas()
      drawingRef.current = true
      lastPosRef.current = pt
      paintStroke(pt, pt)
    }
  }

  const paintStroke = (from: { x: number; y: number }, to: { x: number; y: number }) => {
    const edit = editCanvasRef.current
    if (!edit) return
    const octx = edit.getContext('2d')
    if (!octx) return
    octx.save()
    if (drawTool === 'eraser') {
      octx.globalCompositeOperation = 'destination-out'
      octx.strokeStyle = '#000'
      octx.lineWidth = eraserSize * 2
    } else {
      octx.globalCompositeOperation = 'source-over'
      octx.strokeStyle = brushColor
      octx.lineWidth = brushSize * 2
    }
    octx.lineCap = 'round'
    octx.lineJoin = 'round'
    octx.beginPath()
    octx.moveTo(from.x, from.y)
    octx.lineTo(to.x, to.y)
    octx.stroke()
    octx.restore()
    redrawMainCanvas()
  }

  const handleMouseMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (cropMode && cropStartRef.current) {
      const pt = canvasToDocPoint(e.clientX, e.clientY)
      if (!pt) return
      const sx = Math.min(cropStartRef.current.x, pt.x)
      const sy = Math.min(cropStartRef.current.y, pt.y)
      const ex = Math.max(cropStartRef.current.x, pt.x)
      const ey = Math.max(cropStartRef.current.y, pt.y)
      setCropRect({ x: sx, y: sy, w: ex - sx, h: ey - sy })
      return
    }
    if (selectMode === 'rect' && selectStartRef.current) {
      const pt = canvasToDocPoint(e.clientX, e.clientY)
      if (!pt) return
      const sx = Math.min(selectStartRef.current.x, pt.x)
      const sy = Math.min(selectStartRef.current.y, pt.y)
      const ex = Math.max(selectStartRef.current.x, pt.x)
      const ey = Math.max(selectStartRef.current.y, pt.y)
      setSelectRect({ x: sx, y: sy, w: ex - sx, h: ey - sy })
      return
    }
    if (selectMode === 'lasso') {
      const pt = canvasToDocPoint(e.clientX, e.clientY)
      if (!pt) return
      if (lassoFreeRef.current && !lassoMask) {
        setLassoPoints((prev) => {
          const last = prev[prev.length - 1]
          if (last && Math.hypot(pt.x - last.x, pt.y - last.y) >= 4) {
            return [...prev, pt]
          }
          return prev
        })
      } else if (!lassoFreeRef.current && !lassoMask) {
        setLassoPreview(pt)
      }
      return
    }
    if (!drawingRef.current || !lastPosRef.current) return
    const pt = canvasToDocPoint(e.clientX, e.clientY)
    if (!pt) return
    paintStroke(lastPosRef.current, pt)
    lastPosRef.current = pt
  }

  const handleMouseUp = () => {
    if (drawingRef.current) {
      drawingRef.current = false
      lastPosRef.current = null
      syncEditToFrame()
    }
    if (selectStartRef.current) {
      selectStartRef.current = null
    }
    if (selectMode === 'lasso' && lassoFreeRef.current && !lassoMask) {
      lassoFreeRef.current = false
      setLassoPreview(null)
      closeLasso()
    }
  }

  const handleMouseLeave = () => {
    if (drawingRef.current) {
      drawingRef.current = false
      lastPosRef.current = null
      syncEditToFrame()
    }
  }

  const confirmText = () => {
    if (!textContent.trim() || !textPos || !frame) {
      setTextEntryMode(false)
      setTextPos(null)
      return
    }
    startEditCanvas()
    const edit = editCanvasRef.current
    const octx = edit?.getContext('2d')
    if (edit && octx) {
      octx.save()
      octx.font = `${textFontSize}px sans-serif`
      octx.fillStyle = textColor
      octx.textBaseline = 'top'
      octx.fillText(textContent, textPos.x, textPos.y)
      octx.restore()
    }
    syncEditToFrame()
    setTextEntryMode(false)
    setTextPos(null)
    setTextContent('')
  }

  const applyToFrame = (transform: (data: ImageData) => ImageData, index: number) => {
    const d = docRef.current
    if (!d) return
    if (layerSession && activeLayerId) {
      const layer = layerSession.find((l) => l.id === activeLayerId)
      if (layer) {
        const next = transform(layer.imageData)
        if (next.width !== d.width || next.height !== d.height) {
          api.notify('编辑失败', '图层操作不能改变文档画布尺寸')
          return
        }
        pushLayerHistory()
        setLayerSession((prev) =>
          prev ? prev.map((l) => (l.id === activeLayerId ? { ...l, imageData: next } : l)) : prev
        )
      }
      return
    }
    const f = d.frames[index]
    if (!f) return
    const next = transform(f.imageData)
    if (next.width !== d.width || next.height !== d.height) {
      api.notify('编辑失败', '单帧操作不能改变文档画布尺寸')
      return
    }
    updateFrame(index, { ...f, imageData: next })
  }

  const resetCanvasScopedState = () => {
    setCropMode(false)
    setCropRect(null)
    setSelectMode('none')
    setSelectRect(null)
    setWandMask(null)
    setLassoMask(null)
    setLassoPoints([])
    setLassoPreview(null)
    setLayerSession(null)
    setActiveLayerId(null)
    setLayerUndoStack([])
    setLayerRedoStack([])
  }

  const applyGeometry = (transform: (data: ImageData) => ImageData) => {
    if (!frame) return
    if (layerSession) {
      api.notify('无法改变画布', '请先合并或取消当前图层会话')
      return
    }
    if (commit((d) => applyCanvasTransform(d, transform))) resetCanvasScopedState()
  }

  const confirmCrop = () => {
    if (!cropRect || cropRect.w < 2 || cropRect.h < 2) {
      setCropMode(false)
      setCropRect(null)
      return
    }
    applyGeometry(transforms.crop({ x: cropRect.x, y: cropRect.y, width: cropRect.w, height: cropRect.h }))
    setCropMode(false)
    setCropRect(null)
  }

  const hexToRgb = (hex: string): Rgb => {
    const m = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex)
    return m ? [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16)] : [0, 0, 0]
  }

  const applyReplaceColor = (all: boolean) => {
    const d = docRef.current
    if (!d) return
    const target = targetColor
    const replacement = hexToRgb(replaceColor)
    if (all) {
      commit((doc_) => {
        const frames = doc_.frames.map((f) => ({ ...f, imageData: transforms.replaceColor(target, replacement, tolerance)(f.imageData) }))
        return { ...doc_, frames }
      })
    } else {
      applyToFrame((data) => transforms.replaceColor(target, replacement, tolerance)(data), currentRef.current)
    }
  }

  const removeFrame = () => {
    if (!doc) return
    if (doc.frames.length <= 1) {
      api.notify('无法删除', '至少保留一帧')
      return
    }
    commit((d) => {
      const frames = d.frames.filter((_, i) => i !== currentRef.current)
      return { ...d, frames }
    })
    setCurrent((c) => Math.min(c, (doc.frames.length - 1) - 1))
  }

  const moveFrame = (dir: -1 | 1) => {
    if (!doc) return
    const c = currentRef.current
    const target = c + dir
    if (target < 0 || target >= doc.frames.length) return
    commit((d) => {
      const frames = [...d.frames]
      const tmp = frames[c]
      frames[c] = frames[target]
      frames[target] = tmp
      return { ...d, frames }
    })
    setCurrent(target)
  }

  const addBlankFrame = () => {
    if (!doc) return
    commit((d) => {
      const blank = createBlankFrame(d.width, d.height, 100)
      const frames = [...d.frames]
      const idx = currentRef.current + 1
      frames.splice(idx, 0, blank)
      return { ...d, frames }
    })
    setCurrent((c) => c + 1)
  }

  const setFrameDelay = (index: number, delay: number) => {
    commit((d) => {
      const frames = d.frames.map((f, i) => (i === index ? { ...f, delay } : f))
      return { ...d, frames }
    })
  }

  const setBatchFrameDelay = () => {
    if (!doc) return
    const last = doc.frames.length - 1
    const from = Math.max(0, Math.min(batchFrom - 1, last))
    const to = Math.max(from, Math.min(batchTo - 1, last))
    const delay = Math.max(20, Math.round(batchDelay))
    commit((d) => ({
      ...d,
      frames: d.frames.map((f, i) => (i >= from && i <= to ? { ...f, delay } : f)),
    }))
  }

  const setAllFrameDelay = () => {
    if (!doc) return
    const delay = Math.max(20, Math.round(batchDelay))
    commit((d) => ({ ...d, frames: d.frames.map((f) => ({ ...f, delay })) }))
  }

  const resetFilterValue = (filter: keyof FilterValues) => {
    if (filter === 'brightness') setBrightnessVal(0)
    else if (filter === 'contrast') setContrastVal(0)
    else setSaturationVal(0)
  }

  const applyPendingFilter = (filter: keyof FilterValues) => {
    const amount = filterValues[filter]
    if (amount === 0) return
    applyToFrame(transforms[filter](amount), currentRef.current)
    resetFilterValue(filter)
  }

  const resetFilterValues = () => {
    setBrightnessVal(0)
    setContrastVal(0)
    setSaturationVal(0)
  }

  const applyFilterAll = () => {
    const d = docRef.current
    if (!d || !hasPendingFilters(filterValues)) return
    commit((doc_) => {
      const frames = doc_.frames.map((f) => ({
        ...f,
        imageData: applyFilterValues(f.imageData, filterValues, transforms),
      }))
      return { ...doc_, frames }
    })
    resetFilterValues()
  }

  const applyRemoveBackground = (all: boolean) => {
    const d = docRef.current
    if (!d) return
    const tol = smartTolerance
    if (all) {
      commit((doc_) => ({
        ...doc_,
        frames: doc_.frames.map((f) => ({ ...f, imageData: removeBackground(f.imageData, tol) })),
      }))
    } else {
      applyToFrame((data) => removeBackground(data, tol), currentRef.current)
    }
  }

  const applyAutoCrop = (all: boolean) => {
    const d = docRef.current
    if (!d) return
    if (layerSession) {
      api.notify('无法改变画布', '请先合并或取消当前图层会话')
      return
    }
    const tol = smartTolerance
    const changed = commit((document_) =>
      all
        ? cropCanvasToUnionBounds(document_, tol)
        : cropCanvasToFrameBounds(document_, currentRef.current, tol)
    )
    if (changed) resetCanvasScopedState()
  }

  const applySplitGrid = () => {
    const d = docRef.current
    const f = d?.frames[currentRef.current]
    if (!d || !f) return
    const parts = splitGrid(f.imageData, gridRows, gridCols)
    if (parts.length <= 1) return
    const newFrames = parts.map((imageData) => ({
      id: crypto.randomUUID(),
      imageData,
      delay: f.delay,
    }))
    commit((doc_) => {
      const frames = [...doc_.frames]
      frames.splice(currentRef.current, 1, ...newFrames)
      return { ...doc_, frames }
    })
  }

  const applySplitColor = () => {
    const d = docRef.current
    const f = d?.frames[currentRef.current]
    if (!d || !f) return
    const layers = splitColorLayers(f.imageData, { tolerance: colorLayerTol, maxLayers: colorLayers })
    if (layers.length <= 1) return
    const newFrames = layers.map((imageData) => ({
      id: crypto.randomUUID(),
      imageData,
      delay: f.delay,
    }))
    commit((doc_) => {
      const frames = [...doc_.frames]
      frames.splice(currentRef.current, 1, ...newFrames)
      return { ...doc_, frames }
    })
  }

  const startLayerSeparation = () => {
    const d = docRef.current
    const f = d?.frames[currentRef.current]
    if (!d || !f || busy) return
    if (splitMode === 'lasso') {
      setSelectMode('lasso')
      clearLasso()
      return
    }
    setBusy(true)
    try {
      let layers: ImageData[] = []
      if (splitMode === 'subject') {
        const [fg, bg] = splitSubjectBackground(f.imageData, smartTolerance)
        layers = [fg, bg]
      } else if (splitMode === 'objects') {
        layers = splitConnectedObjects(f.imageData, smartTolerance)
      } else {
        layers = splitColorClusters(f.imageData, clusterCount)
      }
      if (layers.length < 2) {
        api.notify('分离失败', '未识别出足够多的独立区域')
        return
      }
      setLayerSession(
        layers.map((imageData) => ({ id: crypto.randomUUID(), imageData, visible: true }))
      )
    } catch (err) {
      api.notify('分离失败', String(err))
    } finally {
      setBusy(false)
    }
  }

  const separateByLasso = () => {
    const d = docRef.current
    const f = d?.frames[currentRef.current]
    if (!d || !f || !lassoMask) return
    const [inner, outer] = splitByMask(f.imageData, lassoMask)
    setLayerSession((prev) => [
      ...(prev ?? []),
      { id: crypto.randomUUID(), imageData: inner, visible: true },
      { id: crypto.randomUUID(), imageData: outer, visible: true },
    ])
    clearLasso()
    setSelectMode('none')
  }

  const toggleLayer = (id: string) => {
    setLayerSession((prev) =>
      prev ? prev.map((l) => (l.id === id ? { ...l, visible: !l.visible } : l)) : prev
    )
  }

  const removeLayer = (id: string) => {
    setLayerSession((prev) => (prev ? prev.filter((l) => l.id !== id) : prev))
  }

  const moveLayer = (id: string, dir: -1 | 1) => {
    setLayerSession((prev) => {
      if (!prev) return prev
      const idx = prev.findIndex((l) => l.id === id)
      const target = idx + dir
      if (idx < 0 || target < 0 || target >= prev.length) return prev
      const next = [...prev]
      const tmp = next[idx]
      next[idx] = next[target]
      next[target] = tmp
      return next
    })
  }

  const mergeLayerSession = () => {
    const d = docRef.current
    if (!d || !layerSession || layerSession.length === 0) return
    const visible = layerSession.filter((l) => l.visible)
    if (visible.length === 0) {
      api.notify('合并失败', '至少保留一个可见图层')
      return
    }
    if (
      visible.some(
        (layer) => layer.imageData.width !== d.width || layer.imageData.height !== d.height
      )
    ) {
      api.notify('合并失败', '图层尺寸与当前文档画布不一致，请取消本次图层会话')
      return
    }
    const merged = mergeLayers(visible.map((l) => l.imageData), d.width, d.height)
    commit((doc_) => ({
      ...doc_,
      frames: doc_.frames.map((fr, i) =>
        i === currentRef.current ? { ...fr, imageData: merged } : fr
      ),
    }))
    setLayerSession(null)
    setActiveLayerId(null)
  }

  const clearLayerSession = () => {
    setLayerSession(null)
    setActiveLayerId(null)
  }

  const handleAnalyzeResidue = async () => {
    const file = fileRef.current
    if (!file || residueAbortRef.current) return
    const controller = new AbortController()
    residueAbortRef.current = controller
    setResidueAnalyzing(true)
    setResidueReport(null)
    try {
      const report = await runResidueWorker(file, { type: 'analyze' }, controller.signal)
      if (residueAbortRef.current !== controller) return
      if (!report) {
        api.notify('未检测到残留', '未发现明显的叠加残留帧')
      } else {
        setResidueReport(report)
      }
    } catch (err) {
      if (!isResidueWorkerAbortError(err)) api.notify('分析失败', String(err))
    } finally {
      if (residueAbortRef.current === controller) {
        residueAbortRef.current = null
        setResidueAnalyzing(false)
      }
    }
  }

  const handleApplyResidueFix = async () => {
    const file = fileRef.current
    const report = residueReport
    if (!file || !report || residueAbortRef.current) return
    const controller = new AbortController()
    residueAbortRef.current = controller
    setResidueAnalyzing(true)
    try {
      const fixed = await runResidueWorker(
        file,
        { type: 'fix', pollutedFrame: report.pollutedFrame },
        controller.signal
      )
      if (residueAbortRef.current !== controller) return
      const firstFrame = fixed[0]
      if (!firstFrame) throw new Error('修复结果不包含任何帧')
      const changed = commit((d) => ({
        ...d,
        width: firstFrame.imageData.width,
        height: firstFrame.imageData.height,
        frames: fixed,
      }))
      if (!changed) return
      resetCanvasScopedState()
      setResidueReport(null)
      setCurrent(0)
      api.notify('修复完成', `已移除叠加残留（${fixed.length} 帧）`)
    } catch (err) {
      if (!isResidueWorkerAbortError(err)) api.notify('修复失败', String(err))
    } finally {
      if (residueAbortRef.current === controller) {
        residueAbortRef.current = null
        setResidueAnalyzing(false)
      }
    }
  }

  const applyRectDelete = (all: boolean) => {
    const d = docRef.current
    if (!d || !selectRect) return
    const rect = { x: selectRect.x, y: selectRect.y, width: selectRect.w, height: selectRect.h }
    if (all) {
      commit((doc_) => ({
        ...doc_,
        frames: doc_.frames.map((f) => ({ ...f, imageData: deleteRectRegion(f.imageData, rect) })),
      }))
    } else {
      applyToFrame((data) => deleteRectRegion(data, rect), currentRef.current)
    }
    setSelectRect(null)
    setSelectMode('none')
  }

  const applyWandDelete = (all: boolean) => {
    const d = docRef.current
    if (!d || !wandMask) return
    const mask = wandMask
    if (all) {
      commit((doc_) => ({
        ...doc_,
        frames: doc_.frames.map((f) => ({ ...f, imageData: deleteRegionByMask(f.imageData, mask) })),
      }))
    } else {
      applyToFrame((data) => deleteRegionByMask(data, mask), currentRef.current)
    }
    setWandMask(null)
    setSelectMode('none')
  }

  const closeLasso = () => {
    const d = docRef.current
    const f = d?.frames[currentRef.current]
    if (!d || !f) return
    const pts = lassoPointsRef.current
    if (pts.length < 3) {
      setLassoPoints([])
      setLassoPreview(null)
      return
    }
    const mask = polygonMask(f.imageData, pts, { edgeWidth: lassoEdgeWidth })
    setLassoMask(mask)
    setLassoPreview(null)
  }

  const clearLasso = () => {
    setLassoPoints([])
    setLassoPreview(null)
    setLassoMask(null)
  }

  const applyLassoDelete = (all: boolean) => {
    const d = docRef.current
    if (!d || !lassoMask) return
    const mask = lassoMask
    if (all) {
      commit((doc_) => ({
        ...doc_,
        frames: doc_.frames.map((f) => ({ ...f, imageData: deleteRegionByMask(f.imageData, mask) })),
      }))
    } else {
      applyToFrame((data) => deleteRegionByMask(data, mask), currentRef.current)
    }
    clearLasso()
    setSelectMode('none')
  }

  const previewLoop = useCallback(() => {
    const d = docRef.current
    const canvas = previewCanvasRef.current
    if (!d || !canvas) return
    const source = d.frames[currentRef.current]?.imageData
    if (source) drawFrameToCanvas(canvas, applyFilterPreview(source), 1)
  }, [applyFilterPreview, drawFrameToCanvas])

  useEffect(() => {
    previewLoop()
  }, [doc, current, previewLoop])

  const togglePlay = () => {
    if (!doc || doc.frames.length === 0) return
    if (playingRef.current) {
      stopPlay()
    } else {
      setPlaying(true)
      playingRef.current = true
      scheduleNext()
    }
  }

  const stopPlay = useCallback(() => {
    setPlaying(false)
    playingRef.current = false
    if (playTimerRef.current !== null) {
      window.clearTimeout(playTimerRef.current)
      playTimerRef.current = null
    }
  }, [])

  const scheduleNext = useCallback(() => {
    const d = docRef.current
    if (!d || !playingRef.current) return
    const f = d.frames[currentRef.current]
    const delay = Math.max(20, f?.delay ?? 100)
    playTimerRef.current = window.setTimeout(() => {
      setCurrent((c) => {
        const next = (c + 1) % (docRef.current?.frames.length ?? 1)
        return next
      })
      scheduleNext()
    }, delay)
  }, [])

  useEffect(() => () => stopPlay(), [stopPlay])

  const stepFrame = (dir: -1 | 1) => {
    if (!doc) return
    stopPlay()
    setCurrent((c) => (c + dir + doc.frames.length) % doc.frames.length)
  }

  const handleExport = async () => {
    const d = docRef.current
    if (!d || d.frames.length === 0 || exporting) return
    setExporting(true)
    setProgress(0)
    try {
      let frames = d.frames.map((f) => ({ ...f, imageData: f.imageData }))
      if (exportScale !== 100) {
        const factor = exportScale / 100
        frames = frames.map((f) => ({ ...f, imageData: transforms.scale(factor)(f.imageData) }))
      }
      const opts: { repeat: number; delay?: number } = { repeat }
      if (delayMode === 'unified') opts.delay = Math.max(20, unifiedDelay)
      const blob = await encodeGif(frames, opts, (p) => setProgress(Math.round(p)))
      const name = (d.sourceName.replace(/\.gif$/i, '') || 'edited') + '_编辑.gif'
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = name
      a.click()
      setTimeout(() => URL.revokeObjectURL(url), 5000)
      api.notify('导出完成', name)
    } catch (err) {
      api.notify('导出失败', String(err))
    } finally {
      setExporting(false)
      setProgress(0)
    }
  }

  return (
    <div className="ge-root">
      <input ref={fileInputRef} type="file" accept="image/gif" style={{ display: 'none' }} onChange={onFileChange} />

      <div className="ge-topbar">
        <button className="ge-btn ge-btn-primary" onClick={() => fileInputRef.current?.click()} disabled={exporting}>
          导入 GIF
        </button>
        <div className="ge-file-info">
          {doc ? (
            <>
              <span className="ge-file-name">{doc.sourceName}</span>
              <span className="ge-file-meta">
                帧 {current + 1}/{doc.frames.length} · {doc.width}×{doc.height}
              </span>
            </>
          ) : (
            <span className="ge-file-meta">未导入文件</span>
          )}
        </div>
        <div className="ge-topbar-spacer" />
        <button className="ge-btn" onClick={undo} disabled={undoStack.length === 0 || !doc}>
          撤销
        </button>
        <button className="ge-btn" onClick={redo} disabled={redoStack.length === 0 || !doc}>
          重做
        </button>
        <button className="ge-btn" onClick={() => setShowExportSettings((v) => !v)} disabled={!doc}>
          导出设置
        </button>
        <button className="ge-btn ge-btn-primary" onClick={() => void handleExport()} disabled={!doc || exporting}>
          {exporting ? '合成中...' : '导出 GIF'}
        </button>
      </div>

      {showExportSettings && (
        <div className="ge-export-bar">
          <label className="ge-field">
            循环次数
            <select className="ge-select" value={repeat} onChange={(e) => setRepeat(Number(e.target.value))}>
              <option value={0}>无限循环</option>
              <option value={1}>1 次</option>
              <option value={2}>2 次</option>
              <option value={3}>3 次</option>
              <option value={5}>5 次</option>
            </select>
          </label>
          <label className="ge-field">
            延迟模式
            <select className="ge-select" value={delayMode} onChange={(e) => setDelayMode(e.target.value as 'unified' | 'perframe')}>
              <option value="unified">统一</option>
              <option value="perframe">逐帧</option>
            </select>
          </label>
          {delayMode === 'unified' && (
            <label className="ge-field">
              延迟(ms)
              <input className="ge-input ge-input-num" type="number" min={20} max={10000} step={10} value={unifiedDelay} onChange={(e) => setUnifiedDelay(Number(e.target.value))} />
            </label>
          )}
          <label className="ge-field">
            导出尺寸
            <select className="ge-select" value={exportScale} onChange={(e) => setExportScale(Number(e.target.value))}>
              <option value={50}>50%</option>
              <option value={75}>75%</option>
              <option value={100}>100%</option>
            </select>
          </label>
          {exporting && (
            <div className="ge-progress-wrap">
              <div className="ge-progress-track">
                <div className="ge-progress-fill" style={{ width: `${progress}%` }} />
              </div>
              <span className="ge-progress-text">{progress}%</span>
            </div>
          )}
        </div>
      )}

      <div className="ge-previewbar">
        <button className="ge-btn ge-play-btn" onClick={togglePlay} disabled={!doc}>
          {playing ? '暂停' : '播放'}
        </button>
        <button className="ge-btn" onClick={() => stepFrame(-1)} disabled={!doc}>
          ◀ 上一帧
        </button>
        <button className="ge-btn" onClick={() => stepFrame(1)} disabled={!doc}>
          下一帧 ▶
        </button>
        <span className="ge-preview-info">
          {doc
            ? `帧 ${current + 1}/${doc.frames.length} · 延迟 ${frame?.delay ?? 0}ms · 总时长 ${(totalDuration / 1000).toFixed(1)}s`
            : '导入 GIF 后开始编辑'}
        </span>
        <div className="ge-preview-win">
          <canvas ref={previewCanvasRef} className="ge-preview-canvas" />
        </div>
      </div>

      <div className="ge-main">
        <div
          className="ge-canvas-area"
          ref={containerRef}
          onDragOver={(e) => {
            e.preventDefault()
            setDragOver(true)
          }}
          onDragLeave={() => setDragOver(false)}
          onDrop={onDrop}
        >
          {!doc ? (
            <div className={`ge-empty ${dragOver ? 'ge-empty-over' : ''}`} onClick={() => fileInputRef.current?.click()}>
              <div className="ge-empty-icon">🎞️</div>
              <div className="ge-empty-title">点击导入或拖拽 GIF 到此处</div>
              <div className="ge-empty-sub">支持 .gif · 导入后自动拆分为帧</div>
            </div>
          ) : (
            <>
              <div className="ge-canvas-scroll">
                <div
                  className="ge-checkerboard"
                  style={{ width: doc.width * zoom, height: doc.height * zoom }}
                >
                  <canvas ref={canvasRef} className="ge-frame-canvas" onMouseDown={handleMouseDown} onMouseMove={handleMouseMove} onMouseUp={handleMouseUp} onMouseLeave={handleMouseLeave} onDoubleClick={() => { if (selectMode === 'lasso' && lassoMode === 'poly' && !lassoMask) closeLasso() }} style={{ cursor: cropMode ? 'crosshair' : selectMode !== 'none' ? 'crosshair' : tab === 'draw' ? 'crosshair' : tab === 'color' ? 'copy' : 'default' }} />
                  {cropRect && cropMode && (
                    <div
                      className="ge-crop-overlay"
                      style={{
                        left: cropRect.x * zoom,
                        top: cropRect.y * zoom,
                        width: cropRect.w * zoom,
                        height: cropRect.h * zoom,
                      }}
                    />
                  )}
                  {selectRect && selectMode === 'rect' && (
                    <div
                      className="ge-select-overlay"
                      style={{
                        left: selectRect.x * zoom,
                        top: selectRect.y * zoom,
                        width: selectRect.w * zoom,
                        height: selectRect.h * zoom,
                      }}
                    />
                  )}
                  {wandMask && selectMode === 'wand' && (
                    <div className="ge-wand-mask-hint">已选中连通区域，点击「删除选区」</div>
                  )}
                  {selectMode === 'lasso' && (
                    <canvas ref={lassoCanvasRef} className="ge-lasso-canvas" />
                  )}
                </div>
              </div>
              <div className="ge-zoombar">
                <button className="ge-btn ge-btn-sm" onClick={() => { setZoom((z) => Math.max(0.05, z / 1.25)); setZoomFit(false) }}>
                  −
                </button>
                <span className="ge-zoom-label">{Math.round(zoom * 100)}%</span>
                <button className="ge-btn ge-btn-sm" onClick={() => { setZoom((z) => Math.min(4, z * 1.25)); setZoomFit(false) }}>
                  +
                </button>
                <button className="ge-btn ge-btn-sm" onClick={fitZoom}>
                  适应窗口
                </button>
              </div>
              {textEntryMode && textPos && (
                <div className="ge-text-entry" style={{ left: textPos.x * zoom, top: textPos.y * zoom }}>
                  <input
                    autoFocus
                    className="ge-input"
                    placeholder="输入文字，回车确认"
                    value={textContent}
                    onChange={(e) => setTextContent(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') confirmText()
                      if (e.key === 'Escape') setTextEntryMode(false)
                    }}
                  />
                  <div className="ge-text-entry-actions">
                    <button className="ge-btn ge-btn-sm" onClick={confirmText}>
                      确认
                    </button>
                    <button className="ge-btn ge-btn-sm" onClick={() => setTextEntryMode(false)}>
                      取消
                    </button>
                  </div>
                </div>
              )}
            </>
          )}
        </div>

        <div className="ge-toolpanel">
          <div className="ge-tabs">
            {(['geometry', 'draw', 'filter', 'color', 'smart', 'select'] as ToolTab[]).map((t) => (
              <button key={t} className={`ge-tab ${tab === t ? 'ge-tab-active' : ''}`} onClick={() => setTab(t)} disabled={!doc}>
                {t === 'geometry' ? '几何' : t === 'draw' ? '绘制' : t === 'filter' ? '滤镜' : t === 'color' ? '调色' : t === 'smart' ? '智能' : '选区'}
              </button>
            ))}
          </div>

          <div className="ge-tab-content">
            {tab === 'geometry' && (
              <div className="ge-tool-section">
                <div className="ge-tool-row">
                  <button className="ge-btn" onClick={() => applyGeometry(transforms.rotate90)} disabled={!frame}>
                    ↻ 旋转90°
                  </button>
                  <button className="ge-btn" onClick={() => applyGeometry(transforms.rotate270)} disabled={!frame}>
                    ↺ 旋转90°
                  </button>
                </div>
                <div className="ge-tool-row">
                  <button className="ge-btn" onClick={() => applyGeometry(transforms.flipHorizontal)} disabled={!frame}>
                    ↔ 水平镜像
                  </button>
                  <button className="ge-btn" onClick={() => applyGeometry(transforms.flipVertical)} disabled={!frame}>
                    ↕ 垂直镜像
                  </button>
                </div>
                <div className="ge-tool-row">
                  <button className={`ge-btn ${cropMode ? 'ge-btn-active' : ''}`} onClick={() => setCropMode((v) => !v)} disabled={!frame}>
                    裁剪模式
                  </button>
                  {cropMode && (
                    <button className="ge-btn" onClick={confirmCrop}>
                      确认裁剪
                    </button>
                  )}
                </div>
                {cropMode && cropRect && (
                  <div className="ge-crop-info">
                    选中 {cropRect.w}×{cropRect.h} 区域
                  </div>
                )}
                <div className="ge-hint">画布旋转、镜像和裁剪会同步作用于全部帧</div>
              </div>
            )}

            {tab === 'draw' && (
              <div className="ge-tool-section">
                <div className="ge-tool-row">
                  {(['brush', 'eraser', 'text'] as DrawTool[]).map((t) => (
                    <button key={t} className={`ge-btn ${drawTool === t ? 'ge-btn-active' : ''}`} onClick={() => setDrawTool(t)}>
                      {t === 'brush' ? '画笔' : t === 'eraser' ? '橡皮擦' : '文字'}
                    </button>
                  ))}
                </div>
                {drawTool === 'brush' && (
                  <>
                    <label className="ge-field">
                      颜色
                      <input type="color" className="ge-color" value={brushColor} onChange={(e) => setBrushColor(e.target.value)} />
                    </label>
                    <div className="ge-preset-row">
                      {[0.5, 1, 2, 3, 6, 12].map((p) => (
                        <button key={p} className={`ge-btn ge-btn-sm ${brushSize === p ? 'ge-btn-active' : ''}`} onClick={() => setBrushSize(p)}>
                          {formatSize(p)}
                        </button>
                      ))}
                    </div>
                    <label className="ge-field">
                      粗细 {formatSize(brushSize)} · 实际 {formatSize(brushSize * 2)}px
                      <input type="range" min={0.5} max={40} step={0.5} value={brushSize} onChange={(e) => setBrushSize(Number(e.target.value))} />
                    </label>
                  </>
                )}
                {drawTool === 'eraser' && (
                  <>
                    <div className="ge-preset-row">
                      {[1, 2, 4, 8, 16, 32].map((p) => (
                        <button key={p} className={`ge-btn ge-btn-sm ${eraserSize === p ? 'ge-btn-active' : ''}`} onClick={() => setEraserSize(p)}>
                          {formatSize(p)}
                        </button>
                      ))}
                    </div>
                    <label className="ge-field">
                      粗细 {formatSize(eraserSize)} · 实际 {formatSize(eraserSize * 2)}px
                      <input type="range" min={0.5} max={80} step={0.5} value={eraserSize} onChange={(e) => setEraserSize(Number(e.target.value))} />
                    </label>
                  </>
                )}
                {drawTool === 'text' && (
                  <>
                    <label className="ge-field">
                      字号 {textFontSize}
                      <input type="range" min={10} max={120} value={textFontSize} onChange={(e) => setTextFontSize(Number(e.target.value))} />
                    </label>
                    <label className="ge-field">
                      颜色
                      <input type="color" className="ge-color" value={textColor} onChange={(e) => setTextColor(e.target.value)} />
                    </label>
                    <div className="ge-hint">点击画布任意位置放置文字</div>
                  </>
                )}
              </div>
            )}

            {tab === 'filter' && (
              <div className="ge-tool-section">
                <div className="ge-filter-row">
                  <span className="ge-filter-label">亮度</span>
                  <input type="range" min={-1} max={1} step={0.05} value={brightnessVal} onChange={(e) => setBrightnessVal(Number(e.target.value))} />
                  <button className="ge-btn ge-btn-sm" onClick={() => applyPendingFilter('brightness')} disabled={!frame || brightnessVal === 0}>
                    应用
                  </button>
                </div>
                <div className="ge-filter-row">
                  <span className="ge-filter-label">对比度</span>
                  <input type="range" min={-1} max={1} step={0.05} value={contrastVal} onChange={(e) => setContrastVal(Number(e.target.value))} />
                  <button className="ge-btn ge-btn-sm" onClick={() => applyPendingFilter('contrast')} disabled={!frame || contrastVal === 0}>
                    应用
                  </button>
                </div>
                <div className="ge-filter-row">
                  <span className="ge-filter-label">饱和度</span>
                  <input type="range" min={-1} max={1} step={0.05} value={saturationVal} onChange={(e) => setSaturationVal(Number(e.target.value))} />
                  <button className="ge-btn ge-btn-sm" onClick={() => applyPendingFilter('saturation')} disabled={!frame || saturationVal === 0}>
                    应用
                  </button>
                </div>
                <div className="ge-tool-row">
                  <button className="ge-btn" onClick={() => applyToFrame(transforms.grayscale, currentRef.current)} disabled={!frame}>
                    灰度
                  </button>
                  <button className="ge-btn" onClick={() => applyToFrame(transforms.invert, currentRef.current)} disabled={!frame}>
                    反色
                  </button>
                </div>
                <button className="ge-btn ge-btn-full" onClick={applyFilterAll} disabled={!doc || !hasPendingFilters(filterValues)}>
                  应用到全部帧
                </button>
              </div>
            )}

            {tab === 'color' && (
              <div className="ge-tool-section">
                <div className="ge-color-target">
                  <span className="ge-field-label">目标色（点击画布取色）</span>
                  <div className="ge-swatch" style={{ background: `rgb(${targetColor.join(',')})` }} />
                  <code className="ge-code">rgb({targetColor.join(', ')})</code>
                </div>
                <label className="ge-field">
                  替换色
                  <input type="color" className="ge-color" value={replaceColor} onChange={(e) => setReplaceColor(e.target.value)} />
                </label>
                <label className="ge-field">
                  容差 {tolerance}
                  <input type="range" min={0} max={150} value={tolerance} onChange={(e) => setTolerance(Number(e.target.value))} />
                </label>
                <button className="ge-btn ge-btn-full" onClick={() => applyReplaceColor(false)} disabled={!frame}>
                  应用替换（当前帧）
                </button>
                <button className="ge-btn ge-btn-full" onClick={() => applyReplaceColor(true)} disabled={!doc}>
                  应用到全部帧
                </button>
              </div>
            )}

            {tab === 'smart' && (
              <div className="ge-tool-section">
                <label className="ge-field">
                  识别容差 {smartTolerance}
                  <input type="range" min={0} max={100} value={smartTolerance} onChange={(e) => setSmartTolerance(Number(e.target.value))} />
                </label>
                <div className="ge-tool-row">
                  <button className="ge-btn" onClick={() => applyRemoveBackground(false)} disabled={!frame || busy}>
                    删除背景
                  </button>
                  <button className="ge-btn" onClick={() => applyRemoveBackground(true)} disabled={!doc || busy}>
                    全部帧
                  </button>
                </div>
                <div className="ge-tool-row">
                  <button className="ge-btn" onClick={() => applyAutoCrop(false)} disabled={!frame || busy}>
                    按当前帧裁剪画布
                  </button>
                  <button className="ge-btn" onClick={() => applyAutoCrop(true)} disabled={!doc || busy}>
                    按全部帧联合裁剪
                  </button>
                </div>
                <div className="ge-section-title">修复叠加残留</div>
                {!residueReport ? (
                  residueAnalyzing ? (
                    <div className="ge-tool-row">
                      <button className="ge-btn ge-btn-primary" disabled>
                        检测中...
                      </button>
                      <button className="ge-btn" onClick={cancelResidueTask}>
                        停止
                      </button>
                    </div>
                  ) : (
                    <button className="ge-btn ge-btn-full" onClick={() => void handleAnalyzeResidue()} disabled={!doc}>
                      检测叠加残留
                    </button>
                  )
                ) : (
                  <>
                    <div className="ge-residue-info">
                      污染源帧 #{residueReport.pollutedFrame + 1} · 影响 {residueReport.affectedCount} 帧 · 残留 {residueReport.residuePixels.toLocaleString()}px
                    </div>
                    <div className="ge-residue-preview">
                      <div className="ge-residue-preview-col">
                        <span className="ge-field-label">修复前</span>
                        <div className="ge-residue-thumb-wrap">
                          <img src={layerThumb(residueReport.previewBefore)} alt="修复前" className="ge-residue-thumb" />
                        </div>
                      </div>
                      <div className="ge-residue-preview-col">
                        <span className="ge-field-label">修复后</span>
                        <div className="ge-residue-thumb-wrap">
                          <img src={layerThumb(residueReport.previewAfter)} alt="修复后" className="ge-residue-thumb" />
                        </div>
                      </div>
                    </div>
                    <div className="ge-tool-row">
                      <button className="ge-btn ge-btn-primary" onClick={() => void handleApplyResidueFix()} disabled={residueAnalyzing}>
                        {residueAnalyzing ? '修复中...' : '应用修复'}
                      </button>
                      <button className="ge-btn" onClick={residueAnalyzing ? cancelResidueTask : () => setResidueReport(null)}>
                        {residueAnalyzing ? '停止' : '取消'}
                      </button>
                    </div>
                    <div className="ge-hint">将污染源帧改为「画后清除」，移除其后所有帧的叠加残留</div>
                  </>
                )}
                <div className="ge-section-title">智能分离图层</div>
                <div className="ge-tool-row">
                  {(['subject', 'objects', 'colors', 'lasso'] as const).map((m) => (
                    <button key={m} className={`ge-btn ${splitMode === m ? 'ge-btn-active' : ''}`} onClick={() => { setSplitMode(m); if (m === 'lasso') { setSelectMode('lasso'); clearLasso() } }} disabled={busy}>
                      {m === 'subject' ? '主体/背景' : m === 'objects' ? '多对象' : m === 'colors' ? '颜色聚类' : '按边线'}
                    </button>
                  ))}
                </div>
                {splitMode === 'colors' && (
                  <label className="ge-field">
                    聚类层数 {clusterCount}
                    <input type="range" min={2} max={10} value={clusterCount} onChange={(e) => setClusterCount(Number(e.target.value))} />
                  </label>
                )}
                {splitMode === 'lasso' && (
                  <div className="ge-hint">用套索画一条闭合边线，围成要分离的区域</div>
                )}
                {splitMode === 'lasso' && lassoMask ? (
                  <button className="ge-btn ge-btn-full ge-btn-primary" onClick={separateByLasso} disabled={busy}>
                    分离为图层（边线内/外各一层）
                  </button>
                ) : splitMode !== 'lasso' && !layerSession ? (
                  <button className="ge-btn ge-btn-full" onClick={startLayerSeparation} disabled={!frame || busy}>
                    {busy ? '分析中...' : '开始分离'}
                  </button>
                ) : null}
                {layerSession && (
                  <>
                    <div className="ge-layer-list">
                      {layerSession.map((layer, i) => (
                        <div
                          key={layer.id}
                          className={`ge-layer-row ${activeLayerId === layer.id ? 'ge-layer-row-active' : ''}`}
                          onClick={() => { setActiveLayerId((prev) => (prev === layer.id ? null : layer.id)); setSelectMode('none') }}
                        >
                          <button className="ge-btn ge-btn-sm ge-layer-vis" onClick={(e) => { e.stopPropagation(); toggleLayer(layer.id) }} title={layer.visible ? '点击隐藏' : '点击显示'}>
                            {layer.visible ? '●' : '○'}
                          </button>
                          <span className="ge-layer-index">{i + 1}</span>
                          <div className="ge-layer-thumb-wrap">
                            <img
                              src={thumbCache.get(layer.id)?.url ?? layerThumb(layer.imageData)}
                              alt={`图层 ${i + 1}`}
                              className={`ge-layer-thumb ${layer.visible ? '' : 'ge-layer-thumb-hidden'}`}
                            />
                          </div>
                          <button className="ge-btn ge-btn-sm" onClick={(e) => { e.stopPropagation(); moveLayer(layer.id, -1) }} disabled={i === 0} title="上移">
                            ▲
                          </button>
                          <button className="ge-btn ge-btn-sm" onClick={(e) => { e.stopPropagation(); moveLayer(layer.id, 1) }} disabled={i === layerSession.length - 1} title="下移">
                            ▼
                          </button>
                          <button className="ge-btn ge-btn-sm" onClick={(e) => { e.stopPropagation(); removeLayer(layer.id) }} title="删除图层">
                            ✕
                          </button>
                        </div>
                      ))}
                    </div>
                    <div className="ge-hint">{activeLayerId ? '正在编辑选中图层 · 点击图层行取消选择' : '点击图层行可单独查看并编辑'}</div>
                    <div className="ge-tool-row">
                      <button className="ge-btn ge-btn-primary" onClick={mergeLayerSession} disabled={busy}>
                        合并图层
                      </button>
                      <button className="ge-btn" onClick={() => { clearLayerSession(); setActiveLayerId(null) }} disabled={busy}>
                        取消
                      </button>
                    </div>
                    <div className="ge-hint">合并后替换当前帧 · 勾选控制可见性</div>
                  </>
                )}
                <div className="ge-section-title">按网格拆分</div>
                <div className="ge-tool-row">
                  <label className="ge-field">
                    行
                    <input className="ge-input" type="number" min={1} max={16} value={gridRows} onChange={(e) => setGridRows(Math.min(16, Math.max(1, Number(e.target.value))))} />
                  </label>
                  <label className="ge-field">
                    列
                    <input className="ge-input" type="number" min={1} max={16} value={gridCols} onChange={(e) => setGridCols(Math.min(16, Math.max(1, Number(e.target.value))))} />
                  </label>
                </div>
                <button className="ge-btn ge-btn-full" onClick={applySplitGrid} disabled={!frame || busy}>
                  拆分当前帧
                </button>
                <div className="ge-section-title">按颜色拆分</div>
                <label className="ge-field">
                  颜色容差 {colorLayerTol}
                  <input type="range" min={0} max={120} value={colorLayerTol} onChange={(e) => setColorLayerTol(Number(e.target.value))} />
                </label>
                <label className="ge-field">
                  最多层数 {colorLayers}
                  <input type="range" min={2} max={16} value={colorLayers} onChange={(e) => setColorLayers(Number(e.target.value))} />
                </label>
                <button className="ge-btn ge-btn-full" onClick={applySplitColor} disabled={!frame || busy}>
                  拆分颜色区块
                </button>
                <div className="ge-hint">拆分后子帧保持画布尺寸，非目标区域透明</div>
              </div>
            )}

            {tab === 'select' && (
              <div className="ge-tool-section">
                <div className="ge-tool-row">
                  <button className={`ge-btn ${selectMode === 'rect' ? 'ge-btn-active' : ''}`} onClick={() => { setSelectMode('rect'); setWandMask(null); setLassoMask(null) }} disabled={!frame || busy}>
                    框选删除
                  </button>
                  <button className={`ge-btn ${selectMode === 'wand' ? 'ge-btn-active' : ''}`} onClick={() => { setSelectMode('wand'); setSelectRect(null); setLassoMask(null) }} disabled={!frame || busy}>
                    魔棒删除
                  </button>
                </div>
                <div className="ge-tool-row">
                  <button className={`ge-btn ${selectMode === 'lasso' ? 'ge-btn-active' : ''}`} onClick={() => { setSelectMode('lasso'); setSelectRect(null); setWandMask(null); clearLasso() }} disabled={!frame || busy}>
                    套索删除
                  </button>
                </div>
                {selectMode === 'rect' && (
                  <>
                    <div className="ge-hint">在画布上拖出矩形，松手后删除该区域</div>
                    <div className="ge-tool-row">
                      <button className="ge-btn" onClick={() => applyRectDelete(false)} disabled={!selectRect || busy}>
                        删除当前帧
                      </button>
                      <button className="ge-btn" onClick={() => applyRectDelete(true)} disabled={!selectRect || busy}>
                        全部帧
                      </button>
                    </div>
                    <button className="ge-btn ge-btn-full" onClick={() => { setSelectMode('none'); setSelectRect(null) }} disabled={busy}>
                      退出框选
                    </button>
                  </>
                )}
                {selectMode === 'wand' && (
                  <>
                    <label className="ge-field">
                      容差 {wandTolerance}
                      <input type="range" min={0} max={150} value={wandTolerance} onChange={(e) => setWandTolerance(Number(e.target.value))} />
                    </label>
                    <div className="ge-hint">点击画布选中同色连通区域</div>
                    <div className="ge-tool-row">
                      <button className="ge-btn" onClick={() => applyWandDelete(false)} disabled={!wandMask || busy}>
                        删除当前帧
                      </button>
                      <button className="ge-btn" onClick={() => applyWandDelete(true)} disabled={!wandMask || busy}>
                        全部帧
                      </button>
                    </div>
                    <button className="ge-btn ge-btn-full" onClick={() => { setSelectMode('none'); setWandMask(null) }} disabled={busy}>
                      退出魔棒
                    </button>
                  </>
                )}
                {selectMode === 'lasso' && (
                  <>
                    <div className="ge-tool-row">
                      <button className={`ge-btn ${lassoMode === 'poly' ? 'ge-btn-active' : ''}`} onClick={() => { setLassoMode('poly'); clearLasso() }} disabled={busy}>
                        多边形
                      </button>
                      <button className={`ge-btn ${lassoMode === 'free' ? 'ge-btn-active' : ''}`} onClick={() => { setLassoMode('free'); clearLasso() }} disabled={busy}>
                        手绘
                      </button>
                    </div>
                    <label className="ge-field">
                      保留边线宽度 {lassoEdgeWidth}px
                      <input type="range" min={1} max={8} value={lassoEdgeWidth} onChange={(e) => setLassoEdgeWidth(Number(e.target.value))} />
                    </label>
                    {!lassoMask ? (
                      <div className="ge-hint">
                        {lassoMode === 'poly'
                          ? '点击画布添加顶点，双击 / 点击首点闭合'
                          : '按住鼠标沿边界描绘，松手自动闭合'}
                      </div>
                    ) : (
                      <div className="ge-hint">边线已闭合，内部区域已标记（边线保留）</div>
                    )}
                    <div className="ge-tool-row">
                      <button className="ge-btn" onClick={() => applyLassoDelete(false)} disabled={!lassoMask || busy}>
                        删除当前帧
                      </button>
                      <button className="ge-btn" onClick={() => applyLassoDelete(true)} disabled={!lassoMask || busy}>
                        全部帧
                      </button>
                    </div>
                    <button className="ge-btn ge-btn-full" onClick={() => { if (lassoMask) { clearLasso() } else { setSelectMode('none') } }} disabled={busy}>
                      {lassoMask ? '重新描绘' : '退出套索'}
                    </button>
                  </>
                )}
              </div>
            )}
          </div>
        </div>
      </div>

      <div className="ge-timeline">
        <div className="ge-timeline-actions">
          <button className="ge-btn ge-btn-sm" onClick={addBlankFrame} disabled={!doc}>
            ＋空白帧
          </button>
          <button className="ge-btn ge-btn-sm" onClick={removeFrame} disabled={!doc}>
            删除
          </button>
          <button className="ge-btn ge-btn-sm" onClick={() => moveFrame(-1)} disabled={!doc || current === 0}>
            ◀ 左移
          </button>
          <button className="ge-btn ge-btn-sm" onClick={() => moveFrame(1)} disabled={!doc || !doc.frames[current + 1]}>
            右移 ▶
          </button>
          <span className="ge-timeline-sep" />
          <span className="ge-batch-label">批量延迟</span>
          <input
            className="ge-input ge-input-num"
            type="number"
            min={20}
            max={10000}
            step={10}
            value={batchDelay}
            onChange={(e) => setBatchDelay(Number(e.target.value))}
            title="目标延迟(ms)"
          />
          <span className="ge-batch-label">帧</span>
          <input
            className="ge-input ge-input-num ge-input-sm"
            type="number"
            min={1}
            value={batchFrom}
            onChange={(e) => setBatchFrom(Number(e.target.value))}
            title="起始帧"
          />
          <span className="ge-batch-label">至</span>
          <input
            className="ge-input ge-input-num ge-input-sm"
            type="number"
            min={1}
            value={batchTo}
            onChange={(e) => setBatchTo(Number(e.target.value))}
            title="结束帧"
          />
          <button className="ge-btn ge-btn-sm" onClick={setBatchFrameDelay} disabled={!doc}>
            应用
          </button>
          <button className="ge-btn ge-btn-sm" onClick={setAllFrameDelay} disabled={!doc}>
            全部帧
          </button>
        </div>
        <div className="ge-thumbs">
          {doc?.frames.map((f, i) => (
            <div key={f.id} className={`ge-thumb ${i === current ? 'ge-thumb-active' : ''}`} onClick={() => { stopPlay(); setCurrent(i); setLassoMask(null); setLassoPoints([]); setLassoPreview(null); setLayerSession(null); setActiveLayerId(null) }}>
              <div className="ge-thumb-img-wrap">
                <img src={thumbCache.get(f.id)?.url} alt={`帧 ${i + 1}`} className="ge-thumb-img" />
                <span className="ge-thumb-index">{i + 1}</span>
              </div>
              <input
                className="ge-thumb-delay"
                type="number"
                min={20}
                value={f.delay}
                onClick={(e) => e.stopPropagation()}
                onChange={(e) => setFrameDelay(i, Math.max(20, Number(e.target.value)))}
                title="帧延迟(ms)"
              />
            </div>
          ))}
        </div>
      </div>

      <canvas ref={editCanvasRef} style={{ display: 'none' }} />
    </div>
  )
}

function formatSize(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(1)
}

function cloneImageData(imageData: ImageData): ImageData {
  return new ImageData(
    new Uint8ClampedArray(imageData.data),
    imageData.width,
    imageData.height
  )
}

function layerThumb(imageData: ImageData): string {
  const canvas = document.createElement('canvas')
  canvas.width = imageData.width
  canvas.height = imageData.height
  const ctx = canvas.getContext('2d')
  if (!ctx) return ''
  ctx.putImageData(imageData, 0, 0)
  return canvas.toDataURL('image/png')
}

function cloneLayers(layers: LayerItem[]): LayerItem[] {
  return layers.map((l) => ({ ...l, imageData: new ImageData(new Uint8ClampedArray(l.imageData.data), l.imageData.width, l.imageData.height) }))
}
