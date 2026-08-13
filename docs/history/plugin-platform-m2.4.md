# M2.4 验收：可观测性、性能与供应链

## 开发门禁

```powershell
npm ci
npm run check
npm run build
npm run audit:dependencies
npm run package:plugins
npm run verify:plugins
npm run generate:sbom
```

`build` 会执行 `scripts/performance-budgets.json`。当前上限为宿主 main 400,000 B、renderer
3,400,000 B、frame runtime 280,000 B；插件 renderer 分别使用独立阈值。调整预算必须随基准证据
审查，不能为了让回归通过而静默放宽。

## 正式插件签名

签名密钥必须为 PEM 编码 Ed25519，且存放在仓库外：

```powershell
$env:OPENBOX_PLUGIN_SIGNING_KEY_FILE = 'D:\release-keys\openbox-plugins-private.pem'
$env:OPENBOX_PLUGIN_SIGNING_KEY_ID = 'openbox-plugins-2026'
$env:OPENBOX_PLUGIN_VERIFY_KEY_FILE = 'D:\release-keys\openbox-plugins-public.pem'
$env:OPENBOX_PLUGIN_VERIFY_KEY_ID = 'openbox-plugins-2026'
npm run release
```

`release` 顺序为构建、确定性插件打包、签名、强制验签、SBOM、安装器打包。缺少密钥、key ID
不匹配、清单/ZIP/运行文件任一字节变化都会失败。开发用 `package:plugins + verify:plugins` 不要求
私钥，并会删除陈旧签名，避免把旧签名误配给新制品。

## 运行诊断

- 控制台 `[metrics]` 行为 `openbox.startup` schema v1，包含 `app.ready`、数据库、PluginManager、
  `window.created`、`renderer.ready` 与 `plugins.restored` 里程碑。
- `userData/logs/main.jsonl` 记录 session、启动报告、renderer gone、未处理 rejection 和 fatal exception；
  当前文件达到 2 MiB 时轮转为 `main.jsonl.1`。
- `userData/logs/session.json` 只在进程运行期间存在。下次启动看到残留 marker 会写
  `previous-session-unclean`；正常关闭会删除。

## 本里程碑结果

- 根工程及六插件 npm audit：0 漏洞。
- 宿主 22 个 Vitest 文件、153 项；GIF Editor 35 项；UniEnv 116 项；供应链 Node 测试 3 项。
- 六插件版本：Diary 0.4.8、Dice 0.1.4、GIF Editor 0.3.5、ThemeManager 0.1.9、
  Turntable 0.1.7、UniEnv 0.5.1。
- Windows 打包冒烟同时校验数据库、sandboxed preload、跨源 renderer、两个 utility backend、
  UniEnv 固定摘要服务及启动/内存预算。所有测试均使用临时 userData。
