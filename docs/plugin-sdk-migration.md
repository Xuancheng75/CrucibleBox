# 插件 SDK v2 迁移指南

## 1. 更新 manifest

```json
{
  "manifestVersion": 2,
  "backendApiVersion": 2,
  "rendererApiVersion": 2,
  "permissions": ["storage:read", "storage:write", "notification"]
}
```

只保留实际使用的权限。新插件不申请 `database:read/write`；只有旧插件兼容期允许原始 SQL。
缺少 `manifestVersion`（或显式为 1）的已安装包仍由 Legacy Full Trust 兼容适配器运行，但宿主不再接受
新的 v1 安装或升级。新发布必须使用 v2，且 backend/renderer API 都必须为 2。升级确认会单独列出新增和
移除的权限；迁移不会删除既有插件配置、存储或目录。

不需要后台逻辑的插件声明 `"backend": false`，可省略 `backendApiVersion`。为兼容当前安装元数据和制品
布局，`main` 入口仍需存在，但宿主只校验文件而不会执行它。renderer-only 插件不得调用
`sendToBackend`；宿主会返回确定性错误。

## 2. 构建 renderer

renderer 必须是自包含 browser IIFE，不得保留运行时 `require()`，也不得读取
`window.electronAPI`、父窗口或 Node 全局。React 和 JSX runtime 一并打入 bundle。宿主提供的能力只来自
`PluginRenderProps.api`：

- `sendToBackend`
- `notify`
- `confirm`
- `onBackendMessage`
- `theme.get/set`

下载可使用用户激活触发的 Blob；系统确认使用异步 `api.confirm`，不要依赖 iframe modal 权限。

## 3. 使用私有存储

```ts
const value = await ctx.storage.get<{ count: number }>('counter')
await ctx.storage.set('counter', { count: (value?.count ?? 0) + 1 })
const entries = await ctx.storage.list<{ title: string }>('entry:')
await ctx.storage.delete('counter')
```

键最长 256 字符并拒绝控制字符；值必须是有限、无环的 JSON，单值不超过 1 MiB。namespace 由宿主绑定，
不要把插件名或 ID 拼入全局表名。KV 不提供跨键事务；需要多个字段原子更新时保存为一个 JSON 文档。

### 从旧 SQL 迁移

1. 先发布仍能读取旧表、但能写新格式的过渡版本。
2. 将迁移实现为有版本号、幂等且保留旧数据的宿主迁移。
3. 新版本移除 `database:*` 权限，只使用 `storage:*`。
4. 验证旧数据副本、重复启动、迁移失败回滚、升级回滚与空数据库。
5. 在明确结束降级窗口前不要删除旧表。

Diary/Turntable 的宿主迁移可作为参考：已有新键优先，迁移 marker 防止旧数据重复注入。

## 4. Backend API v2

backend 继续导出 `activate/deactivate/onMessage`。所有宿主能力都是异步 SDK 调用；不要访问宿主环境变量
或假设 worker PATH。长任务应立即返回 taskId，并由 renderer 轮询状态和显式取消。

普通 backend 仍是可信 Node 代码。需要 shell、下载、全局文件写入等高权限流程时，不要扩大通用插件
能力；应设计宿主持有的固定服务、严格操作白名单、输入协议、资源预算与摘要策略。

## 5. 主题

CSS 使用 `var(--ob-*)`；renderer 通过 `props.theme`/`api.theme.get()` 获取快照。canvas 绘制从计算样式
解析颜色并监听主题事件。只有主题管理类插件申请 `theme:write`。

## 6. 生命周期与升级

- `activate` 必须可重复启动且失败时抛出明确错误。
- `deactivate` 必须停止计时器、订阅、快捷键与后台任务。
- 不在 `activate` 中执行不可回滚的数据修改。
- manifest、package、lockfile 版本保持一致。
- 每次发布执行 typecheck、测试、clean build、确定性打包和清单验证。

模板位于 `templates/plugin-template`。完整门禁见 `docs/development.md`。
