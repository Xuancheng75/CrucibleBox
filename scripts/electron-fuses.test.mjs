import assert from 'node:assert/strict'
import { test } from 'node:test'
import { FuseState, FuseV1Options, FuseVersion } from '@electron/fuses'
import { assertElectronFuseWire } from './electron-fuses.mjs'

function validWire() {
  return {
    version: FuseVersion.V1,
    [FuseV1Options.RunAsNode]: FuseState.DISABLE,
    [FuseV1Options.EnableCookieEncryption]: FuseState.ENABLE,
    [FuseV1Options.EnableNodeOptionsEnvironmentVariable]: FuseState.DISABLE,
    [FuseV1Options.EnableNodeCliInspectArguments]: FuseState.DISABLE,
    [FuseV1Options.EnableEmbeddedAsarIntegrityValidation]: FuseState.ENABLE,
    [FuseV1Options.OnlyLoadAppFromAsar]: FuseState.ENABLE,
    [FuseV1Options.LoadBrowserProcessSpecificV8Snapshot]: FuseState.DISABLE,
    [FuseV1Options.GrantFileProtocolExtraPrivileges]: FuseState.DISABLE,
    [FuseV1Options.WasmTrapHandlers]: FuseState.ENABLE
  }
}

test('accepts the complete hardened Electron fuse policy', () => {
  assert.doesNotThrow(() => assertElectronFuseWire(validWire()))
})

test('rejects an insecure or incomplete fuse wire', () => {
  assert.throws(
    () => assertElectronFuseWire({ ...validWire(), [FuseV1Options.RunAsNode]: FuseState.ENABLE }),
    /RunAsNode/u
  )
  const incomplete = validWire()
  delete incomplete[FuseV1Options.WasmTrapHandlers]
  assert.throws(() => assertElectronFuseWire(incomplete), /schema changed/u)
})

test('rejects a future fuse schema until its state is explicitly reviewed', () => {
  assert.throws(
    () => assertElectronFuseWire({ ...validWire(), 9: FuseState.ENABLE }),
    /schema changed/u
  )
})
