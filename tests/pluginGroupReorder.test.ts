import { describe, expect, it } from 'vitest'
import { reorderPluginGroup } from '../tauri-frontend/src/utils/plugin-reorder'
import { shouldHandleGlobalFileDrop } from '../tauri-frontend/src/utils/plugin-drop'

describe('Tauri plugin group reorder', () => {
  const ids = ['a', 'b', 'c', 'd', 'e']

  it('moves a selected group after a lower target and preserves group order', () => {
    expect(reorderPluginGroup(ids, ['b', 'd'], 'b', 'e')).toEqual({
      orderedIds: ['a', 'c', 'e', 'b', 'd'],
      selectedIds: ['b', 'd'],
      changed: true
    })
  })

  it('matches single-plugin arrayMove direction semantics', () => {
    expect(reorderPluginGroup(ids, [], 'b', 'c')).toEqual({
      orderedIds: ['a', 'c', 'b', 'd', 'e'],
      selectedIds: ['b'],
      changed: true
    })
    expect(reorderPluginGroup(ids, [], 'd', 'b')).toEqual({
      orderedIds: ['a', 'd', 'b', 'c', 'e'],
      selectedIds: ['d'],
      changed: true
    })
  })

  it('moves a selected group to the beginning when dragged upward', () => {
    expect(reorderPluginGroup(ids, ['c', 'd'], 'd', 'a')).toEqual({
      orderedIds: ['c', 'd', 'a', 'b', 'e'],
      selectedIds: ['c', 'd'],
      changed: true
    })
  })

  it('adds a long-pressed unselected card to the dragged group', () => {
    expect(reorderPluginGroup(ids, ['b'], 'e', 'a')).toEqual({
      orderedIds: ['b', 'e', 'a', 'c', 'd'],
      selectedIds: ['b', 'e'],
      changed: true
    })
  })

  it('returns an unchanged result when the group is already at the target', () => {
    expect(reorderPluginGroup(ids, ['a', 'b'], 'a', 'b')).toEqual({
      orderedIds: ids,
      selectedIds: ['a', 'b'],
      changed: false
    })
  })

  it('does not compress a non-contiguous selection when dropping on a selected card', () => {
    expect(reorderPluginGroup(ids, ['b', 'd'], 'b', 'b')).toEqual({
      orderedIds: ids,
      selectedIds: ['b', 'd'],
      changed: false
    })
  })

  it('rejects unknown active or target IDs', () => {
    expect(reorderPluginGroup(ids, ['a'], 'missing', 'b')).toBeNull()
    expect(reorderPluginGroup(ids, ['a'], 'a', 'missing')).toBeNull()
  })

  it('separates internal card drags from external file drops', () => {
    expect(shouldHandleGlobalFileDrop(true, ['C:/plugin.zip'])).toBe(false)
    expect(shouldHandleGlobalFileDrop(false, [])).toBe(false)
    expect(shouldHandleGlobalFileDrop(false, ['  ', 'C:/Docs/report.pdf'])).toBe(true)
  })
})
