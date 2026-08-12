export interface GifFrame {
  id: string
  imageData: ImageData
  delay: number
}

export interface GifDocument {
  width: number
  height: number
  sourceName: string
  frames: GifFrame[]
}

export interface EncodeOptions {
  repeat?: number
  delay?: number
  quality?: number
}

export interface CropRect {
  x: number
  y: number
  width: number
  height: number
}

export type Rgb = [number, number, number]

export type ImageTransform = (imageData: ImageData) => ImageData

export interface TransformSet {
  rotate90: ImageTransform
  rotate270: ImageTransform
  flipHorizontal: ImageTransform
  flipVertical: ImageTransform
  scale: (factor: number) => ImageTransform
  crop: (rect: CropRect) => ImageTransform
  brightness: (amount: number) => ImageTransform
  contrast: (amount: number) => ImageTransform
  saturation: (amount: number) => ImageTransform
  grayscale: ImageTransform
  invert: ImageTransform
  replaceColor: (target: Rgb, replacement: Rgb, tolerance: number) => ImageTransform
}
