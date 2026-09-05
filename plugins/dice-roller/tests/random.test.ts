import { describe, expect, it } from 'vitest'
import { parseDiceNotation, randomInteger, rollDice } from '../src/random'

describe('dice Web Crypto mapping', () => {
  it('maps the accepted uint32 range onto exact dice boundaries', () => {
    expect(randomInteger(6, () => 0)).toBe(0)
    expect(randomInteger(6, () => 4_294_967_291)).toBe(5)
  })

  it('rejects the modulo-bias tail before accepting another sample', () => {
    const samples = [0xffff_ffff, 7]
    expect(randomInteger(6, () => samples.shift()!)).toBe(1)
    expect(samples).toHaveLength(0)
  })

  it('keeps every roll inside the requested inclusive bounds', () => {
    let sample = 0
    const values = rollDice(20, 100, () => sample++)
    expect(values).toHaveLength(20)
    expect(Math.min(...values)).toBeGreaterThanOrEqual(1)
    expect(Math.max(...values)).toBeLessThanOrEqual(100)
  })

  it('maps a complete deterministic cycle uniformly', () => {
    let sample = 0
    const counts = Array.from({ length: 6 }, () => 0)
    for (let index = 0; index < 6000; index += 1) {
      counts[randomInteger(6, () => sample++)] += 1
    }
    expect(counts).toEqual([1000, 1000, 1000, 1000, 1000, 1000])
  })

  it('rejects invalid dice dimensions and invalid entropy sources', () => {
    expect(() => rollDice(0, 6, () => 0)).toThrow(RangeError)
    expect(() => rollDice(1, 1, () => 0)).toThrow(RangeError)
    expect(() => randomInteger(6, () => -1)).toThrow(RangeError)
    expect(() => randomInteger(6, () => 0x1_0000_0000)).toThrow(RangeError)
  })

  it('parses bounded tabletop notation and modifiers', () => {
    expect(parseDiceNotation(' 3d20 + 4 ')).toEqual({ count: 3, sides: 20, modifier: 4 })
    expect(parseDiceNotation('1D6-2')).toEqual({ count: 1, sides: 6, modifier: -2 })
    expect(() => parseDiceNotation('d6')).toThrow(RangeError)
  })
})
