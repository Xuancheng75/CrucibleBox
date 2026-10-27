import semver from 'semver'
import { contract } from './generated.mjs'

export function packagePath(value) {
  return (
    /^[A-Za-z0-9_./-]+$/.test(value) &&
    value
      .split('/')
      .every(
        (part) =>
          part !== '' &&
          part !== '.' &&
          part !== '..' &&
          !part.endsWith('.') &&
          !/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(part)
      )
  )
}
function format(name, value) {
  if (name === 'plugin-id') return /^[a-z][a-z0-9-]{0,63}$/.test(value)
  if (name === 'package-path') return packagePath(value)
  if (name === 'semver-without-build') return semver.valid(value) === value && !value.includes('+')
  throw Error('Unknown contract format: ' + name)
}
export function matches(rule, value) {
  if (rule.$ref) return matches(contract.$defs[rule.$ref.split('/').at(-1)], value)
  if ('const' in rule && value !== rule.const) return false
  if (rule.enum && !rule.enum.includes(value)) return false
  if (rule.oneOf && rule.oneOf.filter((child) => matches(child, value)).length !== 1) return false
  if (rule.type === 'null') return value === null
  if (rule.type === 'boolean') return typeof value === 'boolean'
  if (rule.type === 'integer' || rule.type === 'number')
    return (
      typeof value === 'number' &&
      Number.isFinite(value) &&
      (rule.type !== 'integer' || Number.isSafeInteger(value)) &&
      value >= (rule.minimum ?? -Infinity) &&
      value <= (rule.maximum ?? Infinity)
    )
  if (rule.type === 'string')
    return (
      typeof value === 'string' &&
      [...value].length >= (rule.minLength ?? 0) &&
      [...value].length <= (rule.maxLength ?? Infinity) &&
      (!rule.pattern || new RegExp(rule.pattern).test(value)) &&
      (!rule.format || format(rule.format, value))
    )
  if (rule.type === 'array')
    return (
      Array.isArray(value) &&
      value.length >= (rule.minItems ?? 0) &&
      value.length <= (rule.maxItems ?? Infinity) &&
      (!rule.uniqueItems ||
        new Set(value.map((item) => JSON.stringify(item))).size === value.length) &&
      value.every((item) => matches(rule.items, item))
    )
  if (rule.type === 'object')
    return (
      value !== null &&
      typeof value === 'object' &&
      !Array.isArray(value) &&
      (rule.required ?? []).every((key) => Object.hasOwn(value, key)) &&
      (rule.additionalProperties !== false ||
        Object.keys(value).every((key) => Object.hasOwn(rule.properties, key))) &&
      Object.entries(rule.properties ?? {}).every(
        ([key, child]) => !Object.hasOwn(value, key) || matches(child, value[key])
      )
    )
  return true
}
export function parse(raw, name, maxBytes = contract.budget.bytes) {
  if (typeof raw !== 'string' || new TextEncoder().encode(raw).length > maxBytes)
    throw Error('BUDGET_EXCEEDED')
  const value = JSON.parse(raw)
  let nodes = 0
  function visit(child, depth) {
    if (
      ++nodes >
        (maxBytes === contract.budget.bytes
          ? contract.budget.nodes
          : contract.budget.manifestNodes) ||
      depth > contract.budget.depth
    )
      throw Error('BUDGET_EXCEEDED')
    if (
      typeof child === 'number' &&
      (!Number.isFinite(child) || Math.abs(child) > Number.MAX_SAFE_INTEGER)
    )
      throw Error('INVALID_PAYLOAD')
    if (typeof child === 'string' && /[\ud800-\udfff]/u.test(child)) throw Error('INVALID_PAYLOAD')
    if (child && typeof child === 'object') {
      if (Object.keys(child).some((key) => /[\ud800-\udfff]/u.test(key)))
        throw Error('INVALID_PAYLOAD')
      Object.values(child).forEach((item) => visit(item, depth + 1))
    }
  }
  visit(value, 1)
  if (!matches(contract.$defs[name], value)) throw Error('INVALID_' + name.toUpperCase())
  return value
}
export function unique(items, key) {
  if (new Set(items.map(key)).size !== items.length) throw Error('DUPLICATE_DECLARATION')
}
export function validRange(range) {
  if (!semver.lt(range.min, range.maxExclusive)) throw Error('INVALID_VERSION_RANGE')
}
export function compatible(version, range) {
  return semver.gte(version, range.min) && semver.lt(version, range.maxExclusive)
}
export function validateInventory(files) {
  unique(files, (file) => file.path.toLowerCase())
  const names = new Set(files.map((file) => file.path.toLowerCase()))
  for (const file of files) {
    if (
      contract.install.reservedPaths.some(
        (reserved) =>
          file.path.toLowerCase() === reserved || file.path.toLowerCase().startsWith(reserved + '/')
      )
    )
      throw Error('RESERVED_PACKAGE_FILE')
    const parents = file.path.split('/')
    while (parents.length > 1) {
      parents.pop()
      if (names.has(parents.join('/').toLowerCase()))
        throw Error('PACKAGE_FILE_DIRECTORY_COLLISION')
    }
  }
}
