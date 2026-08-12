const UINT32_RANGE = 0x1_0000_0000

export type Uint32Source = () => number

export function secureUint32(): number {
  const values = new Uint32Array(1)
  globalThis.crypto.getRandomValues(values)
  return values[0]
}

export function randomInteger(maxExclusive: number, source: Uint32Source = secureUint32): number {
  if (!Number.isSafeInteger(maxExclusive) || maxExclusive < 1 || maxExclusive > UINT32_RANGE) {
    throw new RangeError('maxExclusive must be a safe integer between 1 and 2^32')
  }
  const limit = Math.floor(UINT32_RANGE / maxExclusive) * maxExclusive
  for (;;) {
    const value = source()
    if (!Number.isInteger(value) || value < 0 || value >= UINT32_RANGE) {
      throw new RangeError('random source must return an unsigned 32-bit integer')
    }
    if (value < limit) return value % maxExclusive
  }
}

export function rollDice(
  count: number,
  sides: number,
  source: Uint32Source = secureUint32
): number[] {
  if (!Number.isSafeInteger(count) || count < 1 || count > 20) {
    throw new RangeError('count must be an integer between 1 and 20')
  }
  if (!Number.isSafeInteger(sides) || sides < 2 || sides > 100) {
    throw new RangeError('sides must be an integer between 2 and 100')
  }
  return Array.from({ length: count }, () => randomInteger(sides, source) + 1)
}
