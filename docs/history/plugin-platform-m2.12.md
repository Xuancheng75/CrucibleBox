# M2.12 UniEnv 当前维护制品目录

## 范围与结果

本里程碑将 UniEnv 从“只提示固定旧版本风险”推进为同时提供当前维护制品的兼容目录。OpenBox 版本为
1.5.12，UniEnv 版本为 0.5.4。安装目录、backend RPC、任务模型、镜像回退、已安装版本和用户配置均不变；
旧版本继续保留，未发生删除或数据迁移。

新增 Windows x64 制品：

| 工具            | 版本    | 制品                                               |
| --------------- | ------- | -------------------------------------------------- |
| Node.js         | 24.18.1 | `node-v24.18.1-win-x64.zip`                        |
| Git for Windows | 2.54.0  | `Git-2.54.0-64-bit.exe`                            |
| Go              | 1.26.5  | `go1.26.5.windows-amd64.zip`                       |
| Temurin         | 17.0.20 | `OpenJDK17U-jdk_x64_windows_hotspot_17.0.20_8.zip` |
| Temurin         | 21.0.12 | `OpenJDK21U-jdk_x64_windows_hotspot_21.0.12_8.zip` |
| Temurin         | 25.0.4  | `OpenJDK25U-jdk_x64_windows_hotspot_25.0.4_7.zip`  |

每个条目均固定官方 HTTPS URL、文件名、release tag（如适用）和 SHA-256。下载器沿用 M2.10 的流式
校验、`.part` 清理和原子提升；镜像返回不同字节时会回退官方源，不会解压或执行未验证制品。

## 官方依据与选择

状态固定于 2026-08-11，仅使用上游官方来源：Node.js 发布目录与 Release Working Group 日程、Go 下载页与
发布策略、Git for Windows 官方 release/API、Adoptium v3 API 与支持路线图。

- Node.js 24.18.1 是当前 Active LTS 线的最新补丁，作为 Node 目录首选。
- Git 2.54.0 是当前 Git for Windows 正式版，作为 Git 目录首选。
- Go 1.26.5 是当前 Go 1.26 补丁，作为 Go 目录首选。
- Temurin 17、21、25 均加入当前安全补丁；21.0.12 作为目录首选，以兼顾 LTS 与现有构建生态兼容性。
- Python 仍固定到传统 Windows 安装器目录；其现代 Install Manager 具有不同安装/升级语义，留给独立里程碑，
  不在本次伪装成普通 EXE 更新。

## UI、组合与兼容

- 生命周期增加“当前维护版本”状态，使用成功色；当前制品安装不再显示旧版本二次风险确认。
- maintained/EOL/legacy 制品继续显示说明，并在任务创建前确认。
- 内置前端、Go、Java 和通用组合迁到新目录；Python 组合保留 3.12.5 并继续明确提示固定旧补丁。
- 自定义组合和已有安装目录不改写；协议仍对未知工具、未知版本、额外字段和未维护摘要关闭失败。

## 验收

- UniEnv 11 个测试文件、131 项测试，覆盖目录完整性、官方摘要、首选排序、风险确认与内置组合。
- 宿主 25 文件/163 项、GIF Editor 5 文件/42 项、供应链 6 项继续通过。
- 生产 renderer 为 223,082/280,000 B；可信服务最终摘要为
  `782b4f1b93e31501e8c42e9aad1d51e54e0815efabfeacdf52ff0b096646aaec`。
- 确定性制品 `artifacts/plugins/unienv-0.5.4.zip` 的 SHA-256 为
  `8cb2f38d68ce980c3a15647784fd722c1ed9c5184b88a67c8de2b8183e0c3784`。
- Electron 43.3.0 / Node 24.18.1 / ABI 148 native smoke 通过；Windows unpacked 应用使用临时
  userData 在 1,441 ms 内初始化，working set 为 449,568 KiB，schema v1 数据完成逐字节备份与事务迁移。
- 测试未执行真实工具安装器，也未读取或修改用户安装根目录。

真实 Windows VM 仍需在正式发布前覆盖各新制品的安装、取消、切换、回滚和 PATH 行为；该外部门禁不由
fake fetch/spawn 单测替代。
