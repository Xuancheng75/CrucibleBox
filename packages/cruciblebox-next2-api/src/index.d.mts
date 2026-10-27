export * from './generated.mjs'
import type {
  Manifest,
  ResourceManifest,
  ResourceCatalog,
  Request,
  Response,
  BackendFrame,
  Json,
  VersionRange,
  RendererAppearance
} from './generated.mjs'
export interface RendererContext {
  root: HTMLElement
  session: string
  appearance: RendererAppearance
  exchange(request: Request): Promise<Response>
  onAppearanceChanged(listener: (appearance: RendererAppearance) => void): () => void
  onFilesDropped(listener: (refs: string[]) => void): () => void
}
export declare const contract: Readonly<Record<string, unknown>>
export declare function validateManifest(raw: string): Manifest
export declare function validateResourceManifest(raw: string): ResourceManifest
export declare function validateResourceCatalog(raw: string): ResourceCatalog
export declare function validateRequest(raw: string): Request
export declare function validateResponse(raw: string, requestId: string): Response
export declare function validateBackendFrame(raw: string): BackendFrame
export declare function compatible(version: string, range: VersionRange): boolean
export declare function createClient(options: {
  session: string
  exchange(request: Request): Promise<Response>
}): {
  platform(method: string, params?: Json): Promise<Json>
  backend(method: string, params?: Json): Promise<Json>
  service(id: string, method: string, params?: Json): Promise<Json>
}
