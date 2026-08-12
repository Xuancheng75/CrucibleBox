import { readFileSync, readdirSync } from 'node:fs'
import { extname, join, resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import packageJson from '../package.json'
import { themeToCssVars } from '../shared/themes/css-vars'
import { PRESET_THEMES } from '../shared/themes/presets'

const projectRoot = resolve(import.meta.dirname, '..')
const productionPlugins = [
  'diary',
  'dice-roller',
  'gif-editor',
  'theme-manager',
  'turntable',
  'unienv'
] as const

function collectSourceFiles(directory: string): string[] {
  const files: string[] = []
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) files.push(...collectSourceFiles(path))
    else if (['.ts', '.tsx', '.css'].includes(extname(entry.name))) files.push(path)
  }
  return files
}

describe('Ant Design 6 migration boundary', () => {
  it('pins compatible Ant Design and icon majors', () => {
    expect(packageJson.devDependencies.antd).toMatch(/^6\./)
    expect(packageJson.dependencies['@ant-design/icons']).toMatch(/^6\./)
  })

  it('does not style or request Ant Design internal DOM class names', () => {
    for (const file of collectSourceFiles(join(projectRoot, 'src'))) {
      const source = readFileSync(file, 'utf8')
      expect(source, file).not.toMatch(/\.ant-[a-z0-9-]+/i)
      expect(source, file).not.toMatch(/className=["']ant-/i)
    }
  })

  it('does not use the v6 deprecated component props present in the old host UI', () => {
    const deprecatedProps =
      /\b(message|tip|direction|labelStyle|contentStyle|destroyOnClose|bodyStyle|maskStyle|popupClassName|dropdownRender|onDropdownVisibleChange)=/
    for (const file of collectSourceFiles(join(projectRoot, 'src')).filter((path) =>
      ['.ts', '.tsx'].includes(extname(path))
    )) {
      expect(readFileSync(file, 'utf8'), file).not.toMatch(deprecatedProps)
    }
  })

  it('keeps a complete six-theme by six-plugin visual contract matrix', () => {
    expect(PRESET_THEMES).toHaveLength(6)
    for (const plugin of productionPlugins) {
      const manifest = JSON.parse(
        readFileSync(join(projectRoot, 'plugins', plugin, 'plugin.json'), 'utf8')
      ) as { rendererApiVersion?: number }
      expect(manifest.rendererApiVersion, plugin).toBe(2)
      expect(
        readFileSync(join(projectRoot, 'plugins', plugin, 'dist', 'renderer.js'))
      ).not.toHaveLength(0)
      for (const theme of PRESET_THEMES) {
        const variables = themeToCssVars(theme)
        expect(Object.keys(variables).length, `${theme.id}/${plugin}`).toBeGreaterThan(20)
        expect(variables['--ob-theme-id'], `${theme.id}/${plugin}`).toBe(theme.id)
        expect(variables['--ob-color-bg-container'], `${theme.id}/${plugin}`).toBeTruthy()
        expect(variables['--ob-color-text'], `${theme.id}/${plugin}`).toBeTruthy()
      }
    }
  })

  it('keeps the launcher and command palette keyboard semantics explicit', () => {
    const launcher = readFileSync(
      join(projectRoot, 'src', 'components', 'LauncherCard.tsx'),
      'utf8'
    )
    const palette = readFileSync(
      join(projectRoot, 'src', 'components', 'CommandPalette.tsx'),
      'utf8'
    )
    const styles = readFileSync(join(projectRoot, 'src', 'styles', 'global.css'), 'utf8')

    expect(launcher).toContain('className="ob-launcher-open"')
    expect(launcher).toContain('type="button"')
    expect(styles).toContain('.ob-launcher:focus-within .ob-launcher-actions')
    expect(palette).toContain('role="dialog"')
    expect(palette).toContain('role="listbox"')
    expect(palette).toContain('aria-activedescendant=')
  })
})
