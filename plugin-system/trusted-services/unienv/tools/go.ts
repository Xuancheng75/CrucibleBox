// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import { existsSync, mkdirSync } from 'fs'
import { join } from 'path'
import { runProcess } from '../process-runner'
import { SUPPORTED_TOOL_VERSIONS } from '../protocol'
import { getOfficialToolArtifactUrl, getToolArtifactIntegrity } from '../artifact-integrity'
import {
  downloadWithFallback,
  extractZip,
  createJunction,
  removeJunction,
  createInstallStagingDir,
  cleanupInstallStagingDir,
  promoteStagedRuntime,
  findTopDir,
  type ToolDef,
  type ToolInfo,
  type ProgressCallback,
  type InstallOptions,
  toolDir,
  versionDir,
  currentLink
} from './base'

function getGoUrls(
  version: string,
  archiveName: string,
  mirror?: string
): Array<{ url: string; label: string }> {
  const urls: Array<{ url: string; label: string }> = []

  if (mirror === 'aliyun') {
    urls.push({ url: `https://mirrors.aliyun.com/golang/${archiveName}`, label: 'Go (阿里云)' })
  }
  // Google中国镜像
  urls.push({ url: `https://golang.google.cn/dl/${archiveName}`, label: 'Go (Google中国)' })
  // 官方源兜底
  urls.push({ url: getOfficialToolArtifactUrl('go', version), label: 'Go (官方)' })

  return urls
}

function extractVersion(raw: string): string | null {
  const m = raw.match(/go(\d+\.\d+\.\d+)/)
  return m ? m[1] : null
}

export const goTool: ToolDef = {
  id: 'go',
  displayName: 'Go',
  icon: '\uD83D\uDD35',
  description: 'Go 编程语言运行时',

  async detect(installRoot: string): Promise<ToolInfo> {
    try {
      const { stdout } = await runProcess('go', ['version'], { timeoutMs: 10_000 })
      const v = extractVersion(stdout)
      if (v) return { installed: true, version: v, path: '' }
    } catch {
      // not on PATH
    }

    const link = join(toolDir(installRoot, 'go'), 'current')
    const goExe = join(link, 'bin', 'go.exe')
    if (existsSync(goExe)) {
      try {
        const { stdout } = await runProcess(goExe, ['version'], { timeoutMs: 10_000 })
        const v = extractVersion(stdout)
        if (v) return { installed: true, version: v, path: link }
      } catch {
        // ignore
      }
    }

    return { installed: false }
  },

  async listVersions(): Promise<string[]> {
    return [...SUPPORTED_TOOL_VERSIONS.go]
  },

  async install(
    version: string,
    installRoot: string,
    onProgress: ProgressCallback,
    opts?: InstallOptions
  ): Promise<void> {
    const dir = versionDir(installRoot, 'go', version)
    if (!existsSync(dir)) {
      mkdirSync(dir, { recursive: true })
    }

    const finalDir = join(dir, 'go')
    if (existsSync(finalDir)) {
      throw new Error(`Go ${version} 的运行时目录已存在，拒绝覆盖`)
    }
    const stagingDir = createInstallStagingDir(dir)
    const artifact = getToolArtifactIntegrity('go', version)
    const archiveName = artifact.filename
    const zipPath = join(stagingDir, archiveName)
    const extractDir = join(stagingDir, 'extracted')
    try {
      onProgress({ stage: 'downloading', percent: 0, message: `正在下载 Go ${version}...` })
      await downloadWithFallback(
        getGoUrls(version, archiveName, opts?.downloadMirror),
        zipPath,
        onProgress,
        artifact.sha256,
        opts?.signal
      )

      onProgress({ stage: 'installing', percent: 95, message: `正在解压 Go ${version}...` })
      await extractZip(zipPath, extractDir, opts?.signal)

      const goSrcDir = findTopDir(extractDir)
      promoteStagedRuntime(dir, stagingDir, goSrcDir, finalDir)
    } finally {
      cleanupInstallStagingDir(dir, stagingDir)
    }

    onProgress({ stage: 'configuring', percent: 98, message: '正在创建目录链接...' })
    const td = toolDir(installRoot, 'go')
    if (!existsSync(td)) {
      mkdirSync(td, { recursive: true })
    }
    const link = currentLink(installRoot, 'go')
    await createJunction(link, finalDir)

    onProgress({ stage: 'done', percent: 100, message: `Go ${version} 安装完成` })
  },

  async uninstall(installRoot: string, onProgress: ProgressCallback): Promise<void> {
    onProgress({ stage: 'configuring', percent: 0, message: '正在卸载...' })
    const link = currentLink(installRoot, 'go')
    await removeJunction(link)
    onProgress({ stage: 'done', percent: 100, message: 'Go 已卸载' })
  },

  async switchVersion(version: string, installRoot: string): Promise<void> {
    const dir = join(versionDir(installRoot, 'go', version), 'go')
    if (!existsSync(dir)) {
      throw new Error(`Go ${version} 未安装`)
    }
    const td = toolDir(installRoot, 'go')
    const link = join(td, 'current')
    await createJunction(link, dir)
  }
}
