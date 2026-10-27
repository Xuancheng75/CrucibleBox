import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const contract = JSON.parse(readFileSync(resolve(root, 'contracts/next2/contract.json'), 'utf8'))
const defs = contract.$defs
const reference = (rule) => rule.$ref?.split('/').at(-1)
function ts(rule) {
  if (rule.$ref) return reference(rule)
  if ('const' in rule) return JSON.stringify(rule.const)
  if (rule.enum) return rule.enum.map(JSON.stringify).join(' | ')
  if (rule.oneOf) return rule.oneOf.map(ts).join(' | ')
  if (rule.type === 'array') return `(${ts(rule.items)})[]`
  if (rule.type === 'object') {
    if (rule.additionalProperties === true && !Object.keys(rule.properties ?? {}).length)
      return 'Record<string, Json>'
    if (Object.keys(rule.properties ?? {}).length === 0) return 'Record<string, never>'
    return (
      '{' +
      Object.entries(rule.properties ?? {})
        .map(
          ([key, value]) =>
            `${JSON.stringify(key)}${rule.required.includes(key) ? '' : '?'}: ${ts(value)}`
        )
        .join(';') +
      '}' +
      (rule.additionalProperties === true ? ' & Record<string, Json>' : '')
    )
  }
  if (rule.type === 'integer' || rule.type === 'number') return 'number'
  if (['string', 'boolean', 'null'].includes(rule.type)) return rule.type
  return 'Json'
}
function rust(rule) {
  if (rule.$ref) return reference(rule)
  if (rule.type === 'array') return `Vec<${rust(rule.items)}>`
  if (rule.type === 'string' || typeof rule.const === 'string' || rule.enum) return 'String'
  if (rule.type === 'boolean' || typeof rule.const === 'boolean') return 'bool'
  if (rule.type === 'integer' || typeof rule.const === 'number') return 'u64'
  if (rule.type === 'number') return 'f64'
  return 'serde_json::Value'
}
const snake = (name) => {
  const field = name.replace(/[A-Z]/g, (letter) => '_' + letter.toLowerCase())
  return ['ref', 'type', 'self', 'match', 'mod', 'async', 'await'].includes(field)
    ? 'r#' + field
    : field
}
const typescript = Object.entries(defs)
  .map(
    ([name, rule]) =>
      `export type ${name} = ${name === 'Json' ? 'null | boolean | number | string | Json[] | { [key: string]: Json }' : ts(rule)}`
  )
  .join('\n')
const rustTypes = Object.entries(defs)
  .map(([name, rule]) => {
    const derive = '#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]'
    if (rule.type === 'object' && rule.additionalProperties === false) {
      const literalNames = Object.fromEntries(
        Object.entries(rule.properties)
          .filter(([, value]) => typeof value.const === 'string')
          .map(([field]) => [field, name + field[0].toUpperCase() + field.slice(1)])
      )
      const literals = Object.entries(literalNames)
        .map(
          ([field, type]) =>
            `${derive}\n#[derive(Default)]\npub enum ${type} { #[default] #[serde(rename = ${JSON.stringify(rule.properties[field].const)})] Value }\n`
        )
        .join('')
      return (
        literals +
        `${derive}\n#[serde(rename_all = "camelCase", deny_unknown_fields)]\npub struct ${name} {\n` +
        Object.entries(rule.properties)
          .map(([field, value]) => {
            const required = rule.required.includes(field)
            const type = literalNames[field] ?? rust(value)
            return `${required ? '' : '#[serde(default, skip_serializing_if = "Option::is_none")]\n'}pub ${snake(field)}: ${required ? type : `Option<${type}>`},`
          })
          .join('\n') +
        '\n}'
      )
    }
    if (rule.oneOf) {
      if (rule.oneOf.every((choice) => choice.$ref))
        return (
          `${derive}\n#[serde(untagged)]\npub enum ${name} {\n` +
          rule.oneOf.map((choice) => `${reference(choice)}(${reference(choice)}),`).join('\n') +
          '\n}'
        )
    }
    return `pub type ${name} = ${rust(rule)};`
  })
  .join('\n')
const rustSource =
  `// Generated from contracts/next2/contract.json.\npub const CONTRACT_JSON: &str = r###"${JSON.stringify(contract)}"###;\n` +
  `pub const CODE_DIRECTORY: &str = ${JSON.stringify(contract.install.codeDirectory)};\n` +
  Object.entries(contract.budget)
    .map(([name, value]) => `pub const BUDGET_${snake(name).toUpperCase()}: usize = ${value};`)
    .join('\n') +
  `\n${rustTypes}\n`
const formattedRust = spawnSync('rustfmt', ['--emit', 'stdout', '--edition', '2021'], {
  input: rustSource,
  encoding: 'utf8',
  windowsHide: true
})
if (formattedRust.status !== 0) throw Error(formattedRust.stderr || 'rustfmt failed')
function pretty(source, filename) {
  const result = spawnSync(
    process.execPath,
    [resolve(root, 'node_modules/prettier/bin/prettier.cjs'), '--stdin-filepath', filename],
    { input: source, encoding: 'utf8', windowsHide: true }
  )
  if (result.status !== 0) throw Error(result.stderr || 'prettier failed')
  return result.stdout.replaceAll('\r\n', '\n')
}
const declarations = pretty(
  '// Generated from contracts/next2/contract.json.\n' + typescript + '\n',
  'generated.d.ts'
)
const outputs = {
  'packages/cruciblebox-next2-api/src/generated.mjs': pretty(
    `// Generated from contracts/next2/contract.json.\nexport const contract = ${JSON.stringify(contract)}\n`,
    'generated.mjs'
  ),
  'packages/cruciblebox-next2-api/src/generated.d.ts': declarations,
  'packages/cruciblebox-next2-api/src/generated.d.mts': declarations,
  'src-tauri/crates/next2-protocol/src/generated.rs': formattedRust.stdout.replaceAll('\r\n', '\n')
}
for (const [name, content] of Object.entries(outputs)) {
  const target = resolve(root, name)
  if (process.argv.includes('--check')) {
    if (readFileSync(target, 'utf8') !== content) throw Error('Next 2.0 generated drift: ' + name)
  } else {
    mkdirSync(dirname(target), { recursive: true })
    writeFileSync(target, content)
  }
}
console.log('Next 2.0 contract: Manifest/API 6, wire 4, data 1')
