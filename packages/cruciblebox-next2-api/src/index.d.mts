export * from './generated.mjs'
import type * as Generated from './generated.mjs'
import type {
  Manifest,
  ResourceManifest,
  ResourceCatalog,
  Request,
  Response,
  BackendFrame,
  CapabilityBinding,
  CapabilityDescription,
  CapabilityListParams,
  CapabilityPage,
  EventPollResult,
  EventPublishParams,
  EventPublishResult,
  EventSubscription,
  EventTopicParams,
  FileStreamChunk,
  FileInfo,
  FileReference,
  FileWriteResultParams,
  NetworkDownloadParams,
  ProcessRunParams,
  ProcessResult,
  ResourceMount,
  RuntimeStatus,
  LogWriteParams,
  Json,
  VersionRange,
  RendererAppearance
} from './generated.mjs'
export interface RendererContext {
  root: HTMLElement
  session: string
  appearance: RendererAppearance
  exchange(request: Request, signal?: AbortSignal): Promise<Response>
  onAppearanceChanged(listener: (appearance: RendererAppearance) => void): () => void
  onFilesDropped(listener: (refs: string[]) => void): () => void
  onVisibilityChanged?(listener: (visible: boolean) => void): () => void
  enableThemeStyle?(version: 2): boolean
}
export declare const contract: Readonly<Record<string, unknown>> & {
  readonly versions: import('./generated.mjs').ContractVersions
}
export declare function validateManifest(raw: string): Manifest
export declare function validateThemeDesign(raw: string): import('./generated.mjs').ThemeDesign
export declare function validateResourceManifest(raw: string): ResourceManifest
export declare function validateResourceCatalog(raw: string): ResourceCatalog
export declare function validateRequest(raw: string): Request
export declare function validateResponse(raw: string, requestId: string): Response
export declare function validateBackendFrame(raw: string): BackendFrame
export declare function compatible(version: string, range: VersionRange): boolean
export type RpcOptions = { signal?: AbortSignal }
export declare function createClient(options: {
  session: string
  exchange(request: Request, signal?: AbortSignal): Promise<Response>
}): {
  platform(method: string, params?: Json): Promise<Json>
  resolveCapability(id: string): Promise<CapabilityBinding>
  capabilities: {
    resolve(id: string): Promise<CapabilityBinding>
    describe(id: string): Promise<CapabilityDescription>
    list(options?: CapabilityListParams): Promise<CapabilityPage>
  }
  writeLog(entry: LogWriteParams): Promise<boolean>
  files: {
    info(ref: string): Promise<FileInfo>
    list(ref: string): Promise<FileInfo[]>
    writeResult(options: FileWriteResultParams): Promise<FileReference>
    child(ref: string, path: string): Promise<FileReference>
    createDirectory(ref: string, name: string): Promise<FileReference>
    walk(
      ref: string,
      options?: Omit<Generated.FileWalkParams, 'ref'>
    ): Promise<Generated.FileWalkPage>
    copy(options: Generated.FileTransferParams): Promise<FileReference>
    move(options: Generated.FileTransferParams): Promise<FileReference>
    remove(ref: string, options?: Omit<Generated.FileRemoveParams, 'ref'>): Promise<boolean>
    temporary(options: Generated.FileTemporaryParams): Promise<FileReference>
    releaseTemporary(ref: string): Promise<boolean>
    write: {
      open(options: Generated.FileWriteOpenParams): Promise<Generated.FileWriteStream>
      append(streamId: string, data: string): Promise<Generated.FileWriteChunkResult>
      commit(streamId: string): Promise<FileReference>
      abort(streamId: string): Promise<boolean>
    }
    watch: {
      open(
        ref: string,
        options?: Omit<Generated.FileWatchParams, 'ref'>
      ): Promise<Generated.FileWatch>
      poll(watchId: string): Promise<Generated.FileWatchPoll>
      close(watchId: string): Promise<boolean>
    }
  }
  network: { download(options: NetworkDownloadParams): Promise<FileReference> }
  process: { run(options: ProcessRunParams): Promise<ProcessResult> }
  runtime: {
    list(): Promise<RuntimeStatus[]>
    resolve(id: string): Promise<CapabilityDescription>
  }
  backend(method: string, params?: Json, options?: RpcOptions): Promise<Json>
  service(id: string, method: string, params?: Json, options?: RpcOptions): Promise<Json>
  events: {
    subscribe(topic: EventTopicParams): Promise<EventSubscription>
    publish(event: EventPublishParams): Promise<EventPublishResult>
    poll(subscriptionId: string): Promise<EventPollResult>
    unsubscribe(subscriptionId: string): Promise<boolean>
  }
  readFileChunks(ref: string, options?: RpcOptions): AsyncGenerator<FileStreamChunk, void, unknown>
}

export interface BackendContext {
  session: string
  pluginId: string
  dataDirectory: string
  resources: ResourceMount[]
  exchange(request: Request): Promise<Response>
  isCancelled(): boolean
}

export interface ProviderExecution {
  callerId: string
  taskId?: string | null
  isCancelled(): boolean
}

type ProviderMethods = Record<
  string,
  (params: Json, execution: ProviderExecution) => Json | Promise<Json>
>
export declare function defineProvider(
  activate: (
    context: BackendContext & { api: ReturnType<typeof createClient> }
  ) => ProviderMethods | Promise<ProviderMethods>
): (context: BackendContext) => ProviderMethods | Promise<ProviderMethods>
