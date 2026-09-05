/**
 * 计算批量选择后的插件组排序结果。
 *
 * 从当前顺序中移除整组，再按拖动方向把整组插入目标项之前或之后；组内顺序
 * 始终沿用当前列表顺序，避免拖动时因选择顺序不同而重新排列插件。
 */
export interface GroupReorderResult {
  orderedIds: string[]
  selectedIds: string[]
  changed: boolean
}

export function reorderPluginGroup(
  currentIds: string[],
  selectedIds: string[],
  activeId: string,
  overId: string
): GroupReorderResult | null {
  if (currentIds.length === 0 || !currentIds.includes(activeId) || !currentIds.includes(overId)) {
    return null
  }

  const selectedSet = new Set(selectedIds.filter((id) => currentIds.includes(id)))
  // 长按未选卡片时，先把它加入拖动组；组内顺序仍由 currentIds 决定。
  selectedSet.add(activeId)
  const orderedSelectedIds = currentIds.filter((id) => selectedSet.has(id))

  // 指针仍停留在选中组上时不改变顺序。这样长按启动后松手不会把
  // 原本不相邻的选中项意外压缩到一起；拖到未选目标项才会产生移动。
  if (selectedSet.has(overId)) {
    return {
      orderedIds: [...currentIds],
      selectedIds: orderedSelectedIds,
      changed: false
    }
  }

  const remainingIds = currentIds.filter((id) => !selectedSet.has(id))

  const targetIndex = remainingIds.indexOf(overId)

  if (targetIndex < 0) return null

  // 保持单插件排序原有的 arrayMove 语义：从上往下拖放到目标项后方，
  // 从下往上拖放到目标项前方。多选组沿用同一方向规则。
  const activeIndex = currentIds.indexOf(activeId)
  const overIndex = currentIds.indexOf(overId)
  const insertIndex = activeIndex < overIndex ? targetIndex + 1 : targetIndex
  const orderedIds = [
    ...remainingIds.slice(0, insertIndex),
    ...orderedSelectedIds,
    ...remainingIds.slice(insertIndex)
  ]

  return {
    orderedIds,
    selectedIds: orderedSelectedIds,
    changed: orderedIds.some((id, index) => id !== currentIds[index])
  }
}
