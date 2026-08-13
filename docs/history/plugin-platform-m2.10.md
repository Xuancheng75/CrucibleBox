# M2.10 UniEnv 上游制品完整性

## 范围与结果

本里程碑只收紧 UniEnv 0.5.2 的下载信任边界，不改变宿主/插件 RPC、配置格式、安装目录布局或已安装
数据。OpenBox 版本为 1.5.10。

全部受支持的 Python、Node.js、Git for Windows、Go 与 Eclipse Temurin Windows x64 版本现在均绑定
唯一文件名、官方 HTTPS URL 和 SHA-256。目录由 TypeScript 完整映射约束，新增支持版本如果没有摘要会
在编译或运行时关闭失败，不能退回“下载后直接执行”。

## 摘要来源

- Node.js：各版本官方 `SHASUMS256.txt`，其签名链由 Node.js 发布流程维护。
- Go：官方 `go.dev/dl/?mode=json&include=all` 的文件摘要。
- Eclipse Temurin：Adoptium API 的 package checksum 与发布标签。
- Git for Windows：官方 GitHub release 描述中的 SHA-256 表。
- Python：3.10 及以上版本的官方 Sigstore bundle；3.8/3.9 旧制品从 python.org 官方二进制一次性取样并固定摘要。

固定目录同时修复了两个不可达 JDK URL：Temurin 17.0.12 的正确 build 为 7，21.0.5 的正确 build 为
11。旧实现分别使用 12 和 5，会指向不存在的发布制品。

## 下载与失败语义

下载器只接受无凭据 HTTPS URL 和小写 64 位十六进制 SHA-256。响应仍受 Content-Length、实际流量、
空闲超时和取消信号约束；每个 chunk 在写入 `.part` 的同时更新 SHA-256。

只有摘要匹配后才执行文件同步、关闭与同目录原子 rename。摘要无效、响应不匹配、取消或网络失败都会
关闭句柄并删除 `.part`。镜像不构成新的信任根：镜像摘要不匹配时可回退到官方 URL，官方源仍不匹配则
整个安装失败，错误包含预期值和实际值，不会进入解压或执行阶段。

## 兼容性

- `downloadMirror` 配置值和既有组合安装协议不变。
- 下载缓存文件名改由完整性目录单源生成，与官方文件名一致。
- 已安装版本和用户目录不迁移、不删除。
- JDK 17.0.12/21.0.5 的 URL 修复只影响后续下载；现有正确安装保持可用。

## 验收

- 目录测试逐一遍历所有受支持工具/版本，验证摘要格式、HTTPS URL、无凭据和文件名一致性。
- 下载测试覆盖无效预期摘要、匹配成功、摘要不匹配清理、镜像污染后官方回退、双源失败、取消、超限和
  body 空闲超时。
- 工具版本测试确认五个适配器只暴露完整性目录覆盖的固定版本。
- 确定性制品 `artifacts/plugins/unienv-0.5.2.zip` 的 SHA-256 为
  `ccddbc52f6caa042562ab4e93407c23d5602e438a806ef8ff966d383105ab10d`。
- 全量门禁为宿主 25 文件/163 项、GIF Editor 5 文件/42 项、UniEnv 10 文件/123 项与供应链 6 项；
  Windows packaged smoke 在临时 userData 上耗时 1,062 ms、working set 447,968 KiB。
- 未运行任何真实工具安装器或修改真实用户目录；真实 Windows VM 安装/取消/切换/回滚仍作为外部 E2E。

## 后续边界

固定摘要解决当前目录的传输与镜像完整性，不代表旧工具版本仍受上游安全维护。Python 3.8/3.9、
Node.js 16 等 EOL 版本是否保留，需要兼顾旧项目兼容性的单独产品策略与迁移提示，不能在本里程碑中静默
移除。
