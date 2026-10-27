// Generated from contracts/next2/contract.json.
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json }
export type Empty = Record<string, never>
export type VersionRange = { min: string; maxExclusive: string }
export type ApiRange = { min: 6; maxExclusive: 7 }
export type Publisher = { id: string; displayName: string }
export type FileEntry = { path: string; bytes: number; sha256: string }
export type JsBackend = { kind: 'js'; entry: string }
export type NativeBackend = {
  kind: 'native'
  entry: string
  os: 'windows'
  arch: 'x86_64'
  trust: 'trusted-local-code'
}
export type Backend = JsBackend | NativeBackend
export type Navigation = { title: string; icon?: string }
export type Command = { id: string; title: string; method: string }
export type Contributions = {
  navigation?: Navigation
  commands: Command[]
  fileHandlers?: FileHandler[]
}
export type ServiceExport = { id: string; version: string; methods: string[] }
export type ServiceDependency = { id: string; providerId: string; version: VersionRange }
export type ResourceDependency = {
  id: string
  version: VersionRange
  catalog?: string
  optional?: boolean
}
export type ResourceDownload = { path: string; url: string }
export type ResourceRelease = {
  id: string
  title: string
  description: string
  manifest: ResourceManifest
  downloads: ResourceDownload[]
}
export type ResourceCatalog = { resourceCatalogVersion: 1; releases: ResourceRelease[] }
export type PlatformTarget = { kind: 'platform' }
export type BackendTarget = { kind: 'backend' }
export type ServiceTarget = { kind: 'service'; id: string }
export type Target = PlatformTarget | BackendTarget | ServiceTarget
export type Request = {
  wireVersion: 4
  requestId: string
  session: string
  target: Target
  method: string
  params: Json
}
export type RpcError = { code: string; message: string }
export type Success = { wireVersion: 4; requestId: string; ok: true; result: Json }
export type Failure = { wireVersion: 4; requestId: string; ok: false; error: RpcError }
export type Response = Success | Failure
export type FileReference = { ref: string; name: string; bytes: number }
export type FileReadParams = { ref: string; offset: number; length: number }
export type FileResolveParams = { ref: string }
export type FilePublishParams = { taskId: string; path: string; name: string }
export type TaskStartParams = { target: Target; method: string; params: Json }
export type TaskProgressParams = {
  taskId: string
  sequence: number
  stage: string
  percent: number
  message: string
}
export type BackendHello = { kind: 'hello'; wireVersion: 4; sdkApiVersion: 6 }
export type BackendInit = {
  kind: 'initialize'
  wireVersion: 4
  session: string
  pluginId: string
  dataDirectory: string
  resources: ResourceMount[]
}
export type ResourceMount = { id: string; version: string; directory: string }
export type BackendReady = { kind: 'ready'; wireVersion: 4 }
export type BackendExecute = {
  kind: 'execute'
  request: Request
  taskId?: string
  callerId: string
}
export type BackendReply = { kind: 'reply'; response: Response }
export type BackendCapability = { kind: 'capability'; request: Request }
export type BackendCapabilityReply = { kind: 'capability-result'; response: Response }
export type BackendCancel = { kind: 'cancel'; wireVersion: 4; taskId: string }
export type BackendStop = { kind: 'stop'; wireVersion: 4 }
export type BackendFrame =
  | BackendHello
  | BackendInit
  | BackendReady
  | BackendExecute
  | BackendReply
  | BackendCapability
  | BackendCapabilityReply
  | BackendCancel
  | BackendStop
