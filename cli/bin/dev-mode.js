const { watch, existsSync } = require('fs')
const { join, resolve } = require('path')
const { execSync } = require('child_process')

function startDev(pluginDir) {
  const resolvedDir = resolve(pluginDir)
  console.log(`正在监控插件: ${resolvedDir}`)
  console.log('按 Ctrl+C 停止')

  const srcDir = join(resolvedDir, 'src')

  if (!existsSync(srcDir)) {
    console.error(`错误: 目录中不存在 src/ 子目录: ${srcDir}`)
    process.exit(1)
  }

  try {
    execSync('npm run build', { cwd: resolvedDir, stdio: 'inherit' })
    console.log('首次构建完成')
  } catch {
    console.error('构建失败')
    process.exit(1)
  }

  watch(srcDir, { recursive: true }, (event, filename) => {
    if (filename && (filename.endsWith('.ts') || filename.endsWith('.tsx'))) {
      console.log(`文件变更: ${filename}`)
      try {
        execSync('npm run build', { cwd: resolvedDir, stdio: 'pipe' })
        console.log('重新构建完成')
      } catch {
        console.error('构建失败')
      }
    }
  })
}

module.exports = { startDev }
