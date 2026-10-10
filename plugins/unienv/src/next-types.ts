export type PluginConfig = Record<string, unknown>
export interface ServiceRenderProps {
  config: PluginConfig
  onConfigChange: (values: PluginConfig) => Promise<void>
  api: {
    service: { call(message: unknown): Promise<unknown> }
    dialog: {
      open(options: {
        type: 'file' | 'folder'
        multiple?: boolean
        extensions?: string[]
      }): Promise<string[]>
    }
    confirm(options: {
      title: string
      message: string
      confirmLabel?: string
      cancelLabel?: string
    }): Promise<boolean>
    notify(title: string, body?: string): void
    onFilesDropped(listener: (paths: string[]) => void): () => void
  }
}
