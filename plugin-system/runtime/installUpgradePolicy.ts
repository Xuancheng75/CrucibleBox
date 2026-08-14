// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import { compareVersions } from '../semver'

/**
 * 插件版本升级门禁（安装与预览共用）。
 *
 * 将 PluginInstallationService 与 PluginInstallPreparation 中重复的
 * "拒绝降级 / 拒绝同版本覆盖"判断收敛到单一实现。
 *
 * @param pluginName 插件名（用于错误消息）
 * @param incomingVersion 待安装版本
 * @param existingVersion 已安装版本
 * @param sameVersionMessage 同版本时追加的提示（如 "，如需覆盖请先卸载"），可为空字符串
 */
export function assertPluginUpgradeAllowed(
  pluginName: string,
  incomingVersion: string,
  existingVersion: string,
  sameVersionMessage: string
): void {
  const comparison = compareVersions(incomingVersion, existingVersion)
  if (comparison < 0) {
    throw new Error(`插件 "${pluginName}" 已安装更高版本 ${existingVersion}，无法降级`)
  }
  if (comparison === 0) {
    throw new Error(`插件 "${pluginName}" 已安装（版本 ${existingVersion}）${sameVersionMessage}`)
  }
}
