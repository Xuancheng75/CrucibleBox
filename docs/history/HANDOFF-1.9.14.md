# 交接文档：CrucibleBox v1.9.14 插件根目录双根修复

> 编写时间：2026-08-26
> 仓库：https://github.com/Xuancheng75/CrucibleBox.git
> 分支：`fix/1.9.13-regression-batch`
> 提交：`1a0120a`
> Tag：`tauri-v1.9.14`（已推送到 origin）
> CI 工作流运行 ID：`33033857164`（Tauri Release v1.8.4+，已完成）

---

## 1. 背景与问题

v1.9.12 及之前版本误用 Tauri identifier 根 `%APPDATA%\com.cruciblebox.app\plugins`，
而正确路径应为迁移根 `%APPDATA%\cruciblebox\plugins`（`data_dir.rs` 中 `NEW_DATA_DIR_NAME`）。
由此造成**双根错乱**：

- 重装已卸载插件时报 `Installed plugin directory was expected to be absent`
  （旧版本卸载只删 DB 行、不删目录 → 残留孤儿目录；新安装时目录已存在被拒绝）
- 卸载后磁盘残留孤立插件目录，后续导入失败
- DB 中 `installed_path` 可能仍指向 identifier 根，与实际安装位置不一致

## 2. 本次 1.9.14 改动清单

| 文件                          | 改动                                                                                                                                                                                                                                           |
| ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src-tauri/src/install.rs`    | 新增 `migrate_legacy_plugin_root`（启动迁移 identifier 根 → 统一根）+ `copy_dir_recursive`（跨盘回退）；`preview_from_root` 在 `stage()` 前隔离孤儿目录到 `.orphans`；`run_startup_recovery` 新增 `legacy_roots` 参数，在 journal 恢复前先迁移 |
| `src-tauri/src/db.rs`         | 移除 `plugin_all_roots` / `plugin_update_installed_path` 的 `#[allow(dead_code)]`（现已使用）                                                                                                                                                  |
| `src-tauri/src/main.rs`       | 计算 identifier 根 `com.cruciblebox.app\plugins` 并传给 `run_startup_recovery`；`plugins_dir` 用迁移根 `data_dir.join("plugins")`                                                                                                              |
| `src-tauri/src/commands.rs`   | 卸载时已删除目录（`.uninstalled-<ts>-<name>` 隔离回退）—— 此前版本已实现，本次未改                                                                                                                                                             |
| `src-tauri/tauri.conf.json`   | version 1.9.13 → 1.9.14                                                                                                                                                                                                                        |
| `src-tauri/Cargo.toml`        | version 1.9.13 → 1.9.14                                                                                                                                                                                                                        |
| `tauri-frontend/package.json` | version 1.9.13 → 1.9.14                                                                                                                                                                                                                        |
| `scripts/plugin-catalog.json` | 移除 `document-engine` 条目（CI 构建插件列表）                                                                                                                                                                                                 |

**关键设计点（接手人需理解）：**

- 孤儿目录隔离必须发生在 `preview_from_root` 的 `stage()` **之前**。
  `DirectoryTransaction` 在 `stage()` 时快照目标目录是否存在；若在 `commit_fresh` 才隔离，
  swap 阶段会因 "changed during staging" 失败。这是调试时踩过的坑。
- 迁移逻辑只处理 DB 中有记录、且 `installed_path` 指向遗留根的插件；其他（`%APPDATA%\openbox`）
  已由 L3 数据迁移覆盖，无需额外处理。

## 3. 当前发布状态（已完成）

- [x] 提交 `1a0120a` 已推送到 `origin/fix/1.9.13-regression-batch`
- [x] Tag `tauri-v1.9.14` 已推送，触发 `tauri-release.yml`
- [x] CI 运行 `33033857164` 状态：`success`（2026-08-27T03:04:53Z 完成，耗时 28m17s）
- [x] Release 已在 https://github.com/Xuancheng75/CrucibleBox/releases/tag/tauri-v1.9.14 发布
- [x] `tauri-latest` 下的 `latest.json` 的 `version` 已更新为 `1.9.14`
      （工作流最后一步会把 `latest.json` 同步到 `tauri-latest`，供已装用户滚动更新）

**Release 产物清单：**

- CrucibleBox_1.9.14_x64-setup.exe（NSIS 安装包）
- CrucibleBox_1.9.14_x64-setup.exe.sig（签名）
- latest.json（updater 清单）
- 6 个官方插件包（diary, dice-roller, gif-editor, theme-manager, turntable, unienv）
- SBOM（cruciblebox.cdx.json, cruciblebox-plugin-host.cdx.json）

