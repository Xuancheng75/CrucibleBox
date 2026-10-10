export function compareVersions(left: string, right: string): number {
  const parse = (value: string) => {
    const match = value.replace(/^v/i, '').match(/^(\d+)\.(\d+)\.(\d+)(?:-([^+]+))?/)
    if (!match) return { core: [0, 0, 0], pre: [] as string[] }
    return {
      core: [Number(match[1]), Number(match[2]), Number(match[3])],
      pre: match[4]?.split('.') ?? []
    }
  }
  const a = parse(left)
  const b = parse(right)
  for (let index = 0; index < 3; index += 1) {
    if (a.core[index] !== b.core[index]) return a.core[index] - b.core[index]
  }
  if (a.pre.length === 0 && b.pre.length === 0) return 0
  if (a.pre.length === 0) return 1
  if (b.pre.length === 0) return -1
  for (let index = 0; index < Math.max(a.pre.length, b.pre.length); index += 1) {
    const av = a.pre[index]
    const bv = b.pre[index]
    if (av === undefined) return -1
    if (bv === undefined) return 1
    if (av === bv) continue
    const an = /^\d+$/.test(av)
    const bn = /^\d+$/.test(bv)
    if (an && bn) return Number(av) - Number(bv)
    if (an !== bn) return an ? -1 : 1
    return av < bv ? -1 : 1
  }
  return 0
}
