# M2.8 三平台 unpacked 打包与启动门禁（历史记录）

> 当前支持范围已由 ADR-0019 收敛为 Windows 10/11 x64；本文件仅保留此前跨平台实验的历史证据。

## 目标

此前 Windows、Ubuntu、macOS 都执行质量检查和生产构建，但只有 Windows 生成并启动 Electron 包。
M2.8 不改变运行架构，只让同一 packaged smoke 在三平台验证真实 Electron、原生 SQLite、跨源插件
renderer、utility backend、UniEnv 可信服务和旧数据库迁移。

## 实现

- `packagedExecutableCandidates` 按当前平台和架构解析 electron-builder 的 `win-unpacked`、
  `linux-unpacked`、`mac*/*.app` 布局；错误会列出全部尝试路径。
- packaged smoke 删除 Windows 限制，其临时目录、schema v1 数据、fixture 插件和验收标记保持一致。
- CI 的三个矩阵系统都运行 `package:dir`；macOS 禁用证书自动发现，Linux 使用 `xvfb-run` 提供虚拟显示。
- 三个纯 Node 测试覆盖 Windows/Linux/macOS 候选顺序、架构差异、fallback 和未知平台失败。

## 验证状态

- 本机 Windows x64：完整 `npm run check` 通过；1.5.8 最终 packaged smoke 为 1,003 ms、
  441,824 KiB working set。
- CI YAML 已解析；脚本与供应链测试为 6 项，其中 3 项覆盖跨平台路径策略。
- 当前 Windows 环境没有 WSL/macOS，不能诚实声称另外两套 GUI 已在本机运行。首次将分支推送到远端后，
  Ubuntu/macOS matrix 是本里程碑的外部验收；失败时应只修对应平台的打包/启动问题，不改业务范围。

## 发布边界

unpacked 冒烟不是安装器签名验收。Windows Authenticode 与 macOS Developer ID/notarization 仍需仓库外
证书；Linux 包格式、桌面集成和各平台真实用户升级仍需独立发布测试。

CI 环境依据 [GitHub runner-images](https://github.com/actions/runner-images)：`ubuntu-latest` 的软件清单
包含 Xvfb；macOS latest 标签可能切换 OS/架构，因此路径解析同时覆盖默认、arm64、x64 和 universal 目录。