**查看 CI 运行：**

```powershell
gh run view 33033857164 --repo Xuancheng75/CrucibleBox
```

**查看 Release：**

```powershell
gh release view tauri-v1.9.14 --repo Xuancheng75/CrucibleBox
```

## 4. 被排除的 document-engine WIP（重要）

工作区里还有一批**未提交**的 document-engine 功能代码，本次发布**刻意排除**：

- `src-tauri/src/document_analyzer.rs`（未跟踪）
- `src-tauri/src/document_engine_service.rs`（未跟踪，含 clippy 错误 `unreachable pattern`，且注释 "将在 Phase 3 实现"）
- `src-tauri/src/document_engine_task.rs`（未跟踪）
- `docs/document-engine-*.md`（未跟踪）
- `plugins/document-engine/`（未跟踪）

这些文件**仍保留在本地磁盘**，未被纳入 `tauri-v1.9.14` 提交。
为让发布构建通过，已从 `main.rs` 删除 `mod document_analyzer/engine_service/engine_task`，
并从 `envelope_host.rs` 移除 `"document-engine"` 分发分支。

**接手人后续处理建议（二选一）：**

1. 单独开分支/PR 提交 document-engine，修掉其 clippy 错误后再发版；
2. 或继续留在本地 WIP，待功能完成后再合入。

注意：当前 `main.rs` / `envelope_host.rs` 已不再引用这些文件，本地 `cargo build` 不会编译它们，
但文件本身仍在磁盘上，别误删。

## 5. 发布流程速查（未来发版用）

Tauri 客户端走 `.github/workflows/tauri-release.yml`，与 Electron 链的 `release.yml`（`v*` tag）互不干扰。

**发版步骤：**

1. 改三个版本号文件保持一致：
   - `src-tauri/tauri.conf.json` → `"version"`
   - `src-tauri/Cargo.toml` → `version`
   - `tauri-frontend/package.json` → `"version"`
2. 确保本地通过三门禁（CI 也会跑，失败则发布会中止）：
   ```powershell
   cd src-tauri
   cargo fmt --check      # 必须 0 错误
   cargo clippy --workspace --all-targets -- -D warnings   # 必须 0 警告
   cargo test --workspace  # 必须全绿
   ```
3. 提交、推送分支
4. 打 tag 并推送（**tag 必须精确等于 `tauri-v` + tauri.conf.json 的 version**）：
   ```powershell
   git tag tauri-vX.Y.Z
   git push origin tauri-vX.Y.Z
   ```
5. 工作流会自动：构建 NSIS → 签名 updater 产物 → 发布 GitHub Release → 同步 `latest.json` 到 `tauri-latest`。

**常见坑：**

- tag 格式必须为 `tauri-vX.Y.Z`（不是 `vX.Y.Z`）。
- tag 与 `tauri.conf.json` 的 `version` 不一致 → 工作流直接 `exit 1`。
- 新增 `.rs` 文件若未通过 `cargo fmt` / 触发 clippy 警告（如 doc 注释 `doc_lazy_continuation`、unreachable pattern），
  会导致整个发布失败。提交前务必本地跑一遍第 2 步。

## 6. CI 过程中的坑（记录）

首次发布（commit `bf15963`，CI run `32980259148`）失败：

- 原因：`scripts/plugin-catalog.json` 列出了 `document-engine` 插件，CI 的 `npm run build:plugins` 尝试构建它，
  但 `plugins/document-engine/` 是未跟踪的 WIP 目录，CI checkout 中不存在。
- 修复：从 `plugin-catalog.json` 移除 `document-engine` 条目，amend commit 后 force-push tag。
- 教训：`plugin-catalog.json` 是 CI 构建插件的清单，任何条目对应的 `plugins/<id>/` 目录必须存在于 git 中。

## 7. 回滚说明（如需）

若 1.9.14 发布后发现严重问题：

- Release 本身可在 GitHub 删除/标记为 prerelease；
- 滚动更新指向由 `tauri-latest/latest.json` 控制，可重新上传旧版 `latest.json` 回退；
- 已安装用户需手动重装或等下次更新。

---

_本文件为内部交接用，不包含密钥/签名配置（均在 GitHub Secrets：`TAURI_SIGNING_PRIVATE_KEY`、插件签名 key 等）。_
