import React, { useEffect, useRef, useState } from 'react'
import { shouldLeaveAfterSave } from '../diary-domain'
import {
  deleteEntry,
  discardDraft,
  exportMonth,
  exportSingle,
  getEntry,
  saveDraft,
  saveEntry
} from '../utils/db'
import { downloadAsFile, formatDate } from '../utils/export'
import DiaryEditor from './DiaryEditor'
import DiaryPreview from './DiaryPreview'
import WeekBar from './WeekBar'

interface DiaryPageProps {
  date: string
  onBack: () => void
  onSelectDate: (date: string) => void
}

interface LatestDraft {
  date: string
  title: string
  content: string
  dirty: boolean
  ready: boolean
}

export default function DiaryPage({ date, onBack, onSelectDate }: DiaryPageProps) {
  const [title, setTitle] = useState('')
  const [content, setContent] = useState('')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [dirty, setDirty] = useState(false)
  const [recoveredDraft, setRecoveredDraft] = useState(false)
  const [saveError, setSaveError] = useState<string | null>(null)
  const [showModal, setShowModal] = useState(false)
  const [showDeleteModal, setShowDeleteModal] = useState(false)
  const pendingAction = useRef<(() => void) | null>(null)
  const editRevision = useRef(0)
  const latestDraft = useRef<LatestDraft>({ date, title: '', content: '', dirty: false, ready: false })

  latestDraft.current = { date, title, content, dirty, ready: !loading }

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setSaveError(null)
    setRecoveredDraft(false)

    void (async () => {
      try {
        const loaded = await getEntry(date)
        if (cancelled) return
        const source = loaded.draft ?? loaded.entry
        setTitle(source?.title ?? '')
        setContent(source?.content ?? '')
        setDirty(Boolean(loaded.draft))
        setRecoveredDraft(Boolean(loaded.draft))
        editRevision.current = 0
      } catch (error) {
        if (!cancelled) {
          setSaveError(error instanceof Error ? error.message : '日记加载失败')
        }
      } finally {
        if (!cancelled) setLoading(false)
      }
    })()

    return () => {
      cancelled = true
    }
  }, [date])

  useEffect(() => {
    if (loading || !dirty) return
    const revision = editRevision.current
    const timer = window.setTimeout(() => {
      void saveDraft(date, title, content).then((result) => {
        if (!result.ok && editRevision.current === revision) setSaveError(result.error.message)
      })
    }, 500)
    return () => window.clearTimeout(timer)
  }, [content, date, dirty, loading, title])

  useEffect(() => {
    return () => {
      const draft = latestDraft.current
      if (draft.ready && draft.dirty) {
        void saveDraft(draft.date, draft.title, draft.content)
      }
    }
  }, [])

  const handleSave = async (): Promise<boolean> => {
    const revision = editRevision.current
    setSaving(true)
    setSaveError(null)
    try {
      const result = await saveEntry(date, title, content)
      if (!result.ok) {
        setSaveError(result.error.message)
        return false
      }
      if (!shouldLeaveAfterSave(result, revision, editRevision.current)) {
        setSaveError('保存期间内容已发生变化，请再次保存')
        return false
      }
      latestDraft.current = { ...latestDraft.current, dirty: false }
      setDirty(false)
      setRecoveredDraft(false)
      return true
    } catch (error) {
      setSaveError(error instanceof Error ? error.message : '日记保存失败')
      return false
    } finally {
      setSaving(false)
    }
  }

  const markEdited = () => {
    editRevision.current += 1
    setDirty(true)
    setRecoveredDraft(false)
    setSaveError(null)
  }

  const handleTitleChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    setTitle(event.target.value)
    markEdited()
  }

  const handleContentChange = (value: string) => {
    setContent(value)
    markEdited()
  }

  const tryLeave = (action: () => void) => {
    if (!dirty) {
      action()
      return
    }
    pendingAction.current = action
    setShowModal(true)
  }

  const handleConfirmSaveAndLeave = async () => {
    if (!(await handleSave())) return
    setShowModal(false)
    const action = pendingAction.current
    pendingAction.current = null
    action?.()
  }

  const handleConfirmDiscard = async () => {
    const result = await discardDraft(date)
    if (!result.ok) {
      setSaveError(result.error.message)
      return
    }
    latestDraft.current = { ...latestDraft.current, dirty: false }
    setDirty(false)
    setShowModal(false)
    const action = pendingAction.current
    pendingAction.current = null
    action?.()
  }

  const handleCancelLeave = () => {
    setShowModal(false)
    pendingAction.current = null
  }

  const handleExportSingle = async () => {
    try {
      downloadAsFile(await exportSingle(date), `日记_${date}.md`)
    } catch (error) {
      setSaveError(error instanceof Error ? error.message : '导出失败')
    }
  }

  const handleExportMonth = async () => {
    try {
      const [year, month] = date.split('-')
      downloadAsFile(await exportMonth(Number(year), Number(month)), `日记_${year}年${month}月.md`)
    } catch (error) {
      setSaveError(error instanceof Error ? error.message : '导出失败')
    }
  }

  const handleConfirmDelete = async () => {
    setShowDeleteModal(false)
    const result = await deleteEntry(date)
    if (!result.ok) {
      setSaveError(result.error.message)
      return
    }
    latestDraft.current = { ...latestDraft.current, dirty: false }
    setDirty(false)
    onBack()
  }

  if (loading) return <div className="diary-page-loading">加载中...</div>

  return (
    <div className="diary-page">
      <div className="diary-page-header">
        <div className="diary-page-top">
          <button className="nav-btn back-btn" onClick={() => tryLeave(onBack)}>
            ← 返回
          </button>
          <span className="diary-date-label">{formatDate(date)}</span>
          <div className="diary-page-actions">
            {saving ? (
              <span className="saving-indicator">保存中...</span>
            ) : (
              <button
                className={`nav-btn save-btn ${dirty ? 'dirty' : ''}`}
                onClick={() => void handleSave()}
              >
                {dirty ? '保存 *' : '保存'}
              </button>
            )}
            <button className="nav-btn" onClick={() => void handleExportSingle()}>
              导出单篇
            </button>
            <button className="nav-btn" onClick={() => void handleExportMonth()}>
              导出本月
            </button>
            <button className="nav-btn delete-btn" onClick={() => setShowDeleteModal(true)}>
              删除
            </button>
          </div>
        </div>
        {(recoveredDraft || saveError) && (
          <div className={saveError ? 'diary-status error' : 'diary-status'} role="status">
            {saveError ?? '已恢复未保存的草稿'}
          </div>
        )}
        <input
          className="diary-title-input"
          type="text"
          value={title}
          onChange={handleTitleChange}
          placeholder="日记标题..."
        />
        <WeekBar selectedDate={date} onSelectDate={(next) => tryLeave(() => onSelectDate(next))} />
      </div>
      <div className="diary-page-body">
        <DiaryEditor content={content} onChange={handleContentChange} />
        <div className="diary-divider" />
        <DiaryPreview content={content} />
      </div>

      {showModal && (
        <div className="modal-overlay" onClick={handleCancelLeave}>
          <div className="modal-content" onClick={(event) => event.stopPropagation()}>
            <h3 className="modal-title">未保存的更改</h3>
            <p className="modal-message">您有未保存的内容，是否保存后再离开？</p>
            <div className="modal-actions">
              <button
                className="nav-btn modal-btn-primary"
                disabled={saving}
                onClick={() => void handleConfirmSaveAndLeave()}
              >
                保存并离开
              </button>
              <button className="nav-btn" onClick={() => void handleConfirmDiscard()}>
                不保存
              </button>
              <button className="nav-btn" onClick={handleCancelLeave}>
                取消
              </button>
            </div>
          </div>
        </div>
      )}

      {showDeleteModal && (
        <div className="modal-overlay" onClick={() => setShowDeleteModal(false)}>
          <div className="modal-content" onClick={(event) => event.stopPropagation()}>
            <h3 className="modal-title">确认删除</h3>
            <p className="modal-message">确定要永久删除这篇日记吗？此操作不可撤销。</p>
            <div className="modal-actions">
              <button className="nav-btn delete-btn" onClick={() => void handleConfirmDelete()}>
                确定删除
              </button>
              <button className="nav-btn" onClick={() => setShowDeleteModal(false)}>
                取消
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
