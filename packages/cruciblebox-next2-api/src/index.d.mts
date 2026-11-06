export * from './generated.mjs'
import type {
  Manifest,
  ResourceManifest,
  ResourceCatalog,
  Request,
  Response,
  BackendFrame,
  CapabilityBinding,
  EventPollResult,
  EventPublishParams,
  EventPublishResult,
  EventSubscription,
  EventTopicParams,
  FileStreamChunk,
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
export declare const contract: Readonly<Record<string, unknown>>
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
  writeLog(entry: LogWriteParams): Promise<boolean>
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
