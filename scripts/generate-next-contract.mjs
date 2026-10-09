import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { spawnSync } from 'node:child_process'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const source = readFileSync(resolve(root, 'contracts/next/contract.json'), 'utf8')
const contract = JSON.parse(source)
if (contract.status !== 'frozen') throw new Error('Next contract must be frozen before generation')
const json = JSON.stringify(contract)
const hash = createHash('sha256').update(source).digest('hex')
function rustType(rule) {
  if (rule.type === 'string' || typeof rule.const === 'string') return 'String'
  if (rule.type === 'integer' || rule.type === 'number') return 'f64'
  if (rule.type === 'array') return 'Vec<serde_json::Value>'
  return 'serde_json::Value'
}
function tsType(rule) {
  if ('const' in rule) return JSON.stringify(rule.const)
  if (rule.enum) return rule.enum.map(JSON.stringify).join(' | ')
  if (rule.oneOf) return rule.oneOf.map(tsType).join(' | ')
  if (rule.type === 'string') return 'string'
  if (rule.type === 'integer' || rule.type === 'number') return 'number'
  if (rule.type === 'null') return 'null'
  if (rule.type === 'boolean') return 'boolean'
  if (rule.type === 'array') return '(' + tsType(rule.items) + ')[]'
  if (rule.type === 'object' && rule.additionalProperties === true) return '{ [key:string]: Json }'
  if (rule.type === 'object')
    return (
      '{' +
      Object.entries(rule.properties ?? {})
        .map(
          ([key, child]) =>
            JSON.stringify(key) +
            ((rule.required ?? []).includes(key) ? '' : '?') +
            ': ' +
            tsType(child)
        )
        .join('; ') +
      '}'
    )
  return 'Json'
}
function rustField(key) {
  const name = key.replace(/[A-Z]/g, (letter) => '_' + letter.toLowerCase())
  return [
    'type',
    'match',
    'ref',
    'self',
    'crate',
    'super',
    'mod',
    'move',
    'loop',
    'enum',
    'struct',
    'fn',
    'pub',
    'use',
    'where',
    'async',
    'await',
    'dyn',
    'impl',
    'trait',
    'const',
    'static',
    'return',
    'in',
    'as'
  ].includes(name)
    ? 'r#' + name
    : name
}
const variants = Object.entries(contract.methods).map(([method, descriptor]) => {
  const name = method
    .split('.')
    .map((part) => part[0].toUpperCase() + part.slice(1))
    .join('')
  const fields = Object.keys(descriptor.params.properties)
    .map(
      (key) =>
        `pub ${rustField(key)}: ${descriptor.params.required.includes(key) ? rustType(descriptor.params.properties[key]) : 'Option<' + rustType(descriptor.params.properties[key]) + '>'},`
    )
    .join('\n')
  return { method, name, fields }
})
const rustTypes = variants
  .map(
    ({ name, fields }) =>
      `#[derive(Debug, serde::Serialize, serde::Deserialize)]\n#[serde(rename_all = "camelCase", deny_unknown_fields)]\npub struct ${name}Params {\n${fields}\n}`
  )
  .join('\n')
const rustCall = `#[derive(Debug, serde::Serialize, serde::Deserialize)]\n#[serde(tag = "method", content = "params")]\npub enum Call {\n${variants.map(({ method, name }) => `#[serde(rename = "${method}")] ${name}(${name}Params),`).join('\n')}\n}`
const rustRequest = `#[derive(Debug, serde::Serialize, serde::Deserialize)]\n#[serde(rename_all = "camelCase")]\npub struct Request {pub wire_version: u32, pub request_id: String, pub session: String, #[serde(flatten)] pub call: Call}`
const typeRequest = variants
  .map(({ method }) => {
    const fields = Object.keys(contract.methods[method].params.properties)
      .map(
        (key) =>
          `${key}${contract.methods[method].params.required.includes(key) ? '' : '?'}: ${tsType(contract.methods[method].params.properties[key])}`
      )
      .join('; ')
    return `{ wireVersion: ${contract.versions.wireVersion}; requestId: string; session: string; method: '${method}'; params: ${fields ? `{${fields}}` : 'Record<string, never>'} }`
  })
  .join(' | ')
