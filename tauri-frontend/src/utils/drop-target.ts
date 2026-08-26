/**
 * 全窗口拖拽导入的路径解析（纯函数，便于测试）。
 *
 * 规则（1.9.12 产品决策）：
 * - 一次拖入多个文件时，收集全部 .zip 进入批量流程
 * - 没有任何 .zip 时，取第一个路径按「插件目录」尝试（后端校验失败给出明确错误）
 */

export interface ResolvedDrop {
  kind: 'zip' | 'directory'
  /** 参与安装的路径（单个 zip 或目录） */
  targets: string[]
  /** 被忽略的 .zip 数量（>0 时 UI 提示） */
  ignoredZips: number
}

export function isZipPath(path: string): boolean {
  return path.toLowerCase().endsWith('.zip')
}

export function resolveDropPaths(paths: string[]): ResolvedDrop | null {
  const cleaned = paths.map((p) => p.trim()).filter(Boolean)
  if (cleaned.length === 0) return null

  const zips = cleaned.filter(isZipPath)
  if (zips.length > 0) {
    return { kind: 'zip', targets: zips, ignoredZips: Math.max(0, cleaned.length - zips.length) }
  }

  // 无 zip：按目录尝试（仅取第一个；拖入普通文件时由后端校验报错）
  return { kind: 'directory', targets: [cleaned[0]], ignoredZips: 0 }
}
