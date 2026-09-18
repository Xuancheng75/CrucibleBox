import { spawnSync } from 'node:child_process'

export function spawnFileSync(file, args) {
  const result = spawnSync(file, args, { stdio: 'inherit', windowsHide: true })
  if (result.error) throw result.error
  if (result.status !== 0) throw new Error(`Command failed (${result.status}): ${file}`)
}
