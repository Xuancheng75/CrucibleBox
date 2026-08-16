# CrucibleBox 插件开发指南

> 面向插件开发者的实操指南。契约基准见 `docs/plugin-sdk.md`（Manifest v2 / backend API v2 / renderer API v2，已冻结）。
> 模板在 `templates/plugin-template`。本文聚焦「从模板到可导入安装包」的完整流程、构建、导入、调试与发布。

## 1. 从模板开始

```bash
# 复制模板
cp -r templates/plugin-template plugins/<your-plugin-id>
```

模板结构：

```
plugins/<your-plugin-id>/
  plugin.json          # Manifest v2（必填字段见 plugin-sdk.md §1）
  src/
    main.ts            # backend 入口（renderer-only 可留占位）
    renderer-entry.tsx # renderer 入口（window.__OPENBOX_PLUGIN_RUNTIME__.mount）
    renderer.tsx       # 插件 React 组件（收到 PluginRenderProps）
  scripts/
    build-plugin-renderer.mjs  # esbuild 双产物（dist/main.js + dist/renderer.js）
  tsconfig.json / tsconfig.build.json
  package.json         # 版本须与 plugin.json 一致
```

## 2. 编写插件

### 2.1 plugin.json

```jsonc
{
  "name": "my-plugin",            // 插件 ID（唯一，snake_case）
  "version": "0.1.0",             // 与 package.json 版本一致（构建门禁校验）
  "displayName": "我的插件",
  "author": "you",
  "main": "dist/main.js",         // backend 入口（renderer-only 仍需占位）
  "renderer": "dist/renderer.js",
  "manifestVersion": 2,
  "rendererApiVersion": 2,
  "backend": false,               // renderer-only 时可省略 backendApiVersion
  "permissions": ["storage:read", "storage:write"], // 只声明实际使用的权限
  "config": {
    "example": {
      "type": "string",
      "label": "示例配置",
      "default": ""
    }
  }
}
```

- 权限全集见 `plugin-sdk.md §5`：`storage:read/write`、`notification`、`network:fetch`、`clipboard`、`dialog`、`shortcut`、`file:read/write`、`theme:write`、`trusted:unienv`。
- **高权限能力（下载/解压/环境修改）不开放给通用插件**：应设计为宿主固定服务（UniEnv 模式），插件经 `api.invokeTrustedService` 调用。

### 2.2 Renderer（browser IIFE）

- 自包含 bundle：**无运行时 require()、不读 window.electronAPI / 父窗口 / Node 全局**。
- 能力仅来自 `PluginRenderProps.api`：
  - `sendToBackend(message)` / `onBackendMessage(handler)`
  - `notify({ title, body })`、`confirm(options)`
  - `theme.get()` / `theme.set()`（需 `theme:write`）
- 跨源 sandboxed iframe 运行；样式用 `var(--ob-*)` CSS 变量 + `props.theme` 快照。

```tsx
import React, { useEffect, useState } from 'react'
import type { PluginRenderProps } from 'cruciblebox-plugin-api'

export default function MyPlugin({ api }: PluginRenderProps) {
  const [data, setData] = useState<string[]>([])
  useEffect(() => {
    api.sendToBackend({ type: 'list' }).then((result) => {
      setData((result as { items: string[] }).items ?? [])
    })
  }, [api])
  return <ul>{data.map((d) => <li key={d}>{d}</li>)}</ul>
}
```

### 2.3 Backend（utility process，CJS）

```ts
import type { PluginContext, PluginMain } from 'cruciblebox-plugin-api'

let ctx: PluginContext | null = null

const plugin: PluginMain = {
  activate(pluginCtx: PluginContext) {
    ctx = pluginCtx
    ctx.logger.info('my-plugin activated')
  },
  deactivate() {
    ctx = null
  },
  async onMessage(message: unknown) {
    const msg = message as { type: string }
    switch (msg.type) {
      case 'list':
        return { items: await ctx!.storage.list<string>('items:') }
      default:
        return { error: `unknown message: ${msg.type}` }
    }
  }
}
export default plugin
```

- 所有宿主能力走异步 SDK：`ctx.storage.*`、`ctx.database.*`、`ctx.api.*`（notify/dialog/fetch/readFile/writeFile/registerShortcut/onEvent/emitEvent/invokeTrustedService）、`ctx.logger`。
- **activate 内不做不可回滚的数据修改**；deactivate 停止定时器/订阅/后台任务。

## 3. 构建

```bash
# 单插件
cd plugins/<your-plugin-id>
npm run build        # 产出 dist/main.js + dist/renderer.js（esbuild）

# 全部插件 + 校验（仓库根）
npm run build:plugins
```

- 产物：`dist/main.js`（backend，CJS）+ `dist/renderer.js`（renderer，browser IIFE）。
- 版本一致性：`package.json` / `plugin.json` / lockfile 三处必须一致（构建门禁）。
- 构建后跑 `npm run verify:plugins` 校验 renderer 自包含、可信服务摘要、产物完整性。

## 4. 导入与安装

导入插件有两种方式（宿主「导入插件」弹窗）：

1. **选择插件目录**：选择包含 `plugin.json` + `dist/` 的目录。
2. **选择/拖拽 .zip 插件包**：ZIP 根或唯一子目录含 `plugin.json`。

安装流程（宿主后端事务链）：

```
preview（校验 manifest/权限/升级策略）→ 用户确认 → commit（原子 swap + DB 更新）
```

- 升级：预览会列出新增/移除的权限并要求二次确认；迁移不删配置/存储/目录。
- 新安装默认 **disabled**（不自动激活），需在插件列表手动启用。
- 导入路径必须来自用户对话框/拖拽（受可信路径白名单保护）。

## 5. 调试

- **renderer 日志**：宿主日志页查看 `plugin:log` 事件；插件 `console.*` 走宿主 stderr。
- **backend 日志**：`ctx.logger.*` 写入 DB `plugin_logs` 表 + 广播 `plugin:log`。
- **sidecar stderr**：宿主侧 `[host] ...` 前缀输出到进程 stderr（崩溃/加载错误）。
- **renderer RPC 错误**：`INTERNAL_ERROR: Plugin renderer request failed` 表示 renderer→backend 通信失败；结合宿主 stderr / DB 日志定位。
- **主题**：插件在跨源 iframe，`getComputedStyle` 读 `--ob-*` 并监听 `theme.changed` 事件重绘。

## 6. 权限申请（重要）

- 只声明实际使用的权限；宿主在**主进程统一断言**（PermissionGuard），越权请求返回 `NOT_ALLOWED`。
- 需宿主新增能力（如新 trusted service、新 host 方法）：在 `src-tauri` 侧实现并加白名单，同步 `plugin-sdk.md` 与本文档。

## 7. 打包发布（插件包）

```bash
npm run package:plugins   # 生成确定性 ZIP（含逐文件 SHA-256 清单）
npm run sign:plugins      # Ed25519 签名（canonical JSON）
npm run verify:release    # 完整性 + 摘要校验
```

- 发布产物可分发到仓库 `plugins/` 或外部；宿主导入时校验签名与权限。
- 完整宿主发布链见 `docs/release-runbook.md`。

## 8. 检查单（提交前）

- [ ] `plugin.json` 版本与 `package.json` 一致
- [ ] `dist/main.js`（若有 backend）与 `dist/renderer.js` 已构建
- [ ] `permissions` 仅含实际使用项
- [ ] renderer 无 `require` / Node 全局 / 父窗口访问
- [ ] `npm run verify:plugins` 通过
- [ ] 在宿主中完成一次「导入 → 启用 → 打开 → 操作 → 关闭」冒烟
