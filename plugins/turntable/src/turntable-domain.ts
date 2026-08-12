import type { TurntableItem } from './types'

export const FULL_TURN = Math.PI * 2
export const POINTER_ANGLE = -Math.PI / 2

export function normalizeAngle(angle: number): number {
  const normalized = angle % FULL_TURN
  return normalized < 0 ? normalized + FULL_TURN : normalized
}

export function secureRandomUnit(): number {
  const sample = new Uint32Array(1)
  globalThis.crypto.getRandomValues(sample)
  return sample[0] / 0x1_0000_0000
}

export function selectWeightedItem(
  items: readonly TurntableItem[],
  sample: number
): TurntableItem {
  if (!Number.isFinite(sample) || sample < 0 || sample >= 1) {
    throw new Error('Weighted sample must be in [0, 1)')
  }
  const totalWeight = items.reduce((sum, item) => {
    if (!Number.isFinite(item.weight) || item.weight <= 0) {
      throw new Error('Turntable weights must be finite and positive')
    }
    return sum + item.weight
  }, 0)
  if (items.length === 0 || !Number.isFinite(totalWeight) || totalWeight <= 0) {
    throw new Error('Turntable requires at least one valid item')
  }

  const target = sample * totalWeight
  let cumulative = 0
  for (const item of items) {
    cumulative += item.weight
    if (target < cumulative) return item
  }
  return items[items.length - 1]
}

export function winnerCenterAngle(
  items: readonly TurntableItem[],
  winnerId: number
): number {
  const totalWeight = items.reduce((sum, item) => sum + item.weight, 0)
  if (!Number.isFinite(totalWeight) || totalWeight <= 0) {
    throw new Error('Turntable weights must have a positive finite sum')
  }
  let angle = POINTER_ANGLE
  for (const item of items) {
    const sector = (item.weight / totalWeight) * FULL_TURN
    if (item.id === winnerId) return angle + sector / 2
    angle += sector
  }
  throw new Error('Winner is not present in the rendered turntable')
}

export function targetRotationForWinner(
  items: readonly TurntableItem[],
  winnerId: number,
  currentRotation: number,
  fullSpins: number
): number {
  if (!Number.isSafeInteger(fullSpins) || fullSpins < 0) {
    throw new Error('Full spin count must be a non-negative safe integer')
  }
  const desired = normalizeAngle(POINTER_ANGLE - winnerCenterAngle(items, winnerId))
  const forwardDelta = normalizeAngle(desired - normalizeAngle(currentRotation))
  return currentRotation + fullSpins * FULL_TURN + forwardDelta
}
