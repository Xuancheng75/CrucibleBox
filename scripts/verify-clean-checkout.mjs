import { execFileSync } from 'node:child_process'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

/**
 * verify-clean-checkout — clean checkout 门禁。
 *
 * 1. 构建/校验流程结束后，git 工作树必须保持干净（没有意外产生的追踪文件改动），
 *    证明构建是确定性的、被 gitignore 的产物（dist/out/release/artifacts）不会污染追踪状态。
 * 2. 所有被 git 索引的文本文件行尾必须为 LF（通过 `git ls-files --eol` 检查索引内容，
 *    不读工作树字节，避免本地 autocrlf 检出产物的干扰）。杜绝换行漂移破坏
 *    逐字节比对（插件 ZIP、trusted digest、checksum）。
 *
 * 用法：node scripts/verify-clean-checkout.mjs
 * 退出码：0 = 通过；1 = 任一检查失败（fail-closed）。
 */

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const repositoryRoot = resolve(scriptDirectory, '..')

function git(args) {
  return execFileSync('git', args, { cwd: repositoryRoot, encoding: 'utf8', windowsHide: true })
}

function checkCleanTree() {
  const status = git(['status', '--porcelain'])
  if (status.trim().length > 0) {
    throw new Error(
      `Git working tree is not clean after build/verify:\n${status
        .split('\n')
        .slice(0, 30)
        .join('\n')}\n` +
        'Generated or pinned files changed unexpectedly. If a tracked file must change, ' +
        'commit it intentionally in the same PR.'
    )
  }
  console.log('[clean-checkout] git working tree is clean')
}

function checkIndexLineEndings() {
  const eolLines = git(['ls-files', '--eol']).split('\n').filter(Boolean)
  const offenders = []
  for (const line of eolLines) {
    // 格式：i/<eol> w/<eol> attr/<attrs> <path>
    const match = line.match(/^i\/(\S+)\s+w\/(\S+)\s+attr\/[^\t]*\t(.+)$/u)
    if (!match) continue
    const [indexEol, , path] = [match[1], match[2], match[3]]
    if (indexEol === 'crlf') {
      offenders.push(`${path} (index=${indexEol})`)
    }
  }
  if (offenders.length > 0) {
    throw new Error(
      `Tracked text files in the index contain CRLF (LF expected, per .gitattributes):\n${offenders
        .slice(0, 30)
        .join('\n')}\n` +
        'Normalize line endings (git add --renormalize) and commit before merging.'
    )
  }
  console.log(`[clean-checkout] index text line endings are LF (${eolLines.length} tracked files)`)
}

function main() {
  checkCleanTree()
  checkIndexLineEndings()
  console.log('[clean-checkout] OK')
}

main()
