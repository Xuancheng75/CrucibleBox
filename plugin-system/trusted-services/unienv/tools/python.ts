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
  versionDir
} from './base'

function getPythonUrls(
  version: string,
  installerName: string,
  mirror?: string
): Array<{ url: string; label: string }> {
  const urls: Array<{ url: string; label: string }> = []

  if (mirror === 'huawei') {
    urls.push({
      url: `https://mirrors.huaweicloud.com/python/${version}/${installerName}`,
      label: 'Python (华为云)'
    })
  }
  if (mirror === 'tuna') {
    urls.push({
      url: `https://mirrors.tuna.tsinghua.edu.cn/python/${version}/${installerName}`,
      label: 'Python (TUNA)'
    })
  }
  // 官方源兜底
  urls.push({
    url: getOfficialToolArtifactUrl('python', version),
    label: 'Python (官方)'
  })

  return urls
}

function extractVersion(raw: string): string | null {
  const m = raw.match(/Python\s+(\d+\.\d+\.\d+)/)
  return m ? m[1] : null
}

export const pythonTool: ToolDef = {
  id: 'python',
  displayName: 'Python',
  icon: '\uD83D\uDC0D',
  description: 'Python 编程语言运行时',

  async detect(installRoot: string): Promise<ToolInfo> {
    for (const cmd of ['python', 'python3']) {
      try {
        const { stdout } = await runProcess(cmd, ['--version'], { timeoutMs: 10_000 })
        const v = extractVersion(stdout)
        if (v) return { installed: true, version: v, path: '' }
      } catch {
        // try next
      }
    }

    const td = toolDir(installRoot, 'python')
    const link = join(td, 'current')
    if (existsSync(link)) {
      try {
        const { stdout } = await runProcess(join(link, 'python.exe'), ['--version'], {
          timeoutMs: 10_000
        })
        const v = extractVersion(stdout)
        if (v) return { installed: true, version: v, path: link }
      } catch {
        // ignore
      }
    }

    return { installed: false }
  },

  async listVersions(): Promise<string[]> {
    return [...SUPPORTED_TOOL_VERSIONS.python]
  },

  async install(
    version: string,
    installRoot: string,
    onProgress: ProgressCallback,
    opts?: InstallOptions
  ): Promise<void> {
    const dir = versionDir(installRoot, 'python', version)
    prepareDirectInstallDirectory(dir, `Python ${version}`)

    const artifact = getToolArtifactIntegrity('python', version)
    const installerName = artifact.filename
    const installerPath = join(dir, installerName)

    onProgress({ stage: 'downloading', percent: 0, message: `正在下载 Python ${version}...` })
    await downloadWithFallback(
      getPythonUrls(version, installerName, opts?.downloadMirror),
      installerPath,
      onProgress,
      artifact.sha256,
      opts?.signal
    )

    onProgress({ stage: 'installing', percent: 95, message: `正在安装 Python ${version}...` })
    try {
      await runProcess(
        installerPath,
        ['/quiet', 'InstallAllUsers=0', `TargetDir=${dir}`, 'PrependPath=0', 'Include_test=0'],
        { timeoutMs: 600_000, signal: opts?.signal }
      )
    } finally {
      cleanupFile(installerPath)
    }

    onProgress({ stage: 'configuring', percent: 98, message: '正在创建目录链接...' })
    const td = toolDir(installRoot, 'python')
    if (!existsSync(td)) {
      mkdirSync(td, { recursive: true })
    }
    const link = join(td, 'current')
    await createJunction(link, dir)

    onProgress({ stage: 'done', percent: 100, message: `Python ${version} 安装完成` })
  },

  async uninstall(installRoot: string, onProgress: ProgressCallback): Promise<void> {
    onProgress({ stage: 'configuring', percent: 0, message: '正在卸载...' })
    const td = toolDir(installRoot, 'python')
    const link = join(td, 'current')
    await removeJunction(link)
    onProgress({ stage: 'done', percent: 100, message: 'Python 已卸载' })
  },

  async switchVersion(version: string, installRoot: string): Promise<void> {
    const dir = versionDir(installRoot, 'python', version)
    if (!existsSync(dir)) {
      throw new Error(`Python ${version} 未安装`)
    }
    const td = toolDir(installRoot, 'python')
    const link = join(td, 'current')
    await createJunction(link, dir)
  }
}
