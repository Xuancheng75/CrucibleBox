import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const repositoryRoot = resolve(scriptDirectory, '..')
const policies = JSON.parse(
  readFileSync(resolve(repositoryRoot, 'shared', 'trusted-service-policies.json'), 'utf8')
)
const catalog = JSON.parse(readFileSync(resolve(scriptDirectory, 'plugin-catalog.json'), 'utf8'))

for (const [serviceName, policy] of Object.entries(policies)) {
  const catalogEntry = catalog.find((entry) => entry.id === policy.name)
  if (!catalogEntry) throw new Error(`${serviceName}: plugin is missing from the catalog`)
  const expectedFiles = [...policy.files].sort()
  const runtimeFiles = [...catalogEntry.runtimeFiles].sort()
  if (JSON.stringify(expectedFiles) !== JSON.stringify(runtimeFiles)) {
    throw new Error(`${serviceName}: trusted policy and runtime catalog file sets differ`)
  }

  const pluginDirectory = resolve(repositoryRoot, 'plugins', policy.name)
  const manifest = JSON.parse(readFileSync(resolve(pluginDirectory, 'plugin.json'), 'utf8'))
  if (
    manifest.name !== policy.name ||
    manifest.version !== policy.version ||
    manifest.manifestVersion !== 2 ||
    manifest.backendApiVersion !== 2 ||
    JSON.stringify(manifest.permissions) !== JSON.stringify([`trusted:${serviceName}`])
  ) {
    throw new Error(`${serviceName}: manifest does not match the trusted-service policy`)
  }

  const hash = createHash('sha256')
  for (const file of expectedFiles) {
    hash.update(file)
    hash.update('\0')
    hash.update(readFileSync(join(pluginDirectory, ...file.split('/'))))
    hash.update('\0')
  }
  const digest = hash.digest('hex')
  if (digest !== policy.digest) {
    throw new Error(
      `${serviceName}: digest mismatch; expected ${policy.digest}, received ${digest}`
    )
  }
  console.log(`[trusted-services] verified ${serviceName} ${policy.version} sha256=${digest}`)
}
