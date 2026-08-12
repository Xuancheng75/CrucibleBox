class ImageDataPolyfill implements ImageData {
  readonly colorSpace = 'srgb' as const
  readonly data: Uint8ClampedArray<ArrayBuffer>
  readonly height: number
  readonly width: number

  constructor(data: Uint8ClampedArray<ArrayBuffer>, width: number, height?: number) {
    const resolvedHeight = height ?? data.byteLength / (width * 4)
    if (
      !Number.isInteger(width) ||
      !Number.isInteger(resolvedHeight) ||
      width <= 0 ||
      resolvedHeight <= 0 ||
      data.byteLength !== width * resolvedHeight * 4
    ) {
      throw new RangeError('ImageData dimensions do not match the RGBA buffer')
    }
    this.data = data
    this.width = width
    this.height = resolvedHeight
  }
}

Object.defineProperty(globalThis, 'ImageData', {
  configurable: true,
  value: ImageDataPolyfill
})
