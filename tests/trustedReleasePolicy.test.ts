import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')

function read(path: string): string {
  // Normalize CRLF/LF so assertions stay valid regardless of the
  // line endings used by the checked-out files (e.g. electron-builder.yml).
  return readFileSync(resolve(root, path), 'utf8').replace(/\r\n/g, '\n')
}

describe('Windows release and update policy', () => {
  it('supports only unsigned Windows x64 NSIS artifacts', () => {
    const builder = read('electron-builder.yml')

    expect(builder).toContain('signExecutable: false')
    expect(builder).toContain('target: nsis')
    expect(builder).toContain('arch:\n        - x64')
    expect(builder).not.toMatch(/^mac:/mu)
    expect(builder).not.toMatch(/^linux:/mu)
    expect(builder).not.toContain('target: dmg')
    expect(builder).not.toContain('target: AppImage')
  })

  it('generates public GitHub update metadata without requiring a platform certificate', () => {
    const releaseBuilder = read('electron-builder.release.yml')

    expect(releaseBuilder).toContain('forceCodeSigning: false')
    expect(releaseBuilder).toContain('provider: github')
    expect(releaseBuilder).toContain('channel: latest')
    expect(releaseBuilder).not.toContain('notarize:')
    expect(releaseBuilder).not.toContain('entitlements.mac')
  })

  it('builds, verifies and attests a Windows release before publishing the tag', () => {
    const workflow = read('.github/workflows/release.yml')

    expect(workflow).toContain('runs-on: windows-latest')
    expect(workflow).toContain('Resolve release channel')
    expect(workflow).toContain('channel=latest')
    expect(workflow).toContain('channel=beta')
    expect(workflow).toContain('verify-windows-update-artifacts.mjs')
    expect(workflow).toContain("$signature.Status -ne 'NotSigned'")
    expect(workflow).toContain('npm run smoke:packaged')
    expect(workflow).toContain('npm run smoke:installer')
    expect(workflow).toMatch(/actions\/attest@[a-f0-9]{40} # v4/u)
    expect(workflow).not.toContain('WIN_CSC_LINK')
    expect(workflow).not.toContain('macos-latest')
    expect(workflow).not.toContain('ubuntu-latest')
    expect(workflow).toContain('gh release create')
  })

  it('pins the updater runtime and emits GitHub NSIS metadata', () => {
    const packageJson = JSON.parse(read('package.json')) as {
      dependencies: Record<string, string>
      scripts: Record<string, string>
    }

    expect(packageJson.dependencies['electron-updater']).toBe('6.8.9')
    expect(packageJson.scripts['package:dir']).toContain('signAndEditExecutable=false')
    expect(packageJson.scripts['package:release']).toContain('electron-builder.release.yml')
    expect(packageJson.scripts['test:supply-chain']).toContain(
      'verify-windows-update-artifacts.test.mjs'
    )
  })

  it('keeps repository-free packages offline-capable without updater network access', () => {
    const builder = read('electron-builder.yml')
    const releaseBuilder = read('electron-builder.release.yml')
    const main = read('electron/main.ts')
    const service = read('electron/AppUpdateService.ts')

    expect(builder).not.toMatch(/^publish:/mu)
    expect(releaseBuilder).toMatch(/^publish:/mu)
    expect(main).toContain("existsSync(join(process.resourcesPath, 'app-update.yml'))")
    expect(service).toContain('this.options.packaged && this.options.configured')
    expect(service).toContain('all offline features remain available')
  })

  it('rehearses upgrade and rollback on Windows against the same isolated user data', () => {
    const workflow = read('.github/workflows/release-compatibility.yml')
    const script = read('scripts/smoke-release-transition.mjs')

    expect(workflow).toContain('runs-on: windows-latest')
    expect(workflow).not.toContain('ubuntu-latest')
    expect(workflow).not.toContain('macos-latest')
    expect(workflow).toContain('previous -> candidate -> previous')
    expect(script).toContain("'release-transition-marker'")
    expect(script).toContain("database.exec('PRAGMA user_version')")
    expect(script).toContain("OPENBOX_SMOKE_TEST: '1'")
  })

  it('runs ordinary CI exclusively on Windows and pins every first-party action', () => {
    for (const path of [
      '.github/workflows/ci.yml',
      '.github/workflows/release.yml',
      '.github/workflows/release-compatibility.yml'
    ]) {
      const workflow = read(path)
      expect(workflow, path).toContain('windows-latest')
      expect(workflow, path).not.toContain('ubuntu-latest')
      expect(workflow, path).not.toContain('macos-latest')
      expect(workflow, path).not.toMatch(/uses: actions\/[\w-]+@v\d/u)
      for (const action of workflow.matchAll(/uses: actions\/[\w-]+@([^\s#]+)/gu)) {
        expect(action[1], `${path}: ${action[0]}`).toMatch(/^[a-f0-9]{40}$/u)
      }
    }
  })
})
