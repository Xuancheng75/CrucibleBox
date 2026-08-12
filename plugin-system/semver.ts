const SEMVER_PATTERN =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/

export interface ParsedSemVer {
  major: number
  minor: number
  patch: number
  prerelease: string[]
}

export function parseSemVer(value: string): ParsedSemVer {
  const match = SEMVER_PATTERN.exec(value)
  if (!match) throw new TypeError(`Invalid semantic version: ${value}`)
  const major = Number(match[1])
  const minor = Number(match[2])
  const patch = Number(match[3])
  if (![major, minor, patch].every(Number.isSafeInteger)) {
    throw new TypeError(`Semantic version exceeds safe integer range: ${value}`)
  }
  const prerelease = match[4]?.split('.') ?? []
  for (const identifier of prerelease) {
    if (/^\d+$/.test(identifier) && identifier.length > 1 && identifier.startsWith('0')) {
      throw new TypeError(`Numeric prerelease identifiers cannot contain leading zeroes: ${value}`)
    }
  }
  return { major, minor, patch, prerelease }
}

function comparePrerelease(left: readonly string[], right: readonly string[]): number {
  if (left.length === 0 && right.length === 0) return 0
  if (left.length === 0) return 1
  if (right.length === 0) return -1

  const length = Math.max(left.length, right.length)
  for (let index = 0; index < length; index += 1) {
    const a = left[index]
    const b = right[index]
    if (a === undefined) return -1
    if (b === undefined) return 1
    if (a === b) continue
    const aNumeric = /^\d+$/.test(a)
    const bNumeric = /^\d+$/.test(b)
    if (aNumeric && bNumeric) {
      if (a.length !== b.length) return a.length < b.length ? -1 : 1
      return a < b ? -1 : 1
    }
    if (aNumeric !== bNumeric) return aNumeric ? -1 : 1
    return a < b ? -1 : 1
  }
  return 0
}

export function compareVersions(left: string, right: string): number {
  const a = parseSemVer(left)
  const b = parseSemVer(right)
  for (const key of ['major', 'minor', 'patch'] as const) {
    if (a[key] !== b[key]) return a[key] < b[key] ? -1 : 1
  }
  return comparePrerelease(a.prerelease, b.prerelease)
}
