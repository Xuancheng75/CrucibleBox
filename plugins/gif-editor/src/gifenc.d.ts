// Ambient type declarations for gifenc (v1.0.3 ships no bundled types).
// Mirrors the public API documented in node_modules/gifenc/README.md.

declare module 'gifenc' {
  export type GifColorFormat = 'rgb565' | 'rgb444' | 'rgba4444'

  export interface QuantizeOptions {
    format?: GifColorFormat
    oneBitAlpha?: boolean | number
    clearAlpha?: boolean
    clearAlphaThreshold?: number
    clearAlphaColor?: number
  }

  export interface GifEncoderWriteOptions {
    transparent?: boolean
    transparentIndex?: number
    delay?: number
    palette?: number[][]
    repeat?: number
    colorDepth?: number
    dispose?: number
    first?: boolean
  }

  export interface GifEncoderInstance {
    reset(): void
    finish(): void
    bytes(): Uint8Array<ArrayBuffer>
    bytesView(): Uint8Array<ArrayBuffer>
    writeHeader(): void
    writeFrame(
      index: Uint8Array,
      width: number,
      height: number,
      opts?: GifEncoderWriteOptions
    ): void
    readonly buffer: ArrayBuffer
  }

  export function GIFEncoder(opts?: {
    initialCapacity?: number
    auto?: boolean
  }): GifEncoderInstance

  export function quantize(
    rgba: Uint8Array | Uint8ClampedArray,
    maxColors: number,
    opts?: QuantizeOptions
  ): number[][]

  export function applyPalette(
    rgba: Uint8Array | Uint8ClampedArray,
    palette: number[][],
    format?: GifColorFormat
  ): Uint8Array
}