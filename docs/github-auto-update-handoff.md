# GitHub 自动更新接入交接

## 当前状态

GitHub 自动更新是可选能力，不是应用运行前置条件。

- 未配置 GitHub 仓库时，执行 `npm run build` 和 `npm run package` 可以生成可安装、可卸载、可独立运行的 Windows x64 NSIS 安装包。
- 普通安装包不包含 `resources/app-update.yml`。应用会把在线更新显示为“未配置”，不会定时访问网络，也不会影响插件、主题、数据库或其他离线功能。
- 配置公开 GitHub 仓库后，使用 `electron-builder.release.yml` 和 Release 工作流生成带 `app-update.yml` 的安装包，自动启用 stable/beta 更新能力。
- Windows 安装包目前不使用 Authenticode 证书，首次安装或更新时可能显示“未知发布者”或 SmartScreen 提示。

## 仓库所有者需要完成的配置

1. 使用公开 GitHub 仓库。桌面客户端不会内置 GitHub Token，因此私有 Release 不适合作为普通用户的更新源。
2. 将本项目推送到仓库默认分支，并保留 `.github/workflows/` 中的工作流。
3. 在 `Settings -> Actions -> General` 中启用 Actions，并允许工作流取得写权限。Release 工作流需要：
   - `contents: write`
   - `id-token: write`
   - `attestations: write`
4. 在 `Settings -> Secrets and variables -> Actions` 中配置：

| 类型     | 名称                                | 用途                                   |
| -------- | ----------------------------------- | -------------------------------------- |
| Secret   | `OPENBOX_PLUGIN_SIGNING_KEY_BASE64` | Ed25519 插件签名私钥 PEM 的 Base64     |
| Secret   | `OPENBOX_PLUGIN_VERIFY_KEY_BASE64`  | Ed25519 插件验签公钥 PEM 的 Base64     |
| Variable | `OPENBOX_PLUGIN_SIGNING_KEY_ID`     | 例如 `cruciblebox-first-party-2026-01` |

密钥只用于第一方插件制品签名，与 Windows 代码签名证书无关。生成方式：

```powershell
openssl genpkey -algorithm Ed25519 -out openbox-plugin-signing-key.pem
openssl pkey -in openbox-plugin-signing-key.pem -pubout -out openbox-plugin-verify-key.pem
[Convert]::ToBase64String([IO.File]::ReadAllBytes('.\openbox-plugin-signing-key.pem'))
[Convert]::ToBase64String([IO.File]::ReadAllBytes('.\openbox-plugin-verify-key.pem'))
```

私钥及其 Base64 不得提交到 Git、Issue 或 Release；仓库所有者应保存离线备份。

## 发布正式版本

根目录 `package.json` 的版本必须和 tag 完全一致：

```powershell
npm ci
npm run check
npm run build
git tag -a v1.5.23 -m "CrucibleBox 1.5.23"
git push origin v1.5.23
```

Release 工作流会在 Windows runner 上重新构建并验证：

- 宿主和六个插件的类型、测试与生产构建；
- 第一方插件签名和确定性制品；
- NSIS 安装、真实启动和静默卸载；
- Electron ABI、fuses、SBOM 和构建证明；
- `latest.yml`、blockmap 和安装包 SHA-512 一致性。

稳定版使用普通 SemVer 和 `latest.yml`。Beta 版仅接受 `1.5.24-beta.1` 形式，并生成 `beta.yml`。

## 首次在线闭环验收

1. 发布第一个公开基线版本并在干净的 Windows 11 环境安装。
2. 在 Diary、Turntable 和 ThemeManager 中创建测试数据。
3. 将版本号提升一个 patch，发布第二个版本。
4. 在旧版本的设置页执行“检查更新 -> 下载 -> 重启并安装”。
5. 确认新版本号、插件激活状态和测试数据全部保留。
6. 使用 Release Compatibility 工作流验证旧版 -> 新版 -> 旧版的数据兼容过程。

## 无 GitHub 配置的本地交付

```powershell
npm ci
npm run check
npm run build
npm run package
npm run smoke:installer
```

这条路径不读取 GitHub 仓库、GitHub Token、插件发布密钥或 Windows 证书。它是当前默认的本地成品交付路径。
