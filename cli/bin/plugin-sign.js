// CrucibleBox CLI — sign 插件产物清单（复用 plugin-artifact-provenance 的 Ed25519 canonical JSON 签名）
// 语义与 scripts/sign-plugin-artifacts.mjs 对等，但面向插件作者独立签名：
//   对 artifacts/plugins/manifest.json 签名 → manifest.sig.json
// 密钥来源优先级：--key-file <pem> | env OPENBOX_PLUGIN_SIGNING_KEY_FILE
// keyId 来源：--key-id <id> | env OPENBOX_PLUGIN_SIGNING_KEY_ID

const { existsSync, readFileSync, writeFileSync } = require('fs')
const { dirname, join, resolve } = require('path')

const REPOSITORY_ROOT = resolve(__dirname, '..', '..')
const ARTIFACT_DIR = join(REPOSITORY_ROOT, 'artifacts', 'plugins')
const MANIFEST_PATH = join(ARTIFACT_DIR, 'manifest.json')

function canonicalJson(value) {
  if (value === null || typeof value === 'boolean' || typeof value === 'string') {
    return JSON.stringify(value)
  }
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) throw new TypeError('Canonical JSON only supports finite numbers')
    return JSON.stringify(value)
  }
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`
  if (typeof value === 'object') {
    const entries = Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
    return `{${entries.join(',')}}`
  }
  throw new TypeError(`Canonical JSON does not support ${typeof value}`)
}

function parseArgs(args) {
  const out = { keyFile: null, keyId: null }
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--key-file') out.keyFile = args[++i]
    else if (args[i] === '--key-id') out.keyId = args[++i]
  }
  return out
}

function run() {
  const { keyFile, keyId } = parseArgs(process.argv.slice(3))
  const privateKeyPath = keyFile || process.env.OPENBOX_PLUGIN_SIGNING_KEY_FILE
  const signingKeyId = keyId || process.env.OPENBOX_PLUGIN_SIGNING_KEY_ID

  if (!privateKeyPath || !signingKeyId) {
    console.error('错误: 需要私钥文件与 keyId')
    console.log('用法: openbox sign --key-file <pem> --key-id <id>')
    console.log('      或设置环境变量 OPENBOX_PLUGIN_SIGNING_KEY_FILE / OPENBOX_PLUGIN_SIGNING_KEY_ID')
    process.exit(1)
  }
  if (!existsSync(MANIFEST_PATH)) {
    console.error('错误: 未找到产物清单（先运行 npm run package:plugins）:', MANIFEST_PATH)
    process.exit(1)
  }

  const { createPrivateKey, sign } = require('node:crypto')
  const manifest = JSON.parse(readFileSync(MANIFEST_PATH, 'utf8'))
  let key
  try {
    key = createPrivateKey(readFileSync(resolve(privateKeyPath)))
  } catch (err) {
    console.error('错误: 无法读取 Ed25519 私钥:', err.message)
    process.exit(1)
  }
  if (key.asymmetricKeyType !== 'ed25519') {
    console.error('错误: 插件签名密钥必须是 Ed25519')
    process.exit(1)
  }
  const signature = sign(null, Buffer.from(canonicalJson(manifest)), key).toString('base64')
  writeFileSync(
    join(ARTIFACT_DIR, 'manifest.sig.json'),
    `${JSON.stringify({ algorithm: 'Ed25519', keyId: signingKeyId, signature }, null, 2)}\n`,
    'utf8'
  )
  console.log(`✅ 已签名产物清单（keyId: ${signingKeyId}）→ artifacts/plugins/manifest.sig.json`)
}

module.exports = { run, canonicalJson }
