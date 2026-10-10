import { validateManifest } from '../packages/cruciblebox-next-api/src/index.mjs'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, isAbsolute, normalize, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

interface PluginCatalogEntry {
  id: string
  runtimeFiles: string[]
}

interface PluginManifest {
  manifestVersion?: number
  id?: string
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
const nextIds = new Set(
  (
    JSON.parse(
      readFileSync(resolve(repositoryRoot, 'scripts/next-plugin-catalog.json'), 'utf8')
    ) as PluginCatalogEntry[]
  ).map((entry) => entry.id)
)
const rootPackage = JSON.parse(
  readFileSync(resolve(repositoryRoot, 'package.json'), 'utf8')
) as PackageMetadata
const rootPackageLock = JSON.parse(
  readFileSync(resolve(repositoryRoot, 'package-lock.json'), 'utf8')
) as { packages: Record<string, { resolved?: string; version?: string }> }

describe('production plugin source projects', () => {
  it('contains the expected production plugins', () => {
    expect(catalog.map((plugin) => plugin.id)).toEqual([
      'clipboard-manager',
      'diary',
      'gif-editor',
      'turntable',
      'exchange-rates',
      'archive-extractor',
      'json-toolkit',
      'media-toolkit',
      'audio-video-processor',
      'developer-toolkit',
      'theme-manager',
      'unienv',
      'system-info',
      'document-engine'
    ])
  })

  it('uses the root workspace lockfile as the only dependency lock', () => {
    expect(rootPackage.workspaces).toEqual([
      'plugins/*',
      'packages/cruciblebox-plugin-api',
      'packages/cruciblebox-plugin-ui',
      'packages/openbox-rpc'
    ])

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
      const next = nextIds.has(plugin.id)
      if (next) {
        expect(validateManifest(JSON.stringify(manifest)).id).toBe(plugin.id)
        expect(manifest.backend).toBeUndefined()
        expect(packageJson.scripts?.build).toBe('node scripts/build-next.mjs')
        expect(readFileSync(resolve(pluginDirectory, 'vendor/plugin-build/cli.mjs'), 'utf8')).toBe(
          readFileSync(resolve(repositoryRoot, 'packages/cruciblebox-plugin-build/cli.mjs'), 'utf8')
        )
        for (const file of ['index.mjs', 'index.d.ts', 'index.d.mts', 'generated.mjs']) {
          const vendor = resolve(pluginDirectory, 'vendor/next-api/src', file)
          const source = resolve(repositoryRoot, 'packages/cruciblebox-next-api/src', file)
          if (existsSync(source))
            expect(readFileSync(vendor, 'utf8')).toBe(readFileSync(source, 'utf8'))
        }
      } else {
        expect(manifest.name).toBe(plugin.id)
        expect(manifest.backend === false).toBe(
          ['dice-roller', 'json-toolkit', 'media-toolkit'].includes(plugin.id)
        )
        expect(existsSync(resolve(pluginDirectory, 'src', 'main.ts'))).toBe(true)
      }
      expect(existsSync(resolve(pluginDirectory, 'src', 'renderer.tsx'))).toBe(true)
      for (const entrypoint of next ? [manifest.renderer] : [manifest.main, manifest.renderer]) {
        const normalized = normalize(entrypoint).replaceAll('\\', '/')
        expect(isAbsolute(entrypoint)).toBe(false)
        expect(normalized.startsWith('../')).toBe(false)
        expect(plugin.runtimeFiles).toContain(normalized)
      }
    })
  }

  it('packages only the pinned Next UniEnv renderer', () => {
    const unienv = catalog.find((plugin) => plugin.id === 'unienv')
    expect(unienv?.runtimeFiles).toEqual(['plugin.json', 'dist/renderer.js'])
    expect(unienv?.runtimeFiles.some((file) => file.includes('process-runner'))).toBe(false)
    expect(unienv?.runtimeFiles.some((file) => file.includes('tools/'))).toBe(false)
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
      backendApiVersion: 4,
      rendererApiVersion: 4,
      main: 'dist/main.js',
      renderer: 'dist/renderer.js'
    })
    expect(packageJson.scripts?.build).toContain('esbuild.config.mjs')
    expect(existsSync(resolve(templateDirectory, 'esbuild.config.mjs'))).toBe(true)
    expect(existsSync(resolve(templateDirectory, 'src', 'renderer-entry.tsx'))).toBe(true)
  })

  it('keeps every vendored renderer builder identical to the template copy', () => {
    const builderPath = (project: string) =>
      resolve(repositoryRoot, project, 'scripts', 'build-plugin-renderer.mjs')
    const copies = [
      ...catalog
        .filter((plugin) => !nextIds.has(plugin.id))
        .map((plugin) => `plugins/${plugin.id}`)
        .filter((project) => existsSync(builderPath(project))),
      'templates/plugin-template'
    ]
    const reference = readFileSync(builderPath('templates/plugin-template'), 'utf8')
    expect(reference.length).toBeGreaterThan(0)
    for (const project of copies.slice(1)) {
      expect(readFileSync(builderPath(project), 'utf8')).toBe(reference)
    }
  })

  it('keeps the ten plugin build scripts free of repository-escape references', () => {
    for (const plugin of catalog) {
      const pluginDirectory = resolve(repositoryRoot, 'plugins', plugin.id)
      const packageJson = JSON.parse(
        readFileSync(resolve(pluginDirectory, 'package.json'), 'utf8')
      ) as PackageMetadata
      const scannedScripts = ['build', 'clean', 'typecheck', 'watch']
      const combined = scannedScripts
        .map((name) => packageJson.scripts?.[name])
        .filter((script): script is string => typeof script === 'string')
        .join('\n')
      expect(combined).not.toContain('../../')

      const configPath = resolve(pluginDirectory, 'esbuild.config.mjs')
      if (existsSync(configPath)) {
        expect(readFileSync(configPath, 'utf8')).not.toContain('../../')
      }
    }
  })
})
