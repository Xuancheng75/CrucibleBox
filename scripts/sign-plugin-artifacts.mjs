import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { signPluginArtifactManifest } from './plugin-artifact-provenance.mjs'

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const repositoryRoot = resolve(scriptDirectory, '..')
const artifactDirectory = resolve(repositoryRoot, 'artifacts', 'plugins')
const manifestPath = resolve(artifactDirectory, 'manifest.json')
const privateKeyPath = process.env.OPENBOX_PLUGIN_SIGNING_KEY_FILE
const keyId = process.env.OPENBOX_PLUGIN_SIGNING_KEY_ID

if (!privateKeyPath || !keyId) {
  throw new Error(
    'OPENBOX_PLUGIN_SIGNING_KEY_FILE and OPENBOX_PLUGIN_SIGNING_KEY_ID are required for release signing'
  )
}
if (!existsSync(manifestPath)) throw new Error('Package plugin artifacts before signing them')

const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'))
const signature = signPluginArtifactManifest(manifest, readFileSync(resolve(privateKeyPath)))
writeFileSync(
  resolve(artifactDirectory, 'manifest.sig.json'),
  `${JSON.stringify({ algorithm: 'Ed25519', keyId, signature }, null, 2)}\n`
)
console.log(`[plugins] signed artifact manifest with key ${keyId}`)
