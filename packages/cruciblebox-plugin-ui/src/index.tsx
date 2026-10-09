import type { ButtonHTMLAttributes, HTMLAttributes, InputHTMLAttributes, ReactNode } from 'react'

declare const __CBX_PLUGIN_UI_STYLES__: string

if (typeof document !== 'undefined' && !document.getElementById('cruciblebox-plugin-ui-styles')) {
  const style = document.createElement('style')
  style.id = 'cruciblebox-plugin-ui-styles'
  style.textContent = __CBX_PLUGIN_UI_STYLES__
  document.head.append(style)
}

type PaneProps = HTMLAttributes<HTMLElement> & { children: ReactNode }

export function PluginPage({ children, className = '', ...props }: PaneProps) {
  return <main className={`cbx-plugin-page ${className}`} {...props}>{children}</main>
}

export function Toolbar({ children, className = '', ...props }: PaneProps) {
  return <header className={`cbx-plugin-toolbar ${className}`} {...props}>{children}</header>
}

export function SplitPane({ children, className = '', ...props }: PaneProps) {
  return <div className={`cbx-plugin-split ${className}`} {...props}>{children}</div>
}

export interface FieldProps extends HTMLAttributes<HTMLDivElement> {
  label: string
  hint?: string
  children: ReactNode
}

export function Field({ label, hint, children, className = '', ...props }: FieldProps) {
  return <div className={`cbx-plugin-field ${className}`} {...props}>
    <span className="cbx-plugin-field-label">{label}</span>
    {children}
    {hint && <small>{hint}</small>}
  </div>
}

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'danger'
}

export function Button({ children, variant = 'secondary', className = '', type = 'button', ...props }: ButtonProps) {
  return <button type={type} className={`cbx-plugin-button cbx-plugin-button-${variant} ${className}`} {...props}>{children}</button>
}

export function TextInput({ className = '', ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return <input className={`cbx-plugin-input ${className}`} {...props} />
}

export interface FileItem { id: string; name: string; detail?: string }
export interface FileListProps extends HTMLAttributes<HTMLUListElement> {
  items: FileItem[]
  selectedId?: string
  onItemSelect?: (item: FileItem) => void
}

export function FileList({ items, selectedId, onItemSelect, className = '', ...props }: FileListProps) {
  return <ul className={`cbx-plugin-list ${className}`} {...props}>
    {items.map((item) => <li key={item.id}>
      {onItemSelect ? <button type="button" className={selectedId === item.id ? 'is-selected' : ''} onClick={() => onItemSelect(item)}>
        <strong>{item.name}</strong>{item.detail && <small>{item.detail}</small>}
      </button> : <div className="cbx-plugin-list-item"><strong>{item.name}</strong>{item.detail && <small>{item.detail}</small>}</div>}
    </li>)}
  </ul>
}

export interface TaskProgressProps extends HTMLAttributes<HTMLDivElement> {
  label: string
  progress: number
  status?: string
}

export function TaskProgress({ label, progress, status, className = '', ...props }: TaskProgressProps) {
  const value = Math.max(0, Math.min(100, Number.isFinite(progress) ? progress : 0))
  return <div className={`cbx-plugin-progress ${className}`} {...props}>
    <div><strong>{label}</strong><span>{status ?? `${Math.round(value)}%`}</span></div>
    <div role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={value}>
      <span style={{ width: `${value}%` }} />
    </div>
  </div>
}

export interface ResultItem { id: string; title: string; detail?: string; action?: ReactNode }
export interface ResultListProps extends HTMLAttributes<HTMLUListElement> { items: ResultItem[] }

export function ResultList({ items, className = '', ...props }: ResultListProps) {
  return <ul className={`cbx-plugin-results ${className}`} {...props}>
    {items.map((item) => <li key={item.id}>
      <div><strong>{item.title}</strong>{item.detail && <small>{item.detail}</small>}</div>
      {item.action}
    </li>)}
  </ul>
}

export interface MessagePanelProps extends HTMLAttributes<HTMLDivElement> {
  title: string
  detail?: string
  action?: ReactNode
}

export function EmptyState({ title, detail, action, className = '', ...props }: MessagePanelProps) {
  return <div className={`cbx-plugin-message ${className}`} {...props}>
    <strong>{title}</strong>{detail && <p>{detail}</p>}{action}
  </div>
}

export function ErrorPanel({ title, detail, action, className = '', ...props }: MessagePanelProps) {
  return <div className={`cbx-plugin-message cbx-plugin-error ${className}`} role="alert" {...props}>
    <strong>{title}</strong>{detail && <p>{detail}</p>}{action}
  </div>
}
