import {
  FuseState,
  FuseV1Options,
  FuseVersion,
  flipFuses,
  getCurrentFuseWire
} from '@electron/fuses'

export const ELECTRON_FUSE_CONFIG = Object.freeze({
  version: FuseVersion.V1,
  strictlyRequireAllFuses: true,
  [FuseV1Options.RunAsNode]: false,
  [FuseV1Options.EnableCookieEncryption]: true,
  [FuseV1Options.EnableNodeOptionsEnvironmentVariable]: false,
  [FuseV1Options.EnableNodeCliInspectArguments]: false,
  [FuseV1Options.EnableEmbeddedAsarIntegrityValidation]: true,
  [FuseV1Options.OnlyLoadAppFromAsar]: true,
  [FuseV1Options.LoadBrowserProcessSpecificV8Snapshot]: false,
  [FuseV1Options.GrantFileProtocolExtraPrivileges]: false,
  [FuseV1Options.WasmTrapHandlers]: true
})

const EXPECTED_FUSE_STATES = Object.freeze({
  [FuseV1Options.RunAsNode]: FuseState.DISABLE,
  [FuseV1Options.EnableCookieEncryption]: FuseState.ENABLE,
  [FuseV1Options.EnableNodeOptionsEnvironmentVariable]: FuseState.DISABLE,
  [FuseV1Options.EnableNodeCliInspectArguments]: FuseState.DISABLE,
  [FuseV1Options.EnableEmbeddedAsarIntegrityValidation]: FuseState.ENABLE,
  [FuseV1Options.OnlyLoadAppFromAsar]: FuseState.ENABLE,
  [FuseV1Options.LoadBrowserProcessSpecificV8Snapshot]: FuseState.DISABLE,
  [FuseV1Options.GrantFileProtocolExtraPrivileges]: FuseState.DISABLE,
  [FuseV1Options.WasmTrapHandlers]: FuseState.ENABLE
})

export function assertElectronFuseWire(wire) {
  if (wire.version !== FuseVersion.V1) {
    throw new Error(`Unsupported Electron fuse wire version: ${wire.version}`)
  }

  const indexes = Object.keys(wire)
    .filter((key) => /^\d+$/u.test(key))
    .map(Number)
    .sort((left, right) => left - right)
  const expectedIndexes = Object.keys(EXPECTED_FUSE_STATES).map(Number)
  if (
    indexes.length !== expectedIndexes.length ||
    indexes.some((value, index) => value !== expectedIndexes[index])
  ) {
    throw new Error(
      `Electron fuse wire schema changed: expected ${expectedIndexes.length} fuses, received ${indexes.length}`
    )
  }

  for (const [index, expected] of Object.entries(EXPECTED_FUSE_STATES)) {
    if (wire[index] !== expected) {
      throw new Error(
        `Electron fuse ${FuseV1Options[Number(index)]} has state ${wire[index]}, expected ${expected}`
      )
    }
  }
}

export async function verifyElectronFuses(executable) {
  const wire = await getCurrentFuseWire(executable)
  assertElectronFuseWire(wire)
  return wire
}

export async function applyElectronFuses(executable) {
  await flipFuses(executable, ELECTRON_FUSE_CONFIG)
  return verifyElectronFuses(executable)
}
