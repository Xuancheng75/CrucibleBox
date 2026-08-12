const { readFileSync, writeFileSync, existsSync, mkdirSync } = require('fs')
const { join, resolve } = require('path')

function toDisplayName(name) {
  return name
    .split(/[-_]/)
    .map((s) => s.charAt(0).toUpperCase() + s.slice(1))
    .join(' ')
}

function getAllFiles(dir) {
  const { readdirSync, statSync } = require('fs')
  const result = []
  const entries = readdirSync(dir)
  for (const entry of entries) {
    const fullPath = join(dir, entry)
    const stat = statSync(fullPath)
    if (stat.isDirectory()) {
      result.push(...getAllFiles(fullPath))
    } else {
      result.push(fullPath)
    }
  }
  return result
}

function run(pluginName) {
  if (!pluginName) {
    console.error('错误: 请输入插件名称')
    console.log('用法: openbox create-plugin <插件名称>')
    process.exit(1)
  }

  if (!/^[a-z0-9_-]+$/.test(pluginName)) {
    console.error('错误: 插件名称只能包含小写字母、数字、中划线和下划线')
    process.exit(1)
  }

  const targetDir = resolve(process.cwd(), pluginName)

  if (existsSync(targetDir)) {
    console.error(`错误: 目录 "${pluginName}" 已存在`)
    process.exit(1)
  }

  const templateDir = join(__dirname, '..', '..', 'templates', 'plugin-template')

  if (!existsSync(templateDir)) {
    console.error('错误: 未找到插件模板:', templateDir)
    process.exit(1)
  }

  const displayName = toDisplayName(pluginName)

  const files = getAllFiles(templateDir)
  for (const file of files) {
    const relative = file.slice(templateDir.length).replace(/\\/g, '/')
    const destPath = join(targetDir, relative)
    const parentDir = join(destPath, '..')

    if (!existsSync(parentDir)) {
      mkdirSync(parentDir, { recursive: true })
    }

    let content = readFileSync(file, 'utf-8')
    content = content
      .replace(/{{pluginName}}/g, pluginName)
      .replace(/{{displayName}}/g, displayName)

    writeFileSync(destPath, content, 'utf-8')
  }

  console.log(`
\u2705 插件 "${pluginName}" 创建成功！

目录: ${targetDir}

下一步:
  1. cd ${pluginName}
  2. npm install
  3. 编辑 src/main.ts 和 src/renderer.tsx
  4. 构建: npm run build
  5. 将插件目录导入 OpenBox

插件结构:
  ${pluginName}/
  \u2514\u2500\u2500 plugin.json         # 插件清单
  \u2514\u2500\u2500 tsconfig.json
  \u2514\u2500\u2500 src/
     \u2514\u2500\u2500 main.ts         # 后端 (主进程)
     \u2514\u2500\u2500 renderer.tsx    # 前端 (React 组件)
  \u2514\u2500\u2500 dist/               # 构建输出
  `)
}

module.exports = { run }
