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

function getJavaUrls(
  major: string,
  version: string,
  archiveName: string,
  releaseTag: string,
  mirror?: string
): Array<{ url: string; label: string }> {
  const urls: Array<{ url: string; label: string }> = []

  if (mirror === 'tuna') {
    urls.push({
      url: `https://mirrors.tuna.tsinghua.edu.cn/github-release/adoptium/temurin${major}-binaries/${encodeURIComponent(releaseTag)}/${archiveName}`,
      label: 'JDK (TUNA)'
    })
  }

  // 官方 GitHub 发布源 (总是作为兜底)
  urls.push({
    url: getOfficialToolArtifactUrl('java', version),
    label: 'JDK (官方)'
  })

  return urls
}

function extractVersion(raw: string): string | null {
  const m = raw.match(/(\d+\.\d+\.\d+)[._]?(\d+)?/)
  return m ? m[1] : null
}

export const javaTool: ToolDef = {
  id: 'java',
  displayName: 'Java JDK',
  icon: '\u2615',
  description: 'Java 开发工具包 (Eclipse Adoptium)',

  async detect(installRoot: string): Promise<ToolInfo> {
    try {
      const { stdout, stderr } = await runProcess('java', ['-version'], { timeoutMs: 10_000 })
      const v = extractVersion(stderr || stdout)
      if (v) return { installed: true, version: v, path: '' }
    } catch {
      // not on PATH
    }

    const link = join(toolDir(installRoot, 'java'), 'current')
    const javaExe = join(link, 'bin', 'java.exe')
    if (existsSync(javaExe)) {
      try {
        const { stdout, stderr } = await runProcess(javaExe, ['-version'], { timeoutMs: 10_000 })
        const v = extractVersion(stderr || stdout)
        if (v) return { installed: true, version: v, path: link }
      } catch {
        // ignore
      }
    }

    return { installed: false }
  },

  async listVersions(): Promise<string[]> {
    return [...SUPPORTED_TOOL_VERSIONS.java]
  },

  async install(
    version: string,
    installRoot: string,
    onProgress: ProgressCallback,
    opts?: InstallOptions
  ): Promise<void> {
    const artifact = getToolArtifactIntegrity('java', version)
    if (!artifact.releaseTag) throw new Error(`JDK ${version} 的发布标签未维护`)

    const dir = versionDir(installRoot, 'java', version)
    if (!existsSync(dir)) {
      mkdirSync(dir, { recursive: true })
    }

    const major = version.split('.')[0]
    const finalDir = join(dir, 'jdk')
    if (existsSync(finalDir)) {
      throw new Error(`JDK ${version} 的运行时目录已存在，拒绝覆盖`)
    }
    const stagingDir = createInstallStagingDir(dir)
    const archiveName = artifact.filename
    const zipPath = join(stagingDir, archiveName)
    const extractDir = join(stagingDir, 'extracted')
    try {
      onProgress({ stage: 'downloading', percent: 0, message: `正在下载 JDK ${version}...` })
      await downloadWithFallback(
        getJavaUrls(major, version, archiveName, artifact.releaseTag, opts?.downloadMirror),
        zipPath,
        onProgress,
        artifact.sha256,
        opts?.signal
      )

      onProgress({ stage: 'installing', percent: 95, message: `正在解压 JDK ${version}...` })
      await extractZip(zipPath, extractDir, opts?.signal)

      const jdkSrcDir = findTopDir(extractDir)
      promoteStagedRuntime(dir, stagingDir, jdkSrcDir, finalDir)
    } finally {
      cleanupInstallStagingDir(dir, stagingDir)
    }

    onProgress({ stage: 'configuring', percent: 98, message: '正在创建目录链接...' })
    const td = toolDir(installRoot, 'java')
    if (!existsSync(td)) {
      mkdirSync(td, { recursive: true })
    }
    const link = currentLink(installRoot, 'java')
    await createJunction(link, finalDir)

    onProgress({ stage: 'done', percent: 100, message: `JDK ${version} 安装完成` })
  },

  async uninstall(installRoot: string, onProgress: ProgressCallback): Promise<void> {
    onProgress({ stage: 'configuring', percent: 0, message: '正在卸载...' })
    const link = currentLink(installRoot, 'java')
    await removeJunction(link)
    onProgress({ stage: 'done', percent: 100, message: 'JDK 已卸载' })
  },

  async switchVersion(version: string, installRoot: string): Promise<void> {
    const dir = join(versionDir(installRoot, 'java', version), 'jdk')
    if (!existsSync(dir)) {
      throw new Error(`JDK ${version} 未安装`)
    }
    const td = toolDir(installRoot, 'java')
    const link = join(td, 'current')
    await createJunction(link, dir)
  }
}