const responseCodes = contract.responses.failure.properties.error.properties.code.enum
const typeResponse = `{wireVersion: ${contract.versions.wireVersion}; requestId: string; ok: true; result: Json} | {wireVersion: ${contract.versions.wireVersion}; requestId: string; ok: false; error: {code: ${responseCodes.map((code) => `'${code}'`).join(' | ')}; message: string}}`
const rustResponse = `#[derive(Debug, serde::Serialize, serde::Deserialize)]\n#[serde(deny_unknown_fields)]\npub struct RpcError {pub code: String, pub message: String}\n#[derive(Debug, serde::Serialize, serde::Deserialize)]\n#[serde(rename_all = "camelCase", deny_unknown_fields)]\npub struct Success {pub wire_version: u32, pub request_id: String, pub ok: bool, pub result: serde_json::Value}\n#[derive(Debug, serde::Serialize, serde::Deserialize)]\n#[serde(rename_all = "camelCase", deny_unknown_fields)]\npub struct Failure {pub wire_version: u32, pub request_id: String, pub ok: bool, pub error: RpcError}\n#[derive(Debug, serde::Serialize, serde::Deserialize)]\n#[serde(untagged)]\npub enum Response {Success(Success), Failure(Failure)}`
const manifestFields = Object.entries(contract.manifest.properties)
const typeManifest = `{${manifestFields
  .map(([key, rule]) => {
    const optional = contract.manifest.required.includes(key) ? '' : '?'
    let type
    if ('const' in rule) type = JSON.stringify(rule.const)
    else if (rule.type === 'string') type = 'string'
    else if (rule.type === 'array')
      type = `(${rule.items.enum.map((value) => JSON.stringify(value)).join(' | ')})[]`
    else if (rule.type === 'object') type = tsType(rule)
    else throw new Error(`Unsupported manifest declaration: ${key}`)
    return `${key}${optional}: ${type}`
  })
  .join('; ')}}`
const declarations = `// Generated; edit contracts/next/contract.json.\nexport type Json = null | boolean | number | string | Json[] | { [key: string]: Json }\nexport type Request = ${typeRequest}\nexport type Response = ${typeResponse}\nexport type TaskSnapshot = {taskId:string;status: \u0027queued\u0027|\u0027running\u0027|\u0027paused\u0027|\u0027succeeded\u0027|\u0027failed\u0027|\u0027cancelled\u0027|\u0027interrupted\u0027;sequence:number;cancelRequested:boolean;resultRefs:string[];resourceKey:string;[key:string]:Json}\nexport type StorageOperation = {type:'set';key:string;value:Json} | {type:'delete';key:string}\nexport type StoragePage = {items:{key:string;value:Json}[];nextCursor:string|null}\nexport type RendererAppearance = ${tsType(contract.rendererAppearance)}\nexport declare function validateAppearance(raw:string):RendererAppearance\nexport declare function validateFilesDropped(raw:string):string[]\nexport interface RendererContext {root:HTMLElement;session:string;exchange:(request:Request)=>Promise<Response>;appearance:RendererAppearance;onFilesDropped(listener:(paths:string[])=>void):()=>void;onAppearanceChanged(listener:(appearance:RendererAppearance)=>void):()=>void}\nexport type Manifest = ${typeManifest}\nexport declare function validateManifest(raw:string):Manifest\nexport declare function validateRequest(raw: string): Request\nexport declare function validateResponse(raw: string, requestId: string): Response\nexport declare function createClient(options: {session: string; exchange: (request: Request) => Promise<Response>}): { ping(): Promise<Json>; storage: { get(key: string): Promise<Json>; set(key: string, value: Json): Promise<Json>; delete(key:string):Promise<Json>; batch(operations:StorageOperation[]):Promise<Json>; transact(operations:StorageOperation[]):Promise<Json>; keys(prefix?:string, options?:{after?:string;limit?:number}):Promise<StorageKeysPage>; list(prefix?:string, options?:{after?:string;limit?:number}):Promise<StoragePage> }; dialog: {open(options:{type:'file'|'folder';multiple?:boolean;extensions?:string[]}):Promise<string[]|null>;confirm(options:{title:string;message:string;confirmLabel?:string;cancelLabel?:string}):Promise<boolean>};notify(title:string,body?:string):Promise<boolean>; theme: {get():Promise<Json>;list():Promise<Json[]>;preview(theme:Json):Promise<boolean>;commit():Promise<boolean>;rollback():Promise<boolean>;set(theme:Json):Promise<boolean>}; tasks: {get(taskId:string):Promise<TaskSnapshot|null>;list(options?:{limit?:number;after?:string}):Promise<{items:TaskSnapshot[];nextCursor:string|null}>;cancel(taskId:string):Promise<{accepted:boolean;task:TaskSnapshot|null}>}; config: {get():Promise<Json>;patch(values:{[key:string]:Json}):Promise<Json>}; document: { call(payload: {type:string;[key:string]:Json}):Promise<Json> }; environment: { call(payload: {type:string;[key:string]:Json}):Promise<Json> }; archive: { call(payload: {type:string;[key:string]:Json}):Promise<Json> }; backend: { call(method: string, args?: Json[]): Promise<Json> } }\nexport declare const contractSha256: string\nexport declare const contract: Readonly<Record<string, unknown>>\n`
const rustManifestFields = manifestFields
  .map(([key, rule]) => {
    const name = key.replace(/[A-Z]/g, (letter) => '_' + letter.toLowerCase())
    let type =
      rule.type === 'array'
        ? 'Vec<String>'
        : rule.type === 'object'
          ? 'serde_json::Value'
          : typeof rule.const === 'number'
            ? 'u32'
            : 'String'
    if (!contract.manifest.required.includes(key)) type = `Option<${type}>`
    return `pub ${name}: ${type},`
  })
  .join('\n')
