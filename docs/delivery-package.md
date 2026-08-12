# CrucibleBox 1.5.23 交付说明

## 交付内容

最终交付包由以下内容组成：

```text
CrucibleBox-1.5.23-windows-x64-delivery/
├─ README-交付说明.md
├─ BINARIES/
│  ├─ CrucibleBox-1.5.23-windows-x64-setup.exe
│  └─ CrucibleBox-1.5.23-windows-x64-setup.exe.blockmap
├─ PLUGINS/
│  ├─ diary-0.4.11.zip
│  ├─ dice-roller-0.1.6.zip
│  ├─ gif-editor-0.3.8.zip
│  ├─ theme-manager-0.1.12.zip
│  ├─ turntable-0.1.10.zip
│  ├─ unienv-0.5.7.zip
│  └─ manifest.json
├─ SOURCE/
│  ├─ CrucibleBox-1.5.23-source.zip
│  └─ COMMIT.txt
├─ DOCUMENTATION/
│  ├─ github-auto-update-handoff.md
│  ├─ trusted-release.md
│  ├─ architecture.md
│  ├─ development.md
│  └─ refactor-summary.md
└─ SHA256SUMS.txt
```

`SOURCE/CrucibleBox-1.5.23-source.zip` 是基于 commit `84c1deb9a279ab4f0740bed56bb917a1e987703a` 的 1.5.23
post-commit working-tree release snapshot，在 `84c1deb9` 之后还包含后续已验证修复。它包含宿主、六个插件、插件
模板、共享 SDK、测试、脚本、GitHub Actions 工作流、构建配置和文档。不包含 `.git` 历史、`node_modules`、
构建缓存、本机用户数据和任何私钥。`SOURCE/COMMIT.txt` 仅记录该快照的 base source reference（基线 commit
ID），并不表示该目录下存在完整 Git commit。

## 直接使用

Windows 10/11 x64 用户运行：

```text
BINARIES/CrucibleBox-1.5.23-windows-x64-setup.exe
```

安装包没有 Authenticode 证书，因此 Windows 可能显示“未知发布者”或 SmartScreen 提示。当前离线成品不包含
`app-update.yml`，只会禁用在线更新；插件、主题、数据库以及全部工具功能仍可正常使用。

首次使用时，可在插件管理界面逐个导入 `PLUGINS/` 中的六个 ZIP。它们是与源码快照对应、已经完成确定性摘要验证
的当前生产版本。离线侧载包没有正式 Release 的 Ed25519 签名，宿主会显示 Full Trust 安装确认，这是预期行为。

## 校验交付文件

在交付包根目录执行：

```powershell
Get-Content .\SHA256SUMS.txt
Get-FileHash .\BINARIES\CrucibleBox-1.5.23-windows-x64-setup.exe -Algorithm SHA256
Get-FileHash .\SOURCE\CrucibleBox-1.5.23-source.zip -Algorithm SHA256
```

计算结果必须与 `SHA256SUMS.txt` 对应条目完全一致。

## 从源码构建

要求 Windows 10/11 x64、Node.js 24.15.x 和 npm 11.12.x：

```powershell
Expand-Archive .\SOURCE\CrucibleBox-1.5.23-source.zip .\CrucibleBox-source
Set-Location .\CrucibleBox-source
npm ci
npm run check
npm run build
npm run package
npm run smoke:installer
```

普通 `npm run package` 不需要 GitHub 仓库、GitHub Token、插件发布密钥或 Windows 证书。

## 后续启用自动更新

自动更新配置当前有意留空。仓库所有者应先阅读：

```text
DOCUMENTATION/github-auto-update-handoff.md
```

该文档包含公开 GitHub 仓库、Actions 权限、免费 Ed25519 插件签名、stable/beta tag、Release 制品以及首次真实更新
演练的完整步骤。配置完成后使用 Release 工作流生成带 `app-update.yml` 的安装包；无需改动应用业务代码。

## 已完成验证

- 宿主 35 个测试文件、254 项测试通过；六个插件 190 项测试通过。
- 供应链测试 16 项全部通过（当前 0 跳过）。
- 数据库 schema v3（`plugins.sort_order`）迁移通过；插件列表支持长按拖动排序与键盘上移/下移，
  顺序在事务内持久化，插件激活顺序跟随列表。
- 生产构建和性能预算通过。
- Electron 43.3.0、Node 24.18.1、ABI 148 原生模块冒烟通过。
- 旧数据事务迁移通过。
- 工具箱本体与 UniEnv 的 Windows VM 安装/取消/切换/回滚验收通过。
- NSIS 安装包已完成真实静默安装、正式启动和静默卸载。
- 已确认离线安装包不包含 `resources/app-update.yml`。
