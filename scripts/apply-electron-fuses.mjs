import { join } from 'node:path'
import { applyElectronFuses } from './electron-fuses.mjs'

function resolveExecutable(context) {
  const productFilename = context.packager.appInfo.productFilename
  switch (context.electronPlatformName) {
    case 'darwin':
      return join(context.appOutDir, `${productFilename}.app`)
    case 'linux':
      return join(context.appOutDir, context.packager.executableName)
    case 'win32':
      return join(context.appOutDir, `${productFilename}.exe`)
    default:
      throw new Error(`Unsupported Electron packaging platform: ${context.electronPlatformName}`)
  }
}

export async function afterPack(context) {
  const executable = resolveExecutable(context)
  await applyElectronFuses(executable)
  console.log(`  • verified Electron fuses  executable=${executable}`)
}
