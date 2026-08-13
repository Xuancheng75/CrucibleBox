# M2.11 UniEnv 版本生命周期提示

## 范围与结果

本里程碑只处理 UniEnv 0.5.3 固定版本目录的维护状态与默认选择，不增加或删除工具版本，不改变安装目录、
配置、backend 协议、制品摘要或已安装数据。OpenBox 版本为 1.5.11。

此前五个版本列表均按升序返回，renderer 默认选中第一个条目，实际会把 Python 3.8、Node.js 16、
Git 2.43、Go 1.21 和 Temurin 17.0.11 作为默认。现在每个固定版本都具有完整类型覆盖的生命周期记录，
目录内风险最低的兼容项置顶并默认选中；这只表示“当前目录首选”，不冒充上游最新版本。

## 官方依据

状态固定于 2026-08-10，来源仅使用上游官方页面：

- Python Developer's Guide：3.10–3.12 处于 security 阶段，3.8/3.9 已 EOL。
- Node.js Release Working Group：16、18、20 已 EOL，22 为 Maintenance LTS。
- Go Release Policy：每个大版本只维护到两个更新大版本发布；当前 1.21–1.23 均已超出窗口。
- Adoptium Support Roadmap：17/21 为持续构建的 LTS 线，22 已 EOSL。
- Git for Windows 官方下载页：项目提供当前维护版本但没有 LTS 分支，因此旧固定版本标为 legacy，而非
  伪造具体 EOL 日期。

所有目录项同时提示“固定制品可能不是该分支最新补丁”。SHA-256 只证明下载字节与固定制品一致，不能证明
旧版本仍获得安全更新。

## UI 与失败语义

- 版本下拉项显示版本、维护状态和“目录首选”；首选项固定为 Python 3.12.5、Node.js 22.5.1、
  Git 2.46.0、Go 1.23.0、Temurin 21.0.5。
- 选择项下方显示状态说明和依据日期，EOL 使用错误色，其余旧补丁使用警告色。
- 单项安装在创建后台任务前显示版本专属说明并要求“仍要安装”。取消后不发送安装请求。
- 组合安装列出每一个成员的维护状态，避免一个 EOL 成员被组合名称掩盖。
- backend 仍严格接受原版本白名单；生命周期缺项或目录缺少首选项时 renderer 关闭失败，不回退到最老项。

## 验收

- 纯函数测试遍历全部 21 个固定版本，验证状态、说明、官方 HTTPS 来源和每个工具唯一首选项。
- 测试覆盖首选排序、下拉标签、组合风险摘要、未知版本与缺失首选项关闭失败。
- UniEnv 共 11 个测试文件、128 项；所有安装测试继续使用 fake fetch/spawn 与临时目录。
- 确定性制品 `artifacts/plugins/unienv-0.5.3.zip` 的 SHA-256 为
  `6e75e24f678e5d5b91aa630de09ae4d40977089f7ca37cbb810f8a29e155358c`。
- 全量门禁为宿主 25 文件/163 项、GIF Editor 5 文件/42 项、UniEnv 11 文件/128 项与供应链 6 项；
  Windows packaged smoke 在临时 userData 上耗时 1,377 ms、working set 449,544 KiB。

## 后续边界

本里程碑避免误导和错误默认，但没有把遗留目录变成现代发行目录。下一步应独立评估 Python Install
Manager、Node.js 24 LTS、Go 1.25/1.26、当前 Git for Windows 和 Temurin 25 LTS 的安装语义、摘要与兼容
迁移；在真实 Windows VM 覆盖安装/取消/切换/回滚后，再决定旧版本的默认隐藏或下线期限。
