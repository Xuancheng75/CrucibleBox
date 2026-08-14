// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import { existsSync, mkdirSync } from 'fs'
import { join } from 'path'
import { runProcess } from '../process-runner'
import { SUPPORTED_TOOL_VERSIONS } from '../protocol'
import { getOfficialToolArtifactUrl, getToolArtifactIntegrity } from '../artifact-integrity'
import {
  downloadWithFallback,
  createJunction,
  removeJunction,
  cleanupFile,
  prepareDirectInstallDirectory,
  type ToolDef,
  type ToolInfo,
  type ProgressCallback,
  type InstallOptions,
  toolDir,
  versionDir,
  currentLink
} from './base'

function getGitUrls(
  version: string,
  installerName: string,
  releaseTag: string,
  mirror?: string
): Array<{ url: string; label: string }> {
  const urls: Array<{ url: string; label: string }> = []

  if (mirror === 'tuna') {
    urls.push({
      url: `https://mirrors.tuna.tsinghua.edu.cn/github-release/git-for-windows/git/${releaseTag}/${installerName}`,
      label: 'Git (TUNA)'
    })
  }
  // 官方源兜底
  urls.push({
    url: getOfficialToolArtifactUrl('git', version),
    label: 'Git (官方)'
  })

  return urls
}

function extractVersion(raw: string): string | null {
  const m = raw.match(/git\s+version\s+(\d+\.\d+\.\d+)/)
  return m ? m[1] : null
}

export const gitTool: ToolDef = {
  id: 'git',
  displayName: 'Git',
  icon: '\uD83D\uDD27',
  description: 'Git 分布式版本控制系统',

  async detect(installRoot: string): Promise<ToolInfo> {
    try {
      const { stdout } = await runProcess('git', ['--version'], { timeoutMs: 10_000 })
      const v = extractVersion(stdout)
      if (v) return { installed: true, version: v, path: '' }
    } catch {
      // not on PATH
    }

    const link = join(toolDir(installRoot, 'git'), 'current')
    const gitExe = join(link, 'bin', 'git.exe')
    if (existsSync(gitExe)) {
      try {
        const { stdout } = await runProcess(gitExe, ['--version'], { timeoutMs: 10_000 })
        const v = extractVersion(stdout)
        if (v) return { installed: true, version: v, path: link }
      } catch {
        // ignore
      }
    }

    return { installed: false }
  },

  async listVersions(): Promise<string[]> {
    return [...SUPPORTED_TOOL_VERSIONS.git]
  },

  async install(
    version: string,
    installRoot: string,
    onProgress: ProgressCallback,
    opts?: InstallOptions
  ): Promise<void> {
    const artifact = getToolArtifactIntegrity('git', version)
    if (!artifact.releaseTag) throw new Error(`Git ${version} 的发布标签未维护`)

    const dir = versionDir(installRoot, 'git', version)
    prepareDirectInstallDirectory(dir, `Git ${version}`)

    const installerName = artifact.filename
    const installerPath = join(dir, installerName)

    onProgress({ stage: 'downloading', percent: 0, message: `正在下载 Git ${version}...` })
    await downloadWithFallback(
      getGitUrls(version, installerName, artifact.releaseTag, opts?.downloadMirror),
      installerPath,
      onProgress,
      artifact.sha256,
      opts?.signal
    )

    onProgress({ stage: 'installing', percent: 95, message: `正在安装 Git ${version}...` })
    try {
      await runProcess(
        installerPath,
        ['/VERYSILENT', `/DIR=${dir}`, '/NORESTART', '/NOCANCEL', '/SP-', '/NOICONS'],
        { timeoutMs: 600_000, signal: opts?.signal }
      )
    } finally {
      cleanupFile(installerPath)
    }

    onProgress({ stage: 'configuring', percent: 98, message: '正在创建目录链接...' })
    const td = toolDir(installRoot, 'git')
    if (!existsSync(td)) {
      mkdirSync(td, { recursive: true })
    }
    const link = currentLink(installRoot, 'git')
    await createJunction(link, dir)

    onProgress({ stage: 'done', percent: 100, message: `Git ${version} 安装完成` })
  },

  async uninstall(installRoot: string, onProgress: ProgressCallback): Promise<void> {
    onProgress({ stage: 'configuring', percent: 0, message: '正在卸载...' })
    const link = currentLink(installRoot, 'git')
    await removeJunction(link)
    onProgress({ stage: 'done', percent: 100, message: 'Git 已卸载' })
  },

  async switchVersion(version: string, installRoot: string): Promise<void> {
    const dir = versionDir(installRoot, 'git', version)
    if (!existsSync(dir)) {
      throw new Error(`Git ${version} 未安装`)
    }
    const td = toolDir(installRoot, 'git')
    const link = join(td, 'current')
    await createJunction(link, dir)
  }
}
