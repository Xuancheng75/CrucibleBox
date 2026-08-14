// CrucibleBox CLI — bump 插件版本（package.json + plugin.json 同步，对等 pluginSources 契约）
const { readFileSync, writeFileSync, existsSync } = require('fs')
const { join, resolve } = require('path')

const SEMVER = /^(\d+)\.(\d+)\.(\d+)(-[\w.-]+)?$/

function bumpVersion(version, type) {
  const match = SEMVER.exec(version)
  if (!match) throw new Error(`无效版本号: ${version}`)
  const [, major, minor, patch] = match.map((n) => (n === undefined ? undefined : Number(n)))
  const pre = match[4] ?? ''
  switch (type) {
    case 'patch':
      return `${major}.${minor}.${patch + 1}`
    case 'minor':
      return `${major}.${minor + 1}.0`
    case 'major':
      return `${major + 1}.0.0`
    case 'prerelease':
      if (pre) return `${major}.${minor}.${patch}${pre.includes('-') ? pre.replace(/-([\w.-]+)$/, '-$1') : ''}`
      return `${major}.${minor}.${patch + 1}-beta.1`
    default:
      throw new Error(`未知 bump 类型: ${type}（可选 patch|minor|major|prerelease）`)
  }
}

function run(pluginPath, type = 'patch') {
  if (!pluginPath) {
    console.error('错误: 请输入插件目录')
    console.log('用法: openbox bump <插件目录> [patch|minor|major|prerelease]')
    process.exit(1)
  }
  const dir = resolve(process.cwd(), pluginPath)
  const packagePath = join(dir, 'package.json')
  const manifestPath = join(dir, 'plugin.json')
  if (!existsSync(packagePath) || !existsSync(manifestPath)) {
    console.error(`错误: 目录 "${dir}" 不是插件工程（缺 package.json 或 plugin.json）`)
    process.exit(1)
  }

  const pkg = JSON.parse(readFileSync(packagePath, 'utf8'))
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'))
  if (!pkg.version || !manifest.version) {
    console.error('错误: package.json / plugin.json 缺少 version')
    process.exit(1)
  }
  if (pkg.version !== manifest.version) {
    console.error(`错误: 版本不一致（package.json=${pkg.version} plugin.json=${manifest.version}），请先对齐`)
    process.exit(1)
  }

  const next = bumpVersion(pkg.version, type)
  pkg.version = next
  manifest.version = next
  writeFileSync(packagePath, `${JSON.stringify(pkg, null, 2)}\n`, 'utf8')
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8')
  console.log(`✅ ${pkg.name || dir}: ${next}`)
  console.log('   提醒: 版本变更后重新构建（npm run build）并重新打包签名。')
}

module.exports = { run }
