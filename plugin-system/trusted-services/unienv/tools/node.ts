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

function extractVersion(raw: string): string | null {
  const m = raw.match(/v?(\d+\.\d+\.\d+)/)
  return m ? m[1] : null
}

function getNodeUrls(
  version: string,
  zipName: string,
  mirror?: string
): Array<{ url: string; label: string }> {
  const urls: Array<{ url: string; label: string }> = []

  if (mirror === 'huawei') {
    urls.push({
      url: `https://mirrors.huaweicloud.com/nodejs/v${version}/${zipName}`,
      label: 'Node.js (华为云)'
    })
  }
  if (mirror === 'tuna') {
    urls.push({
      url: `https://mirrors.tuna.tsinghua.edu.cn/nodejs-release/v${version}/${zipName}`,
      label: 'Node.js (TUNA)'
    })
  }
  // 淘宝 NPM 镜像（已验证可用）
  urls.push({
    url: `https://npmmirror.com/mirrors/node/v${version}/${zipName}`,
    label: 'Node.js (淘宝NPM)'
  })
  // 官方源兜底
  urls.push({ url: getOfficialToolArtifactUrl('node', version), label: 'Node.js (官方)' })

  return urls
}

export const nodeTool: ToolDef = {
  id: 'node',
  displayName: 'Node.js',
  icon: '\uD83D\uDFE2',
  description: 'Node.js 运行时与 npm 包管理器',

  async detect(installRoot: string): Promise<ToolInfo> {
    try {
      const { stdout } = await runProcess('node', ['--version'], { timeoutMs: 10_000 })
      const v = extractVersion(stdout)
      if (v) return { installed: true, version: v, path: '' }
    } catch {
      // not on PATH
    }

    const link = join(toolDir(installRoot, 'node'), 'current')
    if (existsSync(join(link, 'node.exe'))) {
      try {
        const { stdout } = await runProcess(join(link, 'node.exe'), ['--version'], {
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
    return [...SUPPORTED_TOOL_VERSIONS.node]
  },

  async install(
    version: string,
    installRoot: string,
    onProgress: ProgressCallback,
    opts?: InstallOptions
  ): Promise<void> {
    const dir = versionDir(installRoot, 'node', version)
    if (!existsSync(dir)) {
      mkdirSync(dir, { recursive: true })
    }

    const finalDir = join(dir, 'runtime')
    if (existsSync(finalDir)) {
      throw new Error(`Node.js ${version} 的运行时目录已存在，拒绝覆盖`)
    }
    const stagingDir = createInstallStagingDir(dir)
    const artifact = getToolArtifactIntegrity('node', version)
    const zipName = artifact.filename
    const zipPath = join(stagingDir, zipName)
    const extractDir = join(stagingDir, 'extracted')
    try {
      onProgress({ stage: 'downloading', percent: 0, message: `正在下载 Node.js ${version}...` })
      await downloadWithFallback(
        getNodeUrls(version, zipName, opts?.downloadMirror),
        zipPath,
        onProgress,
        artifact.sha256,
        opts?.signal
      )

      onProgress({ stage: 'installing', percent: 95, message: `正在解压 Node.js ${version}...` })
      await extractZip(zipPath, extractDir, opts?.signal)

      const srcDir = findTopDir(extractDir)
      promoteStagedRuntime(dir, stagingDir, srcDir, finalDir)
    } finally {
      cleanupInstallStagingDir(dir, stagingDir)
    }

    onProgress({ stage: 'configuring', percent: 98, message: '正在创建目录链接...' })
    const td = toolDir(installRoot, 'node')
    if (!existsSync(td)) {
      mkdirSync(td, { recursive: true })
    }
    const link = currentLink(installRoot, 'node')
    await createJunction(link, finalDir)

    onProgress({ stage: 'done', percent: 100, message: `Node.js ${version} 安装完成` })
  },

  async uninstall(installRoot: string, onProgress: ProgressCallback): Promise<void> {
    onProgress({ stage: 'configuring', percent: 0, message: '正在卸载...' })
    const link = currentLink(installRoot, 'node')
    await removeJunction(link)
    onProgress({ stage: 'done', percent: 100, message: 'Node.js 已卸载' })
  },

  async switchVersion(version: string, installRoot: string): Promise<void> {
    const dir = join(versionDir(installRoot, 'node', version), 'runtime')
    if (!existsSync(dir)) {
      throw new Error(`Node.js ${version} 未安装`)
    }
    const td = toolDir(installRoot, 'node')
    const link = join(td, 'current')
    await createJunction(link, dir)
  }
}
