import { Component, type ErrorInfo, type ReactNode } from 'react'
import { Alert, Button } from 'antd'
import { HomeOutlined, RedoOutlined } from '@ant-design/icons'
import { useAppStore } from '../store/app.store'

interface PageErrorBoundaryProps {
  children: ReactNode
  pageName?: string
}

interface PageErrorBoundaryState {
  hasError: boolean
  error: Error | null
}

export class PageErrorBoundary extends Component<PageErrorBoundaryProps, PageErrorBoundaryState> {
  state: PageErrorBoundaryState = { hasError: false, error: null }

  static getDerivedStateFromError(error: Error): PageErrorBoundaryState {
    return { hasError: true, error }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('[PageErrorBoundary] 页面渲染异常', error, info)
  }

  handleRetry = () => {
    this.setState({ hasError: false, error: null })
  }

  render() {
    if (!this.state.hasError) {
      return this.props.children
    }

    return (
      <Fallback
        pageName={this.props.pageName}
        error={this.state.error}
        onRetry={this.handleRetry}
      />
    )
  }
}

interface FallbackProps {
  pageName?: string
  error: Error | null
  onRetry: () => void
}

function Fallback({ pageName, error, onRetry }: FallbackProps) {
  const setCurrentPage = useAppStore((s) => s.setCurrentPage)

  const handleHome = () => {
    onRetry()
    setCurrentPage('home')
  }

  return (
    <div role="alert" aria-live="assertive" aria-atomic="true" style={{ padding: 24 }}>
      <Alert
        className="ob-alert-error"
        type="error"
        showIcon
        title={`${pageName ?? '当前页面'}渲染出错`}
        description={error?.message ?? '发生未知错误，请尝试恢复。'}
        action={
          <div style={{ display: 'flex', gap: 8 }}>
            <Button icon={<RedoOutlined />} aria-label="重试当前页面" onClick={onRetry}>
              重试当前页面
            </Button>
            <Button
              type="primary"
              icon={<HomeOutlined />}
              aria-label="返回主页"
              onClick={handleHome}
            >
              返回主页
            </Button>
          </div>
        }
      />
    </div>
  )
}
