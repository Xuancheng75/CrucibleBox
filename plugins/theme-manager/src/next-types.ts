export interface Theme {
  id: string
  name: string
  mode: 'light' | 'dark'
  tokens: Record<string, string | number>
}
export interface ThemeRenderProps {
  theme: Theme
  config: Record<string, unknown>
  onConfigChange: (config: Record<string, unknown>) => Promise<void>
  api: {
    theme: {
      get(): Promise<Theme>
      list(): Promise<Theme[]>
      preview(theme: Theme): Promise<boolean>
      commit(): Promise<boolean>
      rollback(): Promise<boolean>
    }
  }
}
