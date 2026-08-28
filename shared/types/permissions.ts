export enum Permission {
  DatabaseRead = 'database:read',
  DatabaseWrite = 'database:write',
  StorageRead = 'storage:read',
  StorageWrite = 'storage:write',
  ShellExec = 'shell:exec',
  NetworkFetch = 'network:fetch',
  Notification = 'notification',
  Clipboard = 'clipboard',
  Dialog = 'dialog',
  Shortcut = 'shortcut',
  FileRead = 'file:read',
  FileWrite = 'file:write',
  ThemeWrite = 'theme:write',
  TrustedUniEnv = 'trusted:unienv',
  TrustedDocumentEngine = 'trusted:document-engine'
}

export const ALL_PERMISSIONS: Permission[] = Object.values(Permission)

export const PERMISSION_DESCRIPTIONS: Record<Permission, string> = {
  [Permission.DatabaseRead]: '读取数据库（插件配置以外）',
  [Permission.DatabaseWrite]: '写入数据库',
  [Permission.StorageRead]: '读取插件私有存储',
  [Permission.StorageWrite]: '写入插件私有存储',
  [Permission.ShellExec]: '执行系统命令',
  [Permission.NetworkFetch]: '发起网络请求',
  [Permission.Notification]: '发送系统通知',
  [Permission.Clipboard]: '读写剪贴板',
  [Permission.Dialog]: '打开系统对话框',
  [Permission.Shortcut]: '注册全局快捷键',
  [Permission.FileRead]: '读取本地文件',
  [Permission.FileWrite]: '写入本地文件',
  [Permission.ThemeWrite]: '修改工具箱主题',
  [Permission.TrustedUniEnv]: '调用宿主持有的 UniEnv 安装服务',
  [Permission.TrustedDocumentEngine]: '调用宿主持有的 Document Engine 文档处理服务'
}
