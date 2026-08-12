import { existsSync, readFileSync } from 'node:fs'
import { dirname, isAbsolute, normalize, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

interface PluginCatalogEntry {
  id: string
  runtimeFiles: string[]
}

interface PluginManifest {
  backend?: boolean
  name: string
  version: string
  main: string
  renderer: string
}

interface PackageMetadata {
  private?: boolean
  version: string
  workspaces?: string[]
  scripts?: Record<string, string>
}

interface TemplateManifest {
  backendApiVersion?: number
  main: string
  renderer: string
  rendererApiVersion?: number
}

const testDirectory = dirname(fileURLToPath(import.meta.url))
const repositoryRoot = resolve(testDirectory, '..')
const catalog = JSON.parse(
  readFileSync(resolve(repositoryRoot, 'scripts', 'plugin-catalog.json'), 'utf8')
) as PluginCatalogEntry[]
const rootPackage = JSON.parse(
  readFileSync(resolve(repositoryRoot, 'package.json'), 'utf8')
) as PackageMetadata
const rootPackageLock = JSON.parse(
  readFileSync(resolve(repositoryRoot, 'package-lock.json'), 'utf8')
) as { packages: Record<string, { resolved?: string; version?: string }> }

describe('production plugin source projects', () => {
  it('contains the six expected plugins', () => {
    expect(catalog.map((plugin) => plugin.id)).toEqual([
      'diary',
      'dice-roller',
      'gif-editor',
      'theme-manager',
      'turntable',
      'unienv'
    ])
  })

  it('uses the root workspace lockfile as the only dependency lock', () => {
    expect(rootPackage.workspaces).toEqual(['plugins/*', 'packages/*'])

    for (const plugin of catalog) {
      expect(existsSync(resolve(repositoryRoot, 'plugins', plugin.id, 'package-lock.json'))).toBe(
        false
      )
      expect(rootPackageLock.packages[`plugins/${plugin.id}`]?.version).toBeDefined()
    }
  })

  it('resolves registry artifacts from the pinned npm registry', () => {
    const registryHosts = new Set(
      Object.values(rootPackageLock.packages)
        .map((entry) => entry.resolved)
        .filter((resolved): resolved is string => resolved?.startsWith('https://') === true)
        .map((resolved) => new URL(resolved).host)
    )

    expect([...registryHosts]).toEqual(['registry.npmjs.org'])
  })

  for (const plugin of catalog) {
    it(`${plugin.id} has aligned metadata and reproducible build scripts`, () => {
      const pluginDirectory = resolve(repositoryRoot, 'plugins', plugin.id)
      const packageJson = JSON.parse(
        readFileSync(resolve(pluginDirectory, 'package.json'), 'utf8')
      ) as PackageMetadata
      const manifest = JSON.parse(
        readFileSync(resolve(pluginDirectory, 'plugin.json'), 'utf8')
      ) as PluginManifest

      expect(packageJson.private).toBe(true)
      expect(packageJson.version).toBe(manifest.version)
      expect(rootPackageLock.packages[`plugins/${plugin.id}`]?.version).toBe(manifest.version)
      expect(packageJson.scripts).toMatchObject({
        clean: expect.any(String),
        build: expect.any(String),
        typecheck: expect.any(String)
      })
      expect(manifest.name).toBe(plugin.id)
      expect(manifest.backend === false).toBe(plugin.id === 'dice-roller')
      expect(existsSync(resolve(pluginDirectory, 'src', 'main.ts'))).toBe(true)
      expect(existsSync(resolve(pluginDirectory, 'src', 'renderer.tsx'))).toBe(true)

      for (const entrypoint of [manifest.main, manifest.renderer]) {
        const normalized = normalize(entrypoint).replaceAll('\\', '/')
        expect(isAbsolute(entrypoint)).toBe(false)
        expect(normalized.startsWith('../')).toBe(false)
        expect(plugin.runtimeFiles).toContain(normalized)
      }
    })
  }

  it('packages only the pinned UniEnv proxy and renderer', () => {
    const unienv = catalog.find((plugin) => plugin.id === 'unienv')
    expect(unienv?.runtimeFiles).toEqual(['plugin.json', 'dist/main.js', 'dist/renderer.js'])
    expect(unienv?.runtimeFiles.some((file) => file.includes('process-runner'))).toBe(false)
    expect(unienv?.runtimeFiles.some((file) => file.includes('tools/'))).toBe(false)
  })

  it('packages the tsc-emitted turntable-domain module without source maps or declarations', () => {
    const turntable = catalog.find((plugin) => plugin.id === 'turntable')
    expect(turntable?.runtimeFiles).toEqual(
      expect.arrayContaining([
        'plugin.json',
        'dist/main.js',
        'dist/renderer.js',
        'dist/turntable-domain.js'
      ])
    )
    expect(
      turntable?.runtimeFiles.some((file) => file.endsWith('.map') || file.endsWith('.d.ts'))
    ).toBe(false)
    // dist/main.js is tsc-emitted CommonJS, so the shared domain module is a runtime dependency.
    const turntableMain = readFileSync(
      resolve(repositoryRoot, 'plugins', 'turntable', 'src', 'main.ts'),
      'utf8'
    )
    expect(turntableMain).toContain("from './turntable-domain'")
  })

  it('keeps the esbuild-bundled diary package free of a separate domain module', () => {
    const diary = catalog.find((plugin) => plugin.id === 'diary')
    expect(diary?.runtimeFiles).toEqual(['plugin.json', 'dist/main.js', 'dist/renderer.js'])
    expect(diary?.runtimeFiles).not.toContain('dist/diary-domain.js')
  })

  it('keeps the plugin template on the v2 browser-bundle contract', () => {
    const templateDirectory = resolve(repositoryRoot, 'templates', 'plugin-template')
    const manifest = JSON.parse(
      readFileSync(resolve(templateDirectory, 'plugin.json'), 'utf8')
    ) as TemplateManifest
    const packageJson = JSON.parse(
      readFileSync(resolve(templateDirectory, 'package.json'), 'utf8')
    ) as PackageMetadata

    expect(manifest).toMatchObject({
      backendApiVersion: 2,
      rendererApiVersion: 2,
      main: 'dist/main.js',
      renderer: 'dist/renderer.js'
    })
    expect(packageJson.scripts?.build).toContain('esbuild.config.mjs')
    expect(existsSync(resolve(templateDirectory, 'esbuild.config.mjs'))).toBe(true)
    expect(existsSync(resolve(templateDirectory, 'src', 'renderer-entry.tsx'))).toBe(true)
  })
})
