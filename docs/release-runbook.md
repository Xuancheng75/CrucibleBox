# CrucibleBox 发布 Runbook

> 当前规范（汇总 trusted-release.md / delivery-package.md / github-auto-update-handoff.md；原文保留）。
> 操作目标：本地两条命令 + CI 一条 tag，即可完成可验证发布。单一发布清单 `release-manifest.json` 构建时生成。

## 1. 发布边界

- 仅支持 Windows 10/11 x64；交付物为 NSIS 安装器。
- 安装器**有意不签名**（无 Authenticode 证书）：Windows 显示 Unknown publisher / SmartScreen，属预期，文档不得描述为可信。
- 自动更新**默认关闭**：无 `app-update.yml` 时离线包为默认交付，不访问网络。
- GitHub Release 为可选能力，配置公开仓库后启用。

## 2. 两条本地命令（1.5.24 落地）

```bash
npm run release:local     # check → build → package:plugins → sign:plugins → SBOM → package:release → manifest → collect
npm run release:validate  # 全部 smoke + 更新元数据校验 + manifest（--require-signature）+ checksum
```

- `release:local` 需要签名密钥环境：`OPENBOX_PLUGIN_SIGNING_KEY_FILE` / `KEY_ID`（仓库外生成，私钥永不入库）。
- 无密钥时仅本地验证路径可用：`npm run check && npm run build && npm run package:dir && npm run smoke:packaged`。
- **本地仅发布 stable**：`electron-builder.release.yml` 固定 `channel: latest`，本地 `release:local` 只产出 `latest.yml`；beta 通道走 CI（release.yml 传 `-c.publish.channel=beta`）。`release:validate` 相应固定校验 `latest.yml`。
- `release:validate` 会重新生成 `release-manifest.json` 与 `SHA256SUMS`（追加校验产物，不是"不改产物"）；本地与 CI 各自生成后做 canonical 对比。

## 3. 发布检查单（每版固定）

1. 版本提升：根 `package.json` + 受影响插件 `package.json`/`plugin.json` 成对提升。
2. `npm run check`（format/lint/typecheck 5 层/test：宿主 254、插件 190、供应链 16）。
3. `npm run build`（frame → plugins(+renderer/trusted verify) → app → performance）。
4. `npm run release:local`。
5. `npm run release:validate`（smoke:packaged + smoke:installer + manifest + checksum）。
6. `git tag -a vX.Y.Z` → `git push origin main && git push origin vX.Y.Z`。

## 4. GitHub 发布工作流（tag 触发，release.yml）

- 触发：push `v*` tag 或 workflow_dispatch；校验 `tag == "v"+package.json.version`。
- 权限：`contents: write` + `id-token: write` + `attestations: write`；用内置 GITHUB_TOKEN，客户端不嵌入 Token。
- 步骤：checkout(tag) → `npm ci` → 全门禁 → 打包签名插件（临时密钥文件，finally 删除）→ 打包 unsigned 安装器 → `verify:fuses`/`smoke:native`/`smoke:packaged`/`smoke:installer`/Windows 更新制品校验 → 收集产物（安装器/blockmap/latest.yml|beta.yml/插件 ZIP+SIG/SBOM/SHA256SUMS）→ `actions/attest@v4` → `gh release create` + upload。
- 发布前建议跑 `release-compatibility.yml`（previous→candidate→previous 同 userData，断言 schema 与 sentinel）。

## 5. stable / beta 通道

- stable = 纯 SemVer（如 `1.5.23`）+ tag `v1.5.23` + `latest.yml`。
- beta = `1.5.24-beta.1` + 对应 tag + `beta.yml`。
- 其他 prerelease channel 被拒绝；tag 必须等于 `v`+根包版本。
- 客户端行为：自动检查但不自动下载/安装；只有用户从 beta 切回 stable 允许一次降级检查，同通道降级禁止。
- 失败版本应撤回并用更高 patch 替换；保留前一安装包至通过升级+回滚演练。

## 6. 产物清单

- `CrucibleBox-<ver>-windows-x64-setup.exe` + `.blockmap`
- `latest.yml` / `beta.yml`（含 installer SHA-512）
- `SHA256SUMS`
- 7 份 CycloneDX SBOM（`cruciblebox` + 6 插件 `.cdx.json`）
- 6 个插件 ZIP + `manifest.json` + `manifest.sig.json`（Ed25519）
- `release-manifest.json`（应用版本、插件版本、ZIP 清单、SHA-256、签名 key ID、SBOM、安装器摘要、attestation subject）
- GitHub artifact attestation（`actions/attest@v4`）

## 7. 消费者验证三连

```powershell
Get-AuthenticodeSignature <setup.exe>   # 预期 NotSigned
Get-FileHash <setup.exe> -Algorithm SHA256   # 比对 SHA256SUMS
gh attestation verify <setup.exe> -R Xuancheng75/CrucibleBox
```

## 8. 交付包（可选，人工发布）

`delivery/`：README-交付说明 + `BINARIES/`（exe/blockmap）+ `PLUGINS/`（ZIP+manifest）+ `SOURCE/`（source.zip + COMMIT.txt）+ `DOCUMENTATION/` + `SHA256SUMS.txt`。

## 9. 自动更新启用（仅当需要在线更新）

1. 公开 GitHub 仓库（无内置 Token）；推送默认分支并保留 `.github/workflows/`。
2. Actions 启用 `contents/id-token/attestations: write`。
3. 配置 Secret `OPENBOX_PLUGIN_SIGNING_KEY_BASE64` / `VERIFY_KEY_BASE64` + Variable `OPENBOX_PLUGIN_SIGNING_KEY_ID`。
4. 用 `electron-builder.release.yml` + Release 工作流生成带 `app-update.yml` 的包；无需改业务代码。
5. 首次在线闭环验收：基线发布 → 造数 → patch 升版 → 设置页检查 → 下载 → 重启安装 → 验证 → Compatibility 工作流验证回滚。
