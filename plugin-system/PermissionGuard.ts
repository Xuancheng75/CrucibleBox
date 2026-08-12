import { Permission, ALL_PERMISSIONS } from '@shared/types/permissions'

export class PermissionGuard {
  private granted: Set<Permission>

  constructor(permissions: Permission[]) {
    this.granted = new Set(permissions)
  }

  has(permission: Permission): boolean {
    return this.granted.has(permission)
  }

  assert(permission: Permission): void {
    if (!this.has(permission)) {
      throw new Error(`Permission denied: ${permission}`)
    }
  }

  checkAll(permissions: Permission[]): { granted: Permission[]; denied: Permission[] } {
    const granted: Permission[] = []
    const denied: Permission[] = []
    for (const perm of permissions) {
      if (this.has(perm)) {
        granted.push(perm)
      } else {
        denied.push(perm)
      }
    }
    return { granted, denied }
  }

  static validatePermission(perm: string): perm is Permission {
    return (ALL_PERMISSIONS as string[]).includes(perm)
  }

  static parsePermissions(raw: string[]): Permission[] {
    return raw.filter(PermissionGuard.validatePermission)
  }
}