export type RuntimePingParams = Record<string, never>
export type RuntimePingResult = Json
export type StorageGetParams = { key: string }
export type StorageGetResult = Json
export type StorageSetParams = { key: string; value: Json }
export type StorageSetResult = Json
export type StorageDeleteParams = { key: string }
export type StorageDeleteResult = Json
export type StorageBatchParams = {
  operations: ({ type: 'set'; key: string; value: Json } | { type: 'delete'; key: string })[]
}
export type StorageBatchResult = Json
export type StorageListParams = { prefix: string; limit: number; after?: string }
export type StorageListResult = { items: { key: string; value: Json }[]; nextCursor: string | null }
export type StorageKeysParams = { prefix: string; limit: number; after?: string }
export type StorageKeysResult = { items: string[]; nextCursor: string | null }
export type StorageReadBeginParams = { key: string }
export type StorageReadBeginResult = { found: boolean; readId: string | null; byteLength: number }
export type StorageReadChunkParams = { readId: string; offset: number }
export type StorageReadChunkResult = { offset: number; data: string }
export type StorageReadCloseParams = { readId: string }
export type StorageReadCloseResult = Json
export type StorageWriteBeginParams = {
  writes: { key: string; byteLength: number }[]
  deletes: string[]
}
export type StorageWriteBeginResult = { transactionId: string }
export type StorageWriteChunkParams = {
  transactionId: string
  key: string
  offset: number
  data: string
}
export type StorageWriteChunkResult = { receivedBytes: number }
export type StorageWriteCommitParams = { transactionId: string }
export type StorageWriteCommitResult = Json
export type StorageWriteAbortParams = { transactionId: string }
export type StorageWriteAbortResult = Json
export type ConfigGetParams = Record<string, never>
export type ConfigGetResult = Json
export type ConfigPatchParams = { values: Record<string, Json> }
export type ConfigPatchResult = Json
export type TasksGetParams = { taskId: string }
export type TasksGetResult = Json
export type TasksControlStateParams = { taskId: string; executionPaused?: boolean }
export type TasksCancelParams = { taskId: string }
export type TasksCancelResult = Json
export type TasksListParams = { limit: number; after?: string }
export type TasksListResult = Json
export type ThemeGetParams = Record<string, never>
export type ThemeGetResult = Json
export type ThemeListParams = Record<string, never>
export type ThemeListResult = Json
export type ThemePreviewParams = {
  theme: {
    id: string
    name: string
    mode: 'light' | 'dark'
    tokens: {
      colorBg: string
      colorBgLayout: string
      colorBgContainer: string
      colorBgElevated: string
      colorPrimary: string
      colorPrimaryHover: string
      colorPrimaryBg: string
      colorText: string
      colorTextSecondary: string
      colorTextTertiary: string
      colorBorder: string
      colorBorderSecondary: string
      colorSuccess: string
      colorSuccessBg: string
      colorWarning: string
      colorWarningBg: string
      colorError: string
      colorErrorBg: string
      colorLink: string
      borderRadius: number
      fontFamily: string
    }
  }
}
export type ThemePreviewResult = Json
export type ThemeCommitParams = Record<string, never>
export type ThemeCommitResult = Json
export type ThemeRollbackParams = Record<string, never>
export type ThemeRollbackResult = Json
export type ThemeSetParams = {
  theme: {
    id: string
    name: string
    mode: 'light' | 'dark'
    tokens: {
      colorBg: string
      colorBgLayout: string
      colorBgContainer: string
      colorBgElevated: string
      colorPrimary: string
      colorPrimaryHover: string
      colorPrimaryBg: string
      colorText: string
      colorTextSecondary: string
      colorTextTertiary: string
      colorBorder: string
      colorBorderSecondary: string
      colorSuccess: string
      colorSuccessBg: string
      colorWarning: string
      colorWarningBg: string
      colorError: string
      colorErrorBg: string
      colorLink: string
      borderRadius: number
      fontFamily: string
    }
  }
}
export type ThemeSetResult = Json
export type DialogOpenParams = {
  type: 'file' | 'folder'
  multiple?: boolean
  extensions?: string[]
}
export type DialogOpenResult = Json
export type DialogConfirmParams = {
  title: string
  message: string
  confirmLabel?: string
  cancelLabel?: string
}
export type DialogConfirmResult = Json
export type NotificationShowParams = { title: string; body: string }
export type NotificationShowResult = Json
export type Manifest = {
  manifestVersion: 6
  sdkApiVersion: 6
  id: string
  version: string
  displayName: string
  description: string
  publisher: Publisher
  minHostVersion: string
  sdk: ApiRange
  renderer: string
  backend?: Backend
  permissions: (
    | 'storage:read'
    | 'storage:write'
    | 'tasks:read'
    | 'tasks:control'
    | 'theme:read'
    | 'theme:write'
    | 'dialog'
    | 'notification'
    | 'clipboard'
    | 'files:read'
    | 'files:publish'
    | 'tasks:execute'
    | 'browser:downloads'
    | 'network:fetch'
  )[]
  background: boolean
  contributes: Contributions
  exports: ServiceExport[]
  dependencies: ServiceDependency[]
  resources: ResourceDependency[]
  files: FileEntry[]
  config?: Record<string, Json>
}
export type ResourceManifest = {
  resourceManifestVersion: 1
  id: string
  pluginId: string
  version: string
  kind: 'model' | 'runtime' | 'data'
  containsNativeCode: boolean
  source: string
  license: string
  pluginVersion: VersionRange
  os: 'windows'
  arch: 'x86_64'
  files: FileEntry[]
  publisher: Publisher
}
export type RendererAppearance = {
  mode: 'light' | 'dark'
  cssVars: {
    '--ob-mode': string
    '--ob-theme-id': string
    '--ob-color-primary-contrast': string
    '--ob-color-success-border': string
    '--ob-color-warning-border': string
    '--ob-color-error-border': string
    '--ob-color-bg': string
    '--ob-color-bg-layout': string
    '--ob-color-bg-container': string
    '--ob-color-bg-elevated': string
    '--ob-color-primary': string
    '--ob-color-primary-hover': string
    '--ob-color-primary-bg': string
    '--ob-color-text': string
    '--ob-color-text-secondary': string
    '--ob-color-text-tertiary': string
    '--ob-color-border': string
    '--ob-color-border-secondary': string
    '--ob-color-success': string
    '--ob-color-success-bg': string
    '--ob-color-warning': string
    '--ob-color-warning-bg': string
    '--ob-color-error': string
    '--ob-color-error-bg': string
    '--ob-color-link': string
    '--ob-radius': string
    '--ob-font-family': string
    '--ob-colorBg': string
    '--ob-colorBgLayout': string
    '--ob-colorBgContainer': string
    '--ob-colorBgElevated': string
    '--ob-colorPrimary': string
    '--ob-colorPrimaryHover': string
    '--ob-colorPrimaryBg': string
    '--ob-colorText': string
    '--ob-colorTextSecondary': string
    '--ob-colorTextTertiary': string
    '--ob-colorBorder': string
    '--ob-colorBorderSecondary': string
    '--ob-colorSuccess': string
    '--ob-colorSuccessBg': string
    '--ob-colorWarning': string
    '--ob-colorWarningBg': string
    '--ob-colorError': string
    '--ob-colorErrorBg': string
    '--ob-colorLink': string
    '--ob-borderRadius': string
    '--ob-fontFamily': string
  }
}
export type TaskSnapshot = {
  taskId: string
  status: 'queued' | 'running' | 'paused' | 'succeeded' | 'failed' | 'cancelled' | 'interrupted'
  sequence: number
  cancelRequested: boolean
  resultRefs: string[]
  resourceKey: string
  error?: Json
  progress?: Json
  publication?: Json
  checkpoint?: Json
  initiatorId: string
  providerId: string
} & Record<string, Json>
export type SignatureRecord = {
  signatureVersion: 1
  publisherId: string
  keyId: string
  signature: string
}
export type NetworkFetchParams = {
  url: string
  method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE' | 'HEAD'
  headers?: Record<string, Json>
  body?: string
}
export type NetworkFetchResult = { status: number; body: string; headers: Record<string, Json> }
export type FileHandler = { extensions: string[] }
