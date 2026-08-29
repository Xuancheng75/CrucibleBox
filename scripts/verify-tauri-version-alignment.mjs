import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { readTauriVersion } from './tauri-version.mjs'

const repositoryRoot = resolve(fileURLToPath(new URL('..', import.meta.url)))
const expected = readTauriVersion(repositoryRoot)

function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'))
}

function readCargoVersion(path) {
  const source = readFileSync(path, 'utf8')
  const packageSection = source.match(/^\[package\]\r?\n([\s\S]*?)(?=\r?\n\[|$)/u)?.[1] ?? ''
  return packageSection.match(/^version\s*=\s*"([^"]+)"/mu)?.[1]
}

function readLockVersion(path, packageName) {
  const source = readFileSync(path, 'utf8')
  const escaped = packageName.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&')
  return source.match(new RegExp(`name = "${escaped}"\\r?\\nversion = "([^"]+)"`, 'u'))?.[1]
}

const checks = [
  [
    'src-tauri/tauri.conf.json',
    readJson(resolve(repositoryRoot, 'src-tauri/tauri.conf.json')).version
  ],
  ['src-tauri/Cargo.toml', readCargoVersion(resolve(repositoryRoot, 'src-tauri/Cargo.toml'))],
  [
    'src-tauri/cruciblebox-plugin-host/Cargo.toml',
    readCargoVersion(resolve(repositoryRoot, 'src-tauri/cruciblebox-plugin-host/Cargo.toml'))
  ],
  [
    'src-tauri/Cargo.lock (cruciblebox)',
    readLockVersion(resolve(repositoryRoot, 'src-tauri/Cargo.lock'), 'cruciblebox')
  ],
  [
    'src-tauri/cruciblebox-plugin-host/Cargo.lock (cruciblebox-plugin-host)',
    readLockVersion(
      resolve(repositoryRoot, 'src-tauri/cruciblebox-plugin-host/Cargo.lock'),
      'cruciblebox-plugin-host'
    )
  ],
  [
    'tauri-frontend/package.json',
    readJson(resolve(repositoryRoot, 'tauri-frontend/package.json')).version
  ],
  [
    'tauri-frontend/package-lock.json',
    readJson(resolve(repositoryRoot, 'tauri-frontend/package-lock.json')).packages?.['']?.version
  ]
]

const mismatches = checks.filter(([, version]) => version !== expected)
if (mismatches.length > 0) {
  throw new Error(
    `Tauri version drift: expected ${expected}; ${mismatches
      .map(([path, version]) => `${path}=${version ?? '<missing>'}`)
      .join(', ')}`
  )
}

const legacyRoot = readJson(resolve(repositoryRoot, 'package.json')).version
console.log(
  `[version] Tauri ${expected} aligned across ${checks.length} files; root Electron legacy package remains ${legacyRoot}`
)
