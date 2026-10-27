// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Xuancheng75
const contract = {
  budget: {
    bytes: 65536,
    depth: 16,
    nodes: 4096,
    inflight: 32,
    storageValueBytes: 4194304,
    storageTransactionBytes: 8388608,
    storageChunkBytes: 24576,
    storageTransactionOps: 32
  },
  rendererAppearance: {
    type: 'object',
    properties: {
      mode: { enum: ['light', 'dark'] },
      cssVars: {
        type: 'object',
        properties: {
          '--ob-mode': { type: 'string', maxLength: 512 },
          '--ob-theme-id': { type: 'string', maxLength: 512 },
          '--ob-color-primary-contrast': { type: 'string', maxLength: 512 },
          '--ob-color-success-border': { type: 'string', maxLength: 512 },
          '--ob-color-warning-border': { type: 'string', maxLength: 512 },
          '--ob-color-error-border': { type: 'string', maxLength: 512 },
          '--ob-color-bg': { type: 'string', maxLength: 512 },
          '--ob-color-bg-layout': { type: 'string', maxLength: 512 },
          '--ob-color-bg-container': { type: 'string', maxLength: 512 },
          '--ob-color-bg-elevated': { type: 'string', maxLength: 512 },
          '--ob-color-primary': { type: 'string', maxLength: 512 },
          '--ob-color-primary-hover': { type: 'string', maxLength: 512 },
          '--ob-color-primary-bg': { type: 'string', maxLength: 512 },
          '--ob-color-text': { type: 'string', maxLength: 512 },
          '--ob-color-text-secondary': { type: 'string', maxLength: 512 },
          '--ob-color-text-tertiary': { type: 'string', maxLength: 512 },
          '--ob-color-border': { type: 'string', maxLength: 512 },
          '--ob-color-border-secondary': { type: 'string', maxLength: 512 },
          '--ob-color-success': { type: 'string', maxLength: 512 },
          '--ob-color-success-bg': { type: 'string', maxLength: 512 },
          '--ob-color-warning': { type: 'string', maxLength: 512 },
          '--ob-color-warning-bg': { type: 'string', maxLength: 512 },
          '--ob-color-error': { type: 'string', maxLength: 512 },
          '--ob-color-error-bg': { type: 'string', maxLength: 512 },
          '--ob-color-link': { type: 'string', maxLength: 512 },
          '--ob-radius': { type: 'string', maxLength: 512 },
          '--ob-font-family': { type: 'string', maxLength: 512 },
          '--ob-colorBg': { type: 'string', maxLength: 512 },
          '--ob-colorBgLayout': { type: 'string', maxLength: 512 },
          '--ob-colorBgContainer': { type: 'string', maxLength: 512 },
          '--ob-colorBgElevated': { type: 'string', maxLength: 512 },
          '--ob-colorPrimary': { type: 'string', maxLength: 512 },
          '--ob-colorPrimaryHover': { type: 'string', maxLength: 512 },
          '--ob-colorPrimaryBg': { type: 'string', maxLength: 512 },
          '--ob-colorText': { type: 'string', maxLength: 512 },
          '--ob-colorTextSecondary': { type: 'string', maxLength: 512 },
          '--ob-colorTextTertiary': { type: 'string', maxLength: 512 },
          '--ob-colorBorder': { type: 'string', maxLength: 512 },
          '--ob-colorBorderSecondary': { type: 'string', maxLength: 512 },
          '--ob-colorSuccess': { type: 'string', maxLength: 512 },
          '--ob-colorSuccessBg': { type: 'string', maxLength: 512 },
          '--ob-colorWarning': { type: 'string', maxLength: 512 },
          '--ob-colorWarningBg': { type: 'string', maxLength: 512 },
          '--ob-colorError': { type: 'string', maxLength: 512 },
          '--ob-colorErrorBg': { type: 'string', maxLength: 512 },
          '--ob-colorLink': { type: 'string', maxLength: 512 },
          '--ob-borderRadius': { type: 'string', maxLength: 512 },
          '--ob-fontFamily': { type: 'string', maxLength: 512 }
        },
        required: [
          '--ob-mode',
          '--ob-theme-id',
          '--ob-color-primary-contrast',
          '--ob-color-success-border',
          '--ob-color-warning-border',
          '--ob-color-error-border',
          '--ob-color-bg',
          '--ob-color-bg-layout',
          '--ob-color-bg-container',
          '--ob-color-bg-elevated',
          '--ob-color-primary',
          '--ob-color-primary-hover',
          '--ob-color-primary-bg',
          '--ob-color-text',
          '--ob-color-text-secondary',
          '--ob-color-text-tertiary',
          '--ob-color-border',
          '--ob-color-border-secondary',
          '--ob-color-success',
          '--ob-color-success-bg',
          '--ob-color-warning',
          '--ob-color-warning-bg',
          '--ob-color-error',
          '--ob-color-error-bg',
          '--ob-color-link',
          '--ob-radius',
          '--ob-font-family',
          '--ob-colorBg',
          '--ob-colorBgLayout',
          '--ob-colorBgContainer',
          '--ob-colorBgElevated',
          '--ob-colorPrimary',
          '--ob-colorPrimaryHover',
          '--ob-colorPrimaryBg',
          '--ob-colorText',
          '--ob-colorTextSecondary',
          '--ob-colorTextTertiary',
          '--ob-colorBorder',
          '--ob-colorBorderSecondary',
          '--ob-colorSuccess',
          '--ob-colorSuccessBg',
          '--ob-colorWarning',
          '--ob-colorWarningBg',
          '--ob-colorError',
          '--ob-colorErrorBg',
          '--ob-colorLink',
          '--ob-borderRadius',
          '--ob-fontFamily'
        ],
        additionalProperties: false
      }
    },
    required: ['mode', 'cssVars'],
    additionalProperties: false
  },
  rendererEvents: {
    filesDropped: {
      type: 'array',
      minItems: 1,
      maxItems: 256,
      items: { type: 'string', minLength: 1, maxLength: 2048 }
    }
  }
}
function canonicalNumeric(value) {
  return /^(0|[1-9][0-9]*)$/.test(value) && Number(value) <= Number.MAX_SAFE_INTEGER
}
function semver(value) {
  const match = value.match(/^([0-9]+)\.([0-9]+)\.([0-9]+)(?:-([A-Za-z0-9.-]+))?$/)
  return (
    !!match &&
    match.slice(1, 4).every(canonicalNumeric) &&
    (!match[4] ||
      match[4].split('.').every((id) => !!id && (!/^[0-9]+$/.test(id) || canonicalNumeric(id))))
  )
}
function shape(schema, value) {
  if (schema.oneOf && schema.oneOf.filter((child) => shape(child, value)).length !== 1) return false
  if (schema.type === 'boolean') return typeof value === 'boolean'
  if (schema.type === 'null') return value === null
  if (schema.type === 'integer' || schema.type === 'number')
    return (
      typeof value === 'number' &&
      Number.isFinite(value) &&
      (schema.type !== 'integer' || Number.isSafeInteger(value)) &&
      value >= (schema.minimum ?? -Infinity) &&
      value <= (schema.maximum ?? Infinity)
    )
  if ('const' in schema && value !== schema.const) return false
  if (schema.enum && !schema.enum.includes(value)) return false
  if (schema.type === 'object') {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return false
    if ((schema.required ?? []).some((key) => !Object.hasOwn(value, key))) return false
    if (
      schema.additionalProperties === false &&
      Object.keys(value).some((key) => !Object.hasOwn(schema.properties ?? {}, key))
    )
      return false
    return Object.entries(schema.properties ?? {}).every(
      ([key, child]) => !Object.hasOwn(value, key) || shape(child, value[key])
    )
  }
  if (schema.type === 'array') {
    return (
      Array.isArray(value) &&
      value.length >= (schema.minItems ?? 0) &&
      value.length <= (schema.maxItems ?? Infinity) &&
      (!schema.uniqueItems || new Set(value).size === value.length) &&
      value.every((item) => shape(schema.items, item))
    )
  }
  if (schema.type === 'string') {
    if (typeof value !== 'string') return false
    let points = value.length
    for (let i = 0; i < value.length; i++) {
      const code = value.charCodeAt(i)
      if (code >= 0xd800 && code <= 0xdbff) {
        points--
        i++
      }
    }
    if (points < (schema.minLength ?? 0) || points > (schema.maxLength ?? Infinity)) return false
    if (schema.pattern && !new RegExp(schema.pattern).test(value)) return false
    if (schema.format && (schema.format !== 'semver-without-build' || !semver(value))) return false
  }
  return true
}
function unicodeLength(text) {
  let bytes = 0
  for (let i = 0; i < text.length; i++) {
    const code = text.charCodeAt(i)
    if (code >= 0xd800 && code <= 0xdbff) {
      const next = text.charCodeAt(++i)
      if (!(next >= 0xdc00 && next <= 0xdfff)) throw new Error('INVALID_REQUEST')
      bytes += 4
    } else if (code >= 0xdc00 && code <= 0xdfff) throw new Error('INVALID_REQUEST')
    else bytes += code < 128 ? 1 : code < 2048 ? 2 : 3
  }
  return bytes
}
function validatePayload(raw, maxBytes = contract.budget.bytes) {
  if (typeof raw !== 'string' || unicodeLength(raw) > maxBytes) throw new Error('INVALID_REQUEST')
  const request = JSON.parse(raw)
  let nodes = 0
  function visit(value, depth) {
    if (++nodes > contract.budget.nodes || depth > contract.budget.depth)
      throw new Error('BUDGET_EXCEEDED')
    if (
      typeof value === 'number' &&
      (!Number.isFinite(value) || Math.abs(value) > Number.MAX_SAFE_INTEGER)
    )
      throw new Error('INVALID_REQUEST')
    if (typeof value === 'string') unicodeLength(value)
    if (value && typeof value === 'object') {
      for (const key of Object.keys(value)) unicodeLength(key)
      for (const child of Object.values(value)) visit(child, depth + 1)
    }
  }
  visit(request, 1)
  return request
}
export function validateAppearance(raw) {
  const value = validatePayload(raw)
  if (!shape(contract.rendererAppearance, value)) throw new Error('INVALID_APPEARANCE')
  return value
}

export function validateFilesDropped(raw) {
  const value = validatePayload(raw)
  if (!shape(contract.rendererEvents.filesDropped, value)) throw new Error('INVALID_EVENT')
  return value
}
