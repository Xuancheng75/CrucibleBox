export enum Permission {
  StorageRead = 'storage:read',
  StorageWrite = 'storage:write',
  TasksRead = 'tasks:read',
  TasksControl = 'tasks:control',
  ThemeRead = 'theme:read',
  ThemeWrite = 'theme:write',
  Dialog = 'dialog',
  Notification = 'notification',
  Clipboard = 'clipboard',
  FilesRead = 'files:read',
  FilesPublish = 'files:publish',
  TasksExecute = 'tasks:execute',
  BrowserDownloads = 'browser:downloads',
  NetworkFetch = 'network:fetch'
}
export const ALL_PERMISSIONS: Permission[] = Object.values(Permission)
export const PERMISSION_DESCRIPTIONS: Record<Permission, string> = {
  [Permission.StorageRead]: 'storage:read',
  [Permission.StorageWrite]: 'storage:write',
  [Permission.TasksRead]: 'tasks:read',
  [Permission.TasksControl]: 'tasks:control',
  [Permission.ThemeRead]: 'theme:read',
  [Permission.ThemeWrite]: 'theme:write',
  [Permission.Dialog]: 'dialog',
  [Permission.Notification]: 'notification',
  [Permission.Clipboard]: 'clipboard',
  [Permission.FilesRead]: 'files:read',
  [Permission.FilesPublish]: 'files:publish',
  [Permission.TasksExecute]: 'tasks:execute',
  [Permission.BrowserDownloads]: 'browser:downloads',
  [Permission.NetworkFetch]: 'network:fetch'
}
