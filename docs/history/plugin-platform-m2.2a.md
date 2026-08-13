# 插件平台 M2.2A：受支持运行时基线

- 状态：已完成
- 日期：2026-08-10

## 交付范围

本里程碑只升级宿主运行时与构建基线，不改变插件 SDK、manifest、数据格式或业务行为。

- Electron 43.3.0、electron-vite 5.0.0、Vite 6.4.3、electron-builder 26.15.7 均采用精确版本。
- 主窗口启用 sandboxed renderer，preload 由 electron-vite 完整打包。
- 主窗口显式启用 `webSecurity`、禁用 insecure content；popup 和顶层导航默认拒绝，
  只有无凭据的 HTTP(S) URL 可交给系统浏览器。
- default session 的 permission check/request 使用同一 fail-closed 策略，插件能力不
  通过 Chromium permission 直接授予。
- better-sqlite3 为 Electron ABI 148 重建；新增 Electron 原生模块 WAL 往返探针。
- clean package 不依赖 npm 安装时自动产生的 Electron `dist` 目录。
- packaged smoke 增加 preload bridge、Node global 不可见的安全状态断言。

## 兼容性

- 现有数据库 schema、`user_version`、插件配置和插件目录未迁移。
- 六个插件仍使用 v1 renderer/backend 合约；M2.2B 前继续保持兼容。
- 当前同 document `new Function` loader 的风险没有被 sandboxed BrowserWindow 消除，不能把 M2.2A 描述为插件 renderer 已隔离。

## 验收结果

- 代码质量：Prettier、ESLint、Node/renderer/test/plugin/script 类型检查通过。
- 测试：宿主 103、GIF Editor 35、UniEnv 116，合计 254 项通过。
- 原生模块：Electron 43.3.0 / Node 24.18.1 / modules 148 下，better-sqlite3 WAL 写入、查询和关闭通过。
- 构建：六插件 clean build 和 electron-vite 生产构建通过。
- 打包：Windows x64 unpacked 包通过；`better_sqlite3.node` 位于 unpacked native 依赖目录。
- 冒烟：隐藏窗口启动完成数据库初始化、preload bridge、Node global 不可见、
  geolocation permission 为 denied 及 renderer 加载检查。

## 下一阶段入口

M2.2B 将新增每会话唯一 origin 的 sandboxed iframe、版本化 MessagePort RPC、严格 CSP 和自包含 browser renderer bundle。所有六个生产插件迁移并通过真实 Electron 安全 E2E 后，删除宿主同 document loader 与 `unsafe-eval`。
