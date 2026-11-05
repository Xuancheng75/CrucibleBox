export declare function validateAppearance(raw: string): {
  mode: 'light' | 'dark'
  cssVars: Record<string, string>
}
export declare function validateFilesDropped(raw: string): string[]
export declare function validateThemeStyle(
  raw: string
): import('../packages/cruciblebox-next2-api/src/generated.mjs').RendererThemeStyle
