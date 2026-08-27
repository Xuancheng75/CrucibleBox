# CrucibleBox 更新日志（Tauri 线）

> 覆盖 tauri-v1.9.2（首个 Tauri 正式版）起的用户可感知变更。
> Electron 1.7.x 线已冻结，历史见 `docs/electron-legacy-registry.md`。

## 1.9.15（2026-08）

宿主能力扩展（插件 SDK v2 契约不变，仅扩展实现面）：

- **`network.fetch`**：插件 backend 可发起 HTTP 请求（ureq，30s 超时，响应 ≤50MB）
- **`clipboard.read/write`**：插件 backend 可读写系统剪贴板文本（arboard）
- **`file.read/write`**：插件 backend 可读写文件系统
- **`notification.show`**：插件 backend 可发送系统通知（经宿主事件总线转发）
- **`system.info`**：插件 backend 可获取系统信息（CPU/内存/磁盘/OS/网络，无需权限）
- 新增四个插件：JSON/文本工具箱、剪贴板管理器、系统信息面板、实时汇率
- 插件总数从 6 个扩展到 10 个

## 1.9.14（2026-08）

- 插件安装根路径统一（`%APPDATA%\cruciblebox\plugins\<id>`），兼容旧双根布局自动迁移
- 隔离孤儿目录（无 `plugin.json`）至 `_quarantine/`
- 修复 `plugin_install_commit/discard` 参数键名不匹配

## 1.9.13（2026-08）

修复版本：

- **导入确认报错**：`plugin_install_commit/discard` 参数键名不匹配（Rust 形参
  `token` vs 前端误传 `installToken`），自 1.9.3 起「确认安装」必失败。现已在
  Tauri 线完整走通 导入→预览→确认 安装链。
- **UniEnv 版本下拉为空**：`listVersions` 不再在请求路径上联网；在线发现改为
  「检查语言新版本」按钮显式触发（独立线程 + 8s 硬超时，覆盖 DNS 挂起场景），
  慢网/断网时秒回内置目录。
- **日记返回按钮不居中**：`.nav-btn` 改 flex 居中、`.back-btn` 恢复对称内边距。
- **默认主题顶栏重叠**：Header 由 transparent 改为不透明背景 + 分隔线
  （cyber/neon 主题外观不受影响）。

发布资产新增官方插件包 `unienv-0.6.0.zip` / `diary-0.4.13.zip`：
**从 1.9.12 及更早升级的用户请导入这两个包**以启用 UniEnv 在线新版本与日记
周切换等新功能（NSIS 安装器不会更新 %APPDATA% 内的已装插件）。

## 1.9.12（2026-08）

- 全窗口拖拽导入（PCL2 风格）：.zip/插件目录拖入窗口任意位置即可导入；
  多文件进入批量队列逐个预览确认；导入弹窗移除装饰性 Dragger 并重排布局。
- 批量删除：主页「批量管理」勾选模式 + 名单确认弹窗 + 顺序卸载。
- 主页网格固定一排四个；卡片简介悬浮显示完整 Tooltip。
- UniEnv 动态版本源：node/go/java 从官方端点发现新版本并可直接安装
  （上游权威 SHA-256 校验）；新增 Rust / PHP 工具；配置项「联网检查语言版本」。
- 日记周视图跨周切换（‹ › / 周区间标题 / 本周）。
- 修复：打包态 sidecar 文件名解析、迁移后 installed_path 残留（schema v4）、
  sidecar FrameQueue 并发自旋、Modal 头部配色、沙箱 confirm 静默失败、
  首帧深色内联样式、图标缓存刷新 hook。

## 1.9.11（2026-08）

- 打包态 sidecar 文件名解析修复（externalBin 剥 triple 后的安装名优先）。
- 数据目录迁移后 `plugins.installed_path` 自动修复（schema v4）。
- sidecar FrameQueue 按帧类型分发，消除并发请求下的自旋死循环。
- NSIS 安装器 hook：安装完成广播 SHChangeNotify 刷新图标缓存；
  CI 新增解包验证内嵌图标步骤。
- UniEnv 阶段 B 主体（下载/解压/junction/任务管理）随本线落地。

## 1.9.10 及更早

- 1.9.10：NSIS 安装器/卸载器图标指向 neon 新图标。
- 1.9.9：UniEnv 阶段 A（trusted.invoke 只读面）、Modal headerBg、首帧深色兜底、
  CI 图标缓存清理。
- 1.9.8：回退 cyber 主题至 1.9.5 样式。
- 1.9.7：滚动更新清单（tauri-latest/latest.json）、应用图标集替换。
- 1.9.6：插件 iframe 加载修复（cruciblebox-plugin.localhost）、导入对话框优化。
- 1.9.5–1.9.2：sidecar 宿主集成、崩溃恢复、发布链成型（详见 ADR 与 git 历史）。
