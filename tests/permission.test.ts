import { describe, it, expect } from 'vitest'
import { PermissionGuard } from '../plugin-system/PermissionGuard'
import { Permission, ALL_PERMISSIONS } from '../shared/types/permissions'

describe('PermissionGuard', () => {
  it('has() returns true only for granted permissions', () => {
    const guard = new PermissionGuard([Permission.DatabaseRead, Permission.NetworkFetch])
    expect(guard.has(Permission.DatabaseRead)).toBe(true)
    expect(guard.has(Permission.NetworkFetch)).toBe(true)
    expect(guard.has(Permission.ShellExec)).toBe(false)
    expect(guard.has(Permission.FileWrite)).toBe(false)
  })

  it('assert() throws with permission message when not granted', () => {
    const guard = new PermissionGuard([])
    expect(() => guard.assert(Permission.Notification)).toThrow(/Permission denied/)
  })

  it('assert() does not throw when permission granted', () => {
    const guard = new PermissionGuard([Permission.DatabaseWrite])
    expect(() => guard.assert(Permission.DatabaseWrite)).not.toThrow()
  })

  it('checkAll() partitions granted and denied permissions', () => {
    const guard = new PermissionGuard([Permission.Clipboard, Permission.ThemeWrite])
    const { granted, denied } = guard.checkAll([
      Permission.Clipboard,
      Permission.Dialog,
      Permission.ThemeWrite
    ])
    expect(granted).toEqual([Permission.Clipboard, Permission.ThemeWrite])
    expect(denied).toEqual([Permission.Dialog])
  })
})

describe('PermissionGuard static helpers', () => {
  it('validatePermission accepts only known permissions', () => {
    expect(PermissionGuard.validatePermission(Permission.NetworkFetch)).toBe(true)
    expect(PermissionGuard.validatePermission('not:a:perm')).toBe(false)
    expect(PermissionGuard.validatePermission('')).toBe(false)
  })

  it('parsePermissions drops unknown entries and keeps valid ones', () => {
    const parsed = PermissionGuard.parsePermissions([
      Permission.FileRead,
      'bogus:perm',
      Permission.ShellExec
    ])
    expect(parsed).toEqual([Permission.FileRead, Permission.ShellExec])
  })

  it('parsePermissions returns empty array for empty input', () => {
    expect(PermissionGuard.parsePermissions([])).toEqual([])
  })

  it('ALL_PERMISSIONS contains every enum value', () => {
    expect(ALL_PERMISSIONS.length).toBe(Object.keys(Permission).length)
    expect(ALL_PERMISSIONS).toContain(Permission.ShellExec)
  })
})
