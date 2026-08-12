# 插件平台 M2.3：backend SDK v2 与 UniEnv 可信服务

## 运行模型

```text
plugin dist/main.js
  -> Electron utility process (minimal env, no in-process fallback)
  -> backend RPC v2 (token + requestId + exact method contract)
  -> PluginManager permission adapters
  -> database / notification / dialog / network / file / shortcut / event

UniEnv dist/main.js (pinned proxy)
  -> trusted.invoke(unienv, operation)
  -> TrustedServiceRuntime (host bundle)
  -> validated UniEnv task protocol / path policy / argv-only process runner
```

## backend RPC v2

信封固定为版本 `2`，请求为 `{v,kind,token,requestId,method,params}`，响应为成功
`{...,ok:true,result}` 或失败 `{...,ok:false,error}`。支持的方法显式列在
`shared/types/plugin-backend-rpc.types.ts`，未知方法、额外字段、非 JSON 值、循环对象、超深、超大或
超过 64 个并发请求均被拒绝。

文件内容跨边界时使用 base64；网络请求只接受 method、字符串 headers 和字符串 body；
`AbortSignal`、函数和任意对象不会跨边界。事件订阅在 worker 内按事件合并，最后一个监听器释放时
才撤销宿主订阅。

## UniEnv 拆分

- 插件包不再包含 `process-runner`、下载器、解压器和五个工具适配器。
- `src/main.ts` 仅负责连接、转发和停用宿主服务；原实现保留在源码树的
  `src/trusted-service.ts`，只由宿主 main bundle 编译和加载。
- `shared/trusted-service-policies.json` 固定版本、三文件集合和摘要；
  `scripts/verify-trusted-services.mjs` 在每次生产构建验证源码产物与策略一致。
- 安装后的目录还会再次校验普通文件、无 symlink、文件集合、总大小和摘要，不能仅靠伪造
  `trusted:unienv` 权限取得能力。
- UniEnv 0.5.0 继续使用原有严格请求/config/path 解析、HTTPS 有界下载、argv-only 进程启动、
  staging 提升、可取消任务和 116 项测试，因此旧配置与任务 UI 无需迁移。

## 插件迁移

新插件 manifest 同时声明：

```json
{
  "backendApiVersion": 2,
  "rendererApiVersion": 2
}
```

普通插件继续实现 `activate/deactivate/onMessage`，不需要感知 utility process 传输。renderer 必须是
M2.2B 的自包含 browser bundle。模板已切到 esbuild：main 输出 CJS，renderer 输出 browser IIFE。
`invokeTrustedService` 不属于通用公开 SDK；只有固定 UniEnv 代理的本地声明包含它。

## 验收

- host：20 个测试文件、147 项；GIF Editor：35 项；UniEnv：116 项，总计 298 项。
- 六插件 typecheck、clean build、自包含 renderer 校验和可信服务摘要校验通过。
- Windows unpacked package 启动后，真实 Dice backend 通过 utility process RPC v2 响应 ping；真实
  UniEnv 代理通过固定摘要校验并由宿主服务返回五个工具；renderer 跨源隔离冒烟同时通过。
- 测试仅使用临时 userData 和临时插件副本，不改写真实用户数据库或插件目录。

## 已知边界

普通 backend 仍可直接调用 Node API，因此只能运行用户明确安装并信任的代码。SDK 权限检查、最小
环境和 RPC 校验是纵深控制，不是恶意代码边界。跨平台 utility process 行为与 UniEnv 的 Windows-only
业务限制需要在后续 CI 矩阵持续验证。
