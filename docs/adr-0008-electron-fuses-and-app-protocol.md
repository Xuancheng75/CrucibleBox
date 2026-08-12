# ADR-0008：Electron fuses 与宿主自定义协议

- 状态：已接受
- 日期：2026-08-11
- 里程碑：M2A 收口

## 背景

Electron 43 的 fuse wire 已扩展为 9 位，electron-builder 26 间接依赖的旧版
`@electron/fuses` 只识别 8 位。若继续依赖 builder 内置配置，新增 fuse 会保持继承状态且无法用
strict 模式验证。宿主生产页面原先由 `file://` 加载，也迫使应用保留 file protocol 的额外特权。

## 决策

- 直接锁定 `@electron/fuses` 2.1.3，通过 `afterPack` hook 配置并回读全部 9 个 fuse；发现未来新增
  fuse 时打包立即失败，必须经过显式审查。
- 禁用 `RunAsNode`、`NODE_OPTIONS`、Node CLI inspector 和 file protocol extra privileges。
- 启用 cookie encryption、embedded ASAR integrity、only-load-app-from-ASAR 与 WASM trap handlers。
- 显式禁用 browser-specific V8 snapshot。Electron 43 Windows 分发包不带
  `browser_v8_context_snapshot.bin`，启用后会在主进程启动前崩溃；普通 snapshot 保持可用。
- 生产宿主页改由固定 origin `openbox-app://app/index.html` 加载。协议只接受 GET、固定 host、已知
  静态资源类型和 renderer 根目录内路径，并拒绝凭据、端口、查询、畸形编码与路径穿越。
- IPC sender、顶层导航和 legacy `plugin://` CORS 同步只信任该固定生产 origin；开发模式仍只信任
  配置的 Vite origin。

## 验证

- fuse 纯函数测试覆盖完整策略、不安全状态、缺失位与未来 schema。
- custom protocol 测试覆盖合法资源、origin lookalike、凭据、端口、查询、编码与穿越负例。
- `package:dir` 在真实 Electron 43 二进制上应用并独立回读 fuse。
- Windows packaged smoke 在临时 userData 下验证 ASAR、preload bridge、native SQLite、跨源插件
  renderer、两个 utility backend 与 UniEnv 可信服务；本次结果为 2,033 ms、461,000 KiB。

## 后果

生产包不再依赖 `file://` 的 Electron 扩展能力，并封死通过环境变量把应用二进制当 Node 运行时的
路径。开发服务器行为、插件数据和 manifest/SDK 契约均不改变；cookie encryption 是单向策略，
后续版本不得随意关闭。