const rustManifest = `#[derive(Debug, serde::Serialize, serde::Deserialize)]\n#[serde(rename_all = "camelCase", deny_unknown_fields)]\npub struct Manifest {${rustManifestFields}}`
const rustRaw = `// Generated; edit contracts/next/contract.json.\npub const CONTRACT_JSON: &str = r#"${json}"#;\npub const CONTRACT_SHA256: &str = "${hash}";\npub const MAX_INFLIGHT: usize = ${contract.budget.inflight};\npub const HANDSHAKE_TIMEOUT_MS: u64 = ${contract.rendererTransport.handshakeTimeoutMs};\npub const RPC_TIMEOUT_MS: u64 = ${contract.rendererTransport.requestTimeoutMs};\npub const BACKEND_TIMEOUT_MS: u64 = ${contract.backendTransport.timeoutMs};\npub const MAX_FRAME_BYTES: usize = ${contract.budget.bytes};\npub const MAX_STORAGE_VALUE_BYTES: usize = ${contract.budget.storageValueBytes};\npub const MAX_STORAGE_TRANSACTION_BYTES: usize = ${contract.budget.storageTransactionBytes};\npub const MAX_STORAGE_CHUNK_BYTES: usize = ${contract.budget.storageChunkBytes};\npub const MAX_STORAGE_TRANSACTION_OPS: usize = ${contract.budget.storageTransactionOps};\npub const LEASE_MS: u64 = ${contract.rendererTransport.leaseMs};\npub const MIN_LEASE_MS: u64 = ${contract.rendererTransport.minLeaseMs};\npub const MAX_LEASE_MS: u64 = ${contract.rendererTransport.maxLeaseMs};\npub const BACKEND_QUEUE: usize = ${contract.backendTransport.maxQueue};\npub const MAX_BACKEND_WORKERS: usize = ${contract.backendTransport.maxWorkers};\n${rustTypes}\n${rustCall}\n${rustRequest}\n${rustResponse}\n${rustManifest}\n`
const formatted = spawnSync('rustfmt', ['--emit', 'stdout', '--edition', '2021'], {
  input: rustRaw,
  encoding: 'utf8',
  windowsHide: true
})
if (formatted.status !== 0) throw new Error(formatted.stderr || 'rustfmt unavailable')
function pretty(value, filename) {
  const result = spawnSync(
    process.execPath,
    [resolve(root, 'node_modules/prettier/bin/prettier.cjs'), '--stdin-filepath', filename],
    { input: value, encoding: 'utf8', windowsHide: true }
  )
  if (result.status !== 0) throw new Error(result.stderr || 'prettier unavailable')
  return result.stdout.replace(/\r\n/g, '\n')
}
const outputs = {
  'packages/cruciblebox-next-api/src/generated.mjs': pretty(
    `// Generated; edit contracts/next/contract.json.\nexport const contract = ${json}\nexport const contractSha256 = '${hash}'\n`,
    'generated.mjs'
  ),
  'src-tauri/crates/next-protocol/src/generated.rs':
    formatted.stdout.replace(/\r\n/g, '\n').trimEnd() + '\n',
  'packages/cruciblebox-next-api/src/index.d.ts': pretty(declarations, 'index.d.ts'),
  'packages/cruciblebox-next-api/src/index.d.mts': pretty(declarations, 'index.d.mts')
}
for (const [path, output] of Object.entries(outputs)) {
  const target = resolve(root, path)
  if (process.argv.includes('--check')) {
    if (readFileSync(target, 'utf8') !== output) throw new Error(`Generated drift: ${path}`)
  } else {
    mkdirSync(dirname(target), { recursive: true })
    writeFileSync(target, output)
  }
}
console.log('Next frozen contract ' + hash)
