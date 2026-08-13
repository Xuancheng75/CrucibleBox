# M2.13 Python 当前稳定版过渡

## 结果

OpenBox 1.5.13 / UniEnv 0.5.5 将 Python 3.14.7 加入受支持目录，并作为 Python 默认项。旧的
3.8.10–3.12.5 继续保留，已有安装目录、配置、任务与协议均不迁移。

官方 Windows x64 制品为 `python-3.14.7-amd64.exe`，SHA-256 为
`9d9eb2709ef81bf5cd30db3c2096bdbc4ea10087c22e62f27d356b36f6ae9649`。下载沿用有界流式校验、镜像回退、
失败清理与固定 argv 静默安装；摘要不一致时不会执行安装器。

## Install Manager 决策

Python 官方推荐 Install Manager 26.3，并计划在 Python 3.16 停止传统安装器。官方文档同时说明：

- 管理器通过 Store/MSIX/WinGet 安装、自动更新并占用全局 `python`/`py`/`pymanager` 别名；
- `pymanager install --target` 可提取隔离 runtime，但前提仍是已安装管理器；
- MSIX 需要登录用户与 AppX 注册，可能和旧 launcher、Store 版本及管理员策略冲突；
- 3.14/3.15 期间传统 Windows 安装器继续由官方提供。

因此本里程碑不在后台替用户部署或卸载 MSIX，也不调用 PowerShell/AppX。3.14.7 采用现有可回滚的固定
EXE 路径作为过渡；Python 3.16 前需新增显式的管理器存在性检测、冲突说明与用户驱动 bootstrap，不能把
系统级注册隐藏在普通 runtime 安装任务中。

## 验收

- 目录、摘要、生命周期、首选项和内置组合由现有 11 文件/131 项 UniEnv 测试覆盖。
- 可信服务摘要为 `96a601fab8ea8f392397e140cf81ec098ec2f0c6d3bb8384842bd4fb38d9dc46`。
- 确定性 ZIP `artifacts/plugins/unienv-0.5.5.zip` 的 SHA-256 为
  `8a9dd83de1364a25729b42de5bbf17a4582e04383a9b826382ac1caf2b8955f1`。
- Electron 43.3.0 / Node 24.18.1 / ABI 148 native smoke 通过；Windows unpacked 应用使用临时
  userData 在 1,390 ms 内初始化，working set 为 449,796 KiB。
- 标准 `package:dir` 的 Electron 下载遇到 GitHub 超时；最终包使用本机已通过 ABI probe 的同版本
  `node_modules/electron/dist` 一次性覆盖生成，正式配置未重新加入 `electronDist`。
- 自动化未运行真实 Python 安装器，也未修改用户 PATH、AppX 包或安装根目录。
