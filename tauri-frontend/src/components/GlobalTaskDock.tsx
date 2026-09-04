import { Button } from 'antd'
import { useAppStore } from '../store/app.store'
import { useTaskStore } from '../store/task.store'

export default function GlobalTaskDock() {
  const task = useTaskStore((state) =>
    state.tasks.find((item) => item.status === 'running' || item.status === 'queued')
  )
  const setCurrentPage = useAppStore((state) => state.setCurrentPage)
  const setActivityTab = useAppStore((state) => state.setActivityTab)

  if (!task) return null
  const progress = Math.max(0, Math.min(100, task.progress ?? 0))

  return (
    <div className="ob-market-download-dock" role="status" aria-live="polite">
      <div className="ob-market-download-dock-title">
        <span>{task.status === 'queued' ? '任务等待中' : '后台任务进行中'}</span>
        <Button
          type="link"
          size="small"
          onClick={() => {
            setActivityTab('tasks')
            setCurrentPage('tasks')
          }}
        >
          查看任务
        </Button>
      </div>
      <div className="ob-market-download-dock-name">{task.title}</div>
      <div className="ob-market-download-dock-track">
        <span style={{ width: `${progress}%` }} />
      </div>
      <span className="ob-market-download-dock-percent">{Math.round(progress)}%</span>
    </div>
  )
}
