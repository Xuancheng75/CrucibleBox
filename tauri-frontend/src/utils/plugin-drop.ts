/**
 * 全局文件拖放只处理带真实路径的外部拖拽。
 * dnd-kit 的卡片排序没有 OS 文件路径，内部状态也会显式标记，双重条件
 * 让插件排序不会误触发 ZIP 安装或 Document Engine 导入。
 */
export function shouldHandleGlobalFileDrop(
  internalPluginDragActive: boolean,
  paths: readonly string[]
): boolean {
  return !internalPluginDragActive && paths.some((path) => path.trim().length > 0)
}
