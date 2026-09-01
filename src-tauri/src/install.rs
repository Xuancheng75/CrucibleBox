// 1.9.3 插件安装链编排层（对等 plugin-system/PluginInstaller.ts）
// 语义：
// - preview：校验来源（trusted_paths 登记 + 类型/symlink）→ 提取/定位插件根 →
//   manifest 全量校验 → 升级策略 → DirectoryTransaction.stage → 防 TOCTOU 重读 →
//   生成 installToken 存入 prepared（TTL 15min）→ 返回前端契约
// - commit：一次性消费 token → in_flight 防并发 → journal 驱动 swap/DB 更新 →
//   失败按硬性顺序回滚（全新：先删 DB 行再 rollback 目录；升级：先恢复元数据再 rollback）
// - discard：删除 token + rollback 事务 + 清理 stage
// 约束：零 unwrap/expect（panic=abort），错误一律 Result<_, String>。

use crate::archive::extract_plugin_archive;
use crate::backend_process::BackendProcessManager;
use crate::db::{Db, PluginRow, VersionFields};
use crate::journal::{clear_journal, recover_interrupted, write_journal, Journal, JOURNAL_VERSION};
use crate::manifest::{
    assert_host_version_compatible, assert_manifest_installable, assert_upgrade_allowed,
    read_manifest, validate_entrypoints, Manifest,
};
use crate::rand_token::random_token_hex;
use crate::transaction::{
    trusted_allowlist, DirectoryTransaction, RemovalTransaction, TransactionOptions,
};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const PREPARED_TTL_MS: u64 = 15 * 60 * 1000;
const TRUSTED_PATHS_CAPACITY: usize = 50;

/// 安装来源（"zip" | "directory"）
pub struct InstallSource {
    pub source_type: String,
    pub path: String,
}

/// 已预备安装（preview 产物，TTL 15min）
pub struct PreparedInstall {
    pub expires_at_ms: u64,
    pub manifest: Manifest,
    pub transaction_id: String,
    pub stage_dir: PathBuf,
    pub previous_metadata: Option<serde_json::Value>,
    /// 持有事务对象：commit/discard 直接操作（source_dir 在 preview 后可能已清理，
    /// 无法重建事务；stage/swap/rollback 均不依赖 source_dir）
    transaction: DirectoryTransaction,
}

pub struct InstallManager {
    plugins_dir: PathBuf,
    db: Arc<Mutex<Db>>,
    backend: Arc<BackendProcessManager>,
    prepared: Mutex<HashMap<String, PreparedInstall>>,
    in_flight: Mutex<HashSet<String>>,
    blocked: Mutex<HashSet<String>>,
    allow_legacy_full_trust: bool,
    trusted_paths: Mutex<Vec<PathBuf>>,
}

impl InstallManager {
    pub fn new(
        plugins_dir: PathBuf,
        db: Arc<Mutex<Db>>,
        backend: Arc<BackendProcessManager>,
    ) -> Arc<Self> {
        Arc::new(InstallManager {
            plugins_dir,
            db,
            backend,
            prepared: Mutex::new(HashMap::new()),
            in_flight: Mutex::new(HashSet::new()),
            blocked: Mutex::new(HashSet::new()),
            allow_legacy_full_trust: false,
            trusted_paths: Mutex::new(Vec::new()),
        })
    }

    /// 启动恢复：调 journal::recover_interrupted，以当前恢复报告重建 blocked 集合。
    /// 启动恢复：先把遗留根（identifier 根 com.cruciblebox.app\plugins）下的插件目录
    /// 迁移到统一根（%APPDATA%\cruciblebox\plugins）并修正 DB installed_path，再做 journal 恢复。
    pub fn run_startup_recovery(&self, legacy_roots: &[PathBuf]) {
        for legacy_root in legacy_roots {
            migrate_legacy_plugin_root(&self.plugins_dir, legacy_root, &self.db);
        }
        self.run_recovery_pass();
    }

    /// 在没有待确认/执行中安装时重新运行确定性恢复，并以本次报告替换内存阻断集。
    /// 用于进程内升级失败后的重新导入或卸载，无需用户重启应用。
    fn recover_blocked_plugin(&self, name: &str) -> Result<(), String> {
        if !self.is_blocked(name) {
            return Ok(());
        }
        if !lock(&self.prepared).is_empty() || !lock(&self.in_flight).is_empty() {
            return Err(format!(
                "{name}: recovery is unavailable while another install is in progress"
            ));
        }
        let _lifecycle = self.backend.begin_lifecycle_operation(name)?;
        self.run_recovery_pass();
        if self.is_blocked(name) {
            Err(format!(
                "{name}: plugin is blocked after an interrupted transaction"
            ))
        } else {
            Ok(())
        }
    }

    fn run_recovery_pass(&self) {
        let find_metadata = |name: &str| -> Option<serde_json::Value> {
            let db = lock(&self.db);
            match db.plugin_find_by_name(name) {
                Ok(Some(row)) => Some(plugin_row_to_metadata(&row)),
                _ => None,
            }
        };
        let restore_metadata = |name: &str, value: &serde_json::Value| -> Result<(), String> {
            restore_plugin_metadata(&self.db, name, value)
        };
        let report = recover_interrupted(&self.plugins_dir, &find_metadata, &restore_metadata);
        let mut blocked = lock(&self.blocked);
        blocked.clear();
        blocked.extend(report.blocked_plugins);
        for action in report.actions {
            eprintln!("[install recovery] {action}");
        }
    }

    /// 登记可信来源路径（容量 50，超限丢最旧）。
    pub fn remember_trusted_path(&self, p: PathBuf) {
        let mut paths = lock(&self.trusted_paths);
        if let Some(index) = paths.iter().position(|x| x == &p) {
            paths.remove(index);
        }
        paths.push(p);
        while paths.len() > TRUSTED_PATHS_CAPACITY {
            paths.remove(0);
        }
    }

    pub fn is_blocked(&self, name: &str) -> bool {
        lock(&self.blocked).contains(name)
    }

    /// Attempt deterministic journal recovery before a lifecycle operation.
    ///
    /// Older clients rejected enable/disable immediately when an interrupted
    /// transaction was detected.  That left a perfectly recoverable plugin
    /// blocked until a full restart.  Re-running the same fail-closed recovery
    /// pass is safe and lets the caller continue when the journal is resolvable.
    pub fn recover_if_blocked(&self, name: &str) -> Result<(), String> {
        self.recover_blocked_plugin(name)
    }

    /// preview：校验来源 → 提取/定位插件根 → manifest 校验 → 事务 stage → 返回前端契约。
    pub fn preview(&self, source: InstallSource) -> Result<serde_json::Value, String> {
        self.sweep_expired_prepared();

        let source_path = PathBuf::from(&source.path);
        if !self.is_trusted_path(&source_path) {
            return Err("path not trusted".into());
        }
        let meta = std::fs::symlink_metadata(&source_path)
            .map_err(|e| format!("failed to stat source path: {e}"))?;
        if meta.file_type().is_symlink() {
            return Err("source path must not be a symbolic link".into());
        }
        match source.source_type.as_str() {
            "zip" if meta.is_file() => {}
            "directory" if meta.is_dir() => {}
            "zip" => return Err("zip source must be a regular file".into()),
            "directory" => return Err("directory source must be a regular directory".into()),
            other => return Err(format!("unsupported source type: {other}")),
        }

        let mut tmp_dir: Option<PathBuf> = None;
        let extracted = if source.source_type == "zip" {
            let token = random_token_hex()?;
            let tmp = self.plugins_dir.join(format!(".tmp-{}", &token[..16]));
            extract_plugin_archive(&source_path, &tmp)?;
            tmp_dir = Some(tmp.clone());
            tmp
        } else {
            source_path.clone()
        };

        let result = self.preview_from_root(&extracted);
        if let Some(tmp) = tmp_dir {
            let _ = std::fs::remove_dir_all(&tmp);
        }
        result
    }

    /// commit：一次性消费 token，执行完整安装/升级，返回 PluginMetaDto 形状。
    pub fn commit(&self, token: String) -> Result<serde_json::Value, String> {
        let mut install = {
            let mut prepared = lock(&self.prepared);
            prepared
                .remove(&token)
                .ok_or_else(|| String::from("install token not found or expired"))?
        };
        if now_ms() > install.expires_at_ms {
            let _ = install.transaction.rollback();
            return Err("install token expired".into());
        }
        let name = install.manifest.name.clone();
        let _lifecycle = match self.backend.begin_lifecycle_operation(&name) {
            Ok(guard) => guard,
            Err(error) => {
                let _ = install.transaction.rollback();
                lock(&self.in_flight).remove(&name);
                return Err(error);
            }
        };
        {
            let mut in_flight = lock(&self.in_flight);
            if in_flight.contains(&name) {
                let _ = install.transaction.rollback();
                return Err(format!("{name}: another install is already in progress"));
            }
            in_flight.insert(name.clone());
        }
        let result = self.perform_commit(&mut install);
        lock(&self.in_flight).remove(&name);
        result
    }

    /// discard：删除 token + rollback 事务（未 committed）+ 清理 stage 目录。
    pub fn discard(&self, token: String) -> Result<(), String> {
        let mut install = {
            let mut prepared = lock(&self.prepared);
            prepared
                .remove(&token)
                .ok_or_else(|| String::from("install token not found"))?
        };
        let _ = install.transaction.rollback();
        let _ = std::fs::remove_dir_all(&install.stage_dir);
        Ok(())
    }

    /// 卸载：维护锁 → journal prepared → quarantine → 删除 DB → committed → 删除 quarantine。
    /// 所有路径操作都限制在 canonical plugins_dir 的直接子目录内，失败时保留 journal
    /// 供启动恢复器决定回滚或完成清理。
    pub fn uninstall(&self, id: &str) -> Result<serde_json::Value, String> {
        let mut row = lock(&self.db)
            .plugin_find_by_id(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("plugin not found: {id}"))?;
        if self.is_blocked(&row.name) {
            self.recover_blocked_plugin(&row.name)?;
            row = lock(&self.db)
                .plugin_find_by_id(id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("plugin not found after recovery: {id}"))?;
        }
        if lock(&self.prepared)
            .values()
            .any(|prepared| prepared.manifest.name == row.name)
        {
            return Err(format!(
                "{}: install preview is waiting for confirmation",
                row.name
            ));
        }
        let _lifecycle = self.backend.begin_lifecycle_operation(id)?;
        let _maintenance = self.backend.enter_maintenance(id)?;
        if let Err(error) = self.backend.deactivate(id) {
            return Err(format!("failed to deactivate plugin: {error}"));
        }
        self.perform_uninstall(&row)
    }

    fn perform_uninstall(&self, row: &PluginRow) -> Result<serde_json::Value, String> {
        let target = self.plugins_dir.join(&row.name);
        if !target.exists() {
            lock(&self.db).plugin_delete(&row.id)?;
            return Ok(json!({
                "success": true,
                "data": { "id": row.id, "name": row.name, "directoryMissing": true }
            }));
        }
        let transaction_id = random_token_hex()?;
        let mut txn = RemovalTransaction::new(
            self.plugins_dir.clone(),
            row.name.clone(),
            transaction_id.clone(),
        )?;
        let journal = Journal {
            version: JOURNAL_VERSION,
            operation: "uninstall".into(),
            phase: "prepared".into(),
            plugin_name: row.name.clone(),
            transaction_id,
            previous_metadata: Some(plugin_row_to_metadata(row)),
            created_at: now_iso(),
        };
        write_journal(txn.target_dir(), &journal)?;
        if let Err(error) = txn.quarantine() {
            let _ = clear_journal(txn.target_dir(), &journal);
            return Err(error);
        }
        let applied = Journal {
            phase: "applied".into(),
            ..journal.clone()
        };
        if let Err(error) = write_journal(txn.quarantine_dir(), &applied) {
            let rollback = txn.rollback();
            let _ = clear_journal(txn.target_dir(), &journal);
            if let Err(rollback_error) = rollback {
                self.block(&row.name);
                return Err(format!("{error}; rollback failed: {rollback_error}"));
            }
            return Err(error);
        }
        if let Err(error) = lock(&self.db).plugin_delete(&row.id) {
            let rollback = txn.rollback();
            let _ = clear_journal(txn.target_dir(), &journal);
            if let Err(rollback_error) = rollback {
                self.block(&row.name);
                return Err(format!("{error}; rollback failed: {rollback_error}"));
            }
            return Err(error);
        }
        let committed = Journal {
            phase: "committed".into(),
            ..journal
        };
        if let Err(error) = write_journal(txn.quarantine_dir(), &committed) {
            self.block(&row.name);
            return Err(error);
        }
        if let Err(error) = txn.commit() {
            self.block(&row.name);
            return Err(error);
        }
        Ok(json!({
            "success": true,
            "data": { "id": row.id, "name": row.name, "directoryMissing": false }
        }))
    }

    // -----------------------------------------------------------------------
    // 内部实现
    // -----------------------------------------------------------------------

    fn is_trusted_path(&self, p: &Path) -> bool {
        lock(&self.trusted_paths).iter().any(|t| t == p)
    }

    fn sweep_expired_prepared(&self) {
        let now = now_ms();
        let mut prepared = lock(&self.prepared);
        let expired: Vec<String> = prepared
            .iter()
            .filter(|(_, p)| now > p.expires_at_ms)
            .map(|(k, _)| k.clone())
            .collect();
        for token in expired {
            if let Some(mut install) = prepared.remove(&token) {
                let _ = install.transaction.rollback();
            }
        }
    }

    fn preview_from_root(&self, extracted: &Path) -> Result<serde_json::Value, String> {
        let root = resolve_plugin_root(extracted)?;
        reject_host_markers(&root)?;
        let manifest = read_manifest(&root)?;
        validate_entrypoints(&root, &manifest)?;
        assert_manifest_installable(&manifest, self.allow_legacy_full_trust)?;
        assert_host_version_compatible(&manifest, env!("CARGO_PKG_VERSION"))?;
        if self.is_blocked(&manifest.name) {
            self.recover_blocked_plugin(&manifest.name)?;
        }
        if lock(&self.prepared)
            .values()
            .any(|prepared| prepared.manifest.name == manifest.name)
        {
            return Err(format!(
                "{}: another install preview is already waiting for confirmation",
                manifest.name
            ));
        }

        let mut existing = lock(&self.db)
            .plugin_find_by_name(&manifest.name)
            .map_err(|e| e.to_string())?;
        // A completed uninstall can leave a stale row when an older build was
        // terminated between quarantine and the DB commit.  Do not present
        // that ghost row as an upgrade (for example v0.1.1 -> v0.1.2): when
        // its recorded directory is gone or no longer contains the same
        // manifest, remove only the stale record and continue as a fresh
        // install.  The normal orphan isolation below protects any leftover
        // directory from being overwritten.
        if let Some(row) = &existing {
            let installed_path = Path::new(&row.installed_path);
            let manifest_matches = installed_path.is_dir()
                && read_manifest(installed_path)
                    .map(|installed| {
                        installed.name == manifest.name && installed.version == row.version
                    })
                    .unwrap_or(false);
            if !manifest_matches {
                lock(&self.db).plugin_delete(&row.id)?;
                existing = None;
            }
        }
        let is_upgrade = existing.is_some();
        if let Some(row) = &existing {
            assert_upgrade_allowed(&manifest.name, &manifest.version, &row.version)?;
        }

        // 1.9.14：若 DB 无记录但目标目录已存在（旧版本卸载残留 / 双根错乱的孤儿目录），
        // 必须在 stage 之前隔离，否则 DirectoryTransaction 会因 "expected to be absent" 失败。
        // 隔离到 plugins_dir/.orphans/<name>-<ts>，允许本次重装。
        if !is_upgrade {
            let target_dir = self.plugins_dir.join(&manifest.name);
            if target_dir.is_dir() {
                let orphans = self.plugins_dir.join(".orphans");
                let _ = std::fs::create_dir_all(&orphans);
                let stamped = orphans.join(format!("{}-{}", manifest.name, now_ms()));
                if let Err(e) = std::fs::rename(&target_dir, &stamped) {
                    eprintln!(
                        "[preview] isolate orphan {} failed ({e}); attempting delete",
                        manifest.name
                    );
                    let _ = std::fs::remove_dir_all(&target_dir);
                }
            }
        }

        let transaction_id = random_token_hex()?;
        let mut txn = DirectoryTransaction::new(TransactionOptions {
            plugins_dir: self.plugins_dir.clone(),
            plugin_name: manifest.name.clone(),
            transaction_id: transaction_id.clone(),
            source_dir: root.to_path_buf(),
            expected_target_exists: is_upgrade,
            allowed_files: trusted_allowlist(&manifest.permissions),
        })?;
        txn.stage()?;

        // stage 后重读 stage 目录 manifest 并与预期比对（防 TOCTOU）
        let stage_manifest = read_manifest(txn.stage_dir())?;
        if stage_manifest != manifest {
            let _ = txn.rollback();
            return Err("plugin manifest changed while it was being staged".into());
        }

        let previous_metadata = existing.as_ref().map(plugin_row_to_metadata);

        let token = random_token_hex()?;
        let expires_at_ms = now_ms() + PREPARED_TTL_MS;
        lock(&self.prepared).insert(
            token.clone(),
            PreparedInstall {
                expires_at_ms,
                manifest: manifest.clone(),
                transaction_id,
                stage_dir: txn.stage_dir().to_path_buf(),
                previous_metadata,
                transaction: txn,
            },
        );

        let old_permissions: Vec<String> = existing
            .as_ref()
            .map(|r| serde_json::from_str(&r.permissions).unwrap_or_default())
            .unwrap_or_default();
        let new_permissions = manifest.permissions.clone();
        let added: Vec<String> = new_permissions
            .iter()
            .filter(|p| !old_permissions.contains(p))
            .cloned()
            .collect();
        let removed: Vec<String> = old_permissions
            .iter()
            .filter(|p| !new_permissions.contains(p))
            .cloned()
            .collect();

        Ok(json!({
            "success": true,
            "installToken": token,
            "data": {
                "isUpgrade": is_upgrade,
                "version": manifest.version,
                "previousVersion": existing.as_ref().map(|r| r.version.clone()),
                "permissions": new_permissions,
                "addedPermissions": added,
                "removedPermissions": removed,
                "legacyFullTrust": manifest.permissions.iter().any(|p| p == "trusted:unienv"),
            }
        }))
    }

    fn perform_commit(&self, install: &mut PreparedInstall) -> Result<serde_json::Value, String> {
        let name = install.manifest.name.clone();
        let target_dir = self.plugins_dir.join(&name);
        if install.previous_metadata.is_some() {
            self.commit_upgrade(install, &name, &target_dir)
        } else {
            self.commit_fresh(install, &name, &target_dir)
        }
    }

    fn commit_fresh(
        &self,
        install: &mut PreparedInstall,
        name: &str,
        target_dir: &Path,
    ) -> Result<serde_json::Value, String> {
        let db_has_record = lock(&self.db)
            .plugin_find_by_name(name)
            .map_err(|e| e.to_string())?
            .is_some();
        if !db_has_record && target_dir.exists() {
            // 理论上 preview 阶段已隔离孤儿目录；此处仅作兜底，不应触发。
            let _ = install.transaction.rollback();
            return Err(format!(
                "{name}: plugin directory already exists but has no database record"
            ));
        }

        let journal = Journal {
            version: JOURNAL_VERSION,
            operation: "install".into(),
            phase: "prepared".into(),
            plugin_name: name.to_string(),
            transaction_id: install.transaction_id.clone(),
            previous_metadata: None,
            created_at: now_iso(),
        };
        if let Err(e) = write_journal(install.transaction.stage_dir(), &journal) {
            let _ = install.transaction.rollback();
            return Err(e);
        }
        if let Err(e) = install.transaction.swap() {
            self.rollback_fresh(install);
            return Err(e);
        }
        let applied = Journal {
            phase: "applied".into(),
            ..journal.clone()
        };
        if let Err(e) = write_journal(target_dir, &applied) {
            self.rollback_fresh(install);
            return Err(e);
        }
        let id = name.to_string();
        let row = PluginRow {
            id: id.clone(),
            name: name.to_string(),
            version: install.manifest.version.clone(),
            display_name: install.manifest.display_name.clone(),
            description: install.manifest.description.clone(),
            author: install.manifest.author.clone(),
            icon: install.manifest.icon.clone().unwrap_or_default(),
            entry_main: install.manifest.main.clone(),
            entry_renderer: install.manifest.renderer.clone(),
            permissions: serde_json::to_string(&install.manifest.permissions)
                .map_err(|e| e.to_string())?,
            config_schema: serde_json::to_string(&install.manifest.config)
                .map_err(|e| e.to_string())?,
            config_data: serde_json::to_string(&config_defaults(&install.manifest.config))
                .map_err(|e| e.to_string())?,
            enabled: false,
            installed_path: target_dir.to_string_lossy().into_owned(),
            installed_at: String::new(),
            updated_at: String::new(),
            sort_order: 0,
        };
        if let Err(e) = lock(&self.db).plugin_create(&row) {
            self.rollback_fresh(install);
            return Err(e);
        }
        let committed = Journal {
            phase: "committed".into(),
            ..journal.clone()
        };
        if let Err(e) = write_journal(target_dir, &committed) {
            self.block(name);
            return Err(e);
        }
        if let Err(e) = install.transaction.commit() {
            self.block(name);
            return Err(e);
        }
        let _ = clear_journal(target_dir, &committed);
        Ok(json!({
            "success": true,
            "data": plugin_meta_dto(&self.db, &id)?,
        }))
    }

    fn commit_upgrade(
        &self,
        install: &mut PreparedInstall,
        name: &str,
        target_dir: &Path,
    ) -> Result<serde_json::Value, String> {
        // swap 前读 DB 全行快照 previous_metadata（含 enabled）
        let existing = lock(&self.db)
            .plugin_find_by_name(name)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("{name}: plugin record missing during upgrade"))?;
        let id = existing.id.clone();
        let was_enabled = existing.enabled;
        let previous_metadata = plugin_row_to_metadata(&existing);
        let maintenance = match self.backend.enter_maintenance(&id) {
            Ok(guard) => guard,
            Err(error) => {
                let _ = install.transaction.rollback();
                return Err(error);
            }
        };

        let journal = Journal {
            version: JOURNAL_VERSION,
            operation: "upgrade".into(),
            phase: "prepared".into(),
            plugin_name: name.to_string(),
            transaction_id: install.transaction_id.clone(),
            previous_metadata: Some(previous_metadata.clone()),
            created_at: now_iso(),
        };
        if let Err(e) = write_journal(install.transaction.stage_dir(), &journal) {
            let _ = install.transaction.rollback();
            return Err(e);
        }
        if was_enabled {
            if let Err(error) = self.backend.deactivate(&id) {
                let _ = install.transaction.rollback();
                return Err(format!(
                    "failed to deactivate plugin before upgrade: {error}"
                ));
            }
        }
        if let Err(e) = install.transaction.swap() {
            return match self.rollback_upgrade(install, &previous_metadata) {
                Ok(()) => Err(e),
                Err(rollback_error) => {
                    self.block(name);
                    Err(format!("{e}; rollback failed: {rollback_error}"))
                }
            };
        }
        let applied = Journal {
            phase: "applied".into(),
            ..journal.clone()
        };
        if let Err(e) = write_journal(target_dir, &applied) {
            return match self.rollback_upgrade(install, &previous_metadata) {
                Ok(()) => Err(e),
                Err(rollback_error) => {
                    self.block(name);
                    Err(format!("{e}; rollback failed: {rollback_error}"))
                }
            };
        }
        let fields = manifest_to_version_fields(&install.manifest, target_dir);
        if let Err(e) = lock(&self.db).plugin_update_version(&id, &fields) {
            return match self.rollback_upgrade(install, &previous_metadata) {
                Ok(()) => Err(e),
                Err(rollback_error) => {
                    self.block(name);
                    Err(format!("{e}; rollback failed: {rollback_error}"))
                }
            };
        }
        if was_enabled {
            // commit() 仍持有插件生命周期单飞锁；先释放维护窗口，重新激活才不会
            // 被 ensure_activated() 以 "plugin is in maintenance" 必然拒绝。
            drop(maintenance);
            let record = lock(&self.db).plugin_backend_record(&id).ok().flatten();
            if let Some(record) = record {
                if let Err(error) = self.backend.ensure_activated(&id, record) {
                    if let Err(rollback_error) = self.rollback_upgrade(install, &previous_metadata)
                    {
                        self.block(name);
                        return Err(format!(
                            "failed to reactivate plugin after upgrade: {error}; rollback failed: {rollback_error}"
                        ));
                    }
                    let restored = lock(&self.db).plugin_backend_record(&id).ok().flatten();
                    if let Some(restored) = restored {
                        if let Err(restored_error) = self.backend.ensure_activated(&id, restored) {
                            let _ = lock(&self.db).set_plugin_enabled(&id, false);
                            return Err(format!(
                                "failed to reactivate plugin after upgrade: {error}; upgrade rolled back, but the previous plugin could not be reactivated: {restored_error}"
                            ));
                        }
                    }
                    return Err(format!(
                        "failed to reactivate plugin after upgrade: {error}; upgrade rolled back"
                    ));
                }
            }
        }
        let committed = Journal {
            phase: "committed".into(),
            ..journal.clone()
        };
        if let Err(e) = write_journal(target_dir, &committed) {
            self.block(name);
            return Err(e);
        }
        if let Err(e) = install.transaction.commit() {
            self.block(name);
            return Err(e);
        }
        let _ = clear_journal(target_dir, &committed);
        Ok(json!({
            "success": true,
            "data": plugin_meta_dto(&self.db, &id)?,
        }))
    }

    /// 全新安装回滚（硬性顺序）：先删 DB 行 → 成功后才 rollback 目录；
    /// DB 删失败 → blocked，不碰目录。
    fn rollback_fresh(&self, install: &mut PreparedInstall) {
        let name = install.manifest.name.clone();
        let id = match lock(&self.db).plugin_find_by_name(&name) {
            Ok(Some(row)) => row.id,
            _ => {
                // 无 DB 行（swap 前失败）→ 直接 rollback 目录
                if let Err(e) = install.transaction.rollback() {
                    eprintln!("[install] rollback directory failed: {e}");
                    self.block(&name);
                }
                return;
            }
        };
        if let Err(e) = lock(&self.db).plugin_delete(&id) {
            eprintln!("[install] rollback: DB delete failed: {e}");
            self.block(&name);
            return;
        }
        if let Err(e) = install.transaction.rollback() {
            eprintln!("[install] rollback directory failed: {e}");
            self.block(&name);
        }
    }

    /// 升级回滚（硬性顺序）：先恢复 previous_metadata → 成功后才 rollback 目录。
    /// 调用方仅在回滚本身失败时设置 blocked；普通激活失败不再留下永久阻断。
    fn rollback_upgrade(
        &self,
        install: &mut PreparedInstall,
        previous_metadata: &serde_json::Value,
    ) -> Result<(), String> {
        let name = &install.manifest.name;
        restore_plugin_metadata(&self.db, name, previous_metadata)
            .map_err(|error| format!("metadata restore failed: {error}"))?;
        install
            .transaction
            .rollback()
            .map_err(|error| format!("directory rollback failed: {error}"))
    }

    fn block(&self, name: &str) {
        lock(&self.blocked).insert(name.to_string());
    }
}

// ---------------------------------------------------------------------------
// 内部辅助
// ---------------------------------------------------------------------------

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 把遗留根（如 %APPDATA%\com.cruciblebox.app\plugins）下的插件目录迁移到统一根
/// （%APPDATA%\cruciblebox\plugins），并修正 DB 中的 installed_path。
///
/// 策略：
///
/// - 遗留目录存在、统一根不存在 → rename（跨盘失败则 copy+delete 回退）
/// - 两边都存在 → 保留统一根，删除遗留副本
/// - 更新 DB 路径指向统一根
///
/// 最后尝试删除已清空的遗留根目录。
fn migrate_legacy_plugin_root(
    canonical_plugins_dir: &Path,
    legacy_root: &Path,
    db: &Arc<Mutex<Db>>,
) {
    if !legacy_root.is_dir() {
        return;
    }
    let rows = match lock(db).plugin_all_roots() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[reconcile] failed to read plugin roots: {e}");
            return;
        }
    };
    for (id, name, installed_path) in rows {
        let installed = PathBuf::from(&installed_path);
        if !installed.starts_with(legacy_root) {
            continue;
        }
        let canonical_target = canonical_plugins_dir.join(&name);
        if installed.is_dir() {
            if canonical_target.exists() {
                let _ = std::fs::remove_dir_all(&installed);
            } else if let Err(e) = std::fs::rename(&installed, &canonical_target) {
                eprintln!("[reconcile] rename {name} failed ({e}); trying copy fallback");
                if copy_dir_recursive(&installed, &canonical_target).is_ok() {
                    let _ = std::fs::remove_dir_all(&installed);
                } else {
                    eprintln!("[reconcile] failed to migrate plugin {name} from legacy root");
                    continue;
                }
            }
        }
        let new_path = canonical_target.to_string_lossy().into_owned();
        if installed != canonical_target {
            if let Err(e) = lock(db).plugin_update_installed_path(&id, &new_path) {
                eprintln!("[reconcile] failed to update path for {name}: {e}");
            }
        }
    }
    let _ = std::fs::remove_dir(legacy_root);
}

/// 递归复制目录（rename 跨盘失败时的回退方案）。
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else {
            std::fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn now_iso() -> String {
    format!("{}", now_ms())
}

/// resolve_plugin_root：根含 plugin.json → 根；否则恰好一个非 symlink 子目录含
/// plugin.json → 下钻；否则用根（后续 read_manifest 会报 plugin.json 缺失）。
fn resolve_plugin_root(root: &Path) -> Result<PathBuf, String> {
    if root.join("plugin.json").is_file() {
        return Ok(root.to_path_buf());
    }
    let entries =
        std::fs::read_dir(root).map_err(|e| format!("failed to read plugin root: {e}"))?;
    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.is_dir() && !meta.file_type().is_symlink() && path.join("plugin.json").is_file() {
            candidates.push(path);
        }
    }
    if candidates.len() == 1 {
        return Ok(candidates.remove(0));
    }
    Ok(root.to_path_buf())
}

/// 拒绝含 .openbox-host-transaction.json / .pending 标记的候选。
fn reject_host_markers(root: &Path) -> Result<(), String> {
    for marker in [
        crate::journal::JOURNAL_FILENAME,
        ".openbox-host-transaction.json.pending",
    ] {
        if root.join(marker).exists() {
            return Err(format!(
                "plugin root contains reserved host marker: {marker}"
            ));
        }
    }
    Ok(())
}

/// 从 manifest.config schema 生成默认值（number→0、boolean→false、string/select→""、
/// multiselect→[]、有 default 用 default）。
fn config_defaults(config: &serde_json::Map<String, serde_json::Value>) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (key, field) in config {
        let value = field.get("default").cloned().unwrap_or_else(|| {
            match field.get("type").and_then(|t| t.as_str()) {
                Some("number") => json!(0),
                Some("boolean") => json!(false),
                Some("multiselect") => json!([]),
                _ => json!(""), // string/select/未知 → ""
            }
        });
        map.insert(key.clone(), value);
    }
    serde_json::Value::Object(map)
}

fn manifest_to_version_fields(manifest: &Manifest, installed_path: &Path) -> VersionFields {
    VersionFields {
        version: manifest.version.clone(),
        display_name: manifest.display_name.clone(),
        description: manifest.description.clone(),
        author: manifest.author.clone(),
        icon: manifest.icon.clone().unwrap_or_default(),
        entry_main: manifest.main.clone(),
        entry_renderer: manifest.renderer.clone(),
        permissions: serde_json::to_string(&manifest.permissions).unwrap_or_else(|_| "[]".into()),
        config_schema: serde_json::to_string(&manifest.config).unwrap_or_else(|_| "{}".into()),
        installed_path: installed_path.to_string_lossy().into_owned(),
    }
}

/// PluginRow → serde_json 快照（journal previous_metadata / 恢复用，形状与
/// plugin_row_to_metadata 一致）。
fn plugin_row_to_metadata(row: &PluginRow) -> serde_json::Value {
    json!({
        "id": row.id,
        "name": row.name,
        "version": row.version,
        "display_name": row.display_name,
        "description": row.description,
        "author": row.author,
        "icon": row.icon,
        "entry_main": row.entry_main,
        "entry_renderer": row.entry_renderer,
        "permissions": row.permissions,
        "config_schema": row.config_schema,
        "config_data": row.config_data,
        "enabled": row.enabled,
        "installed_path": row.installed_path,
        "installed_at": row.installed_at,
        "updated_at": row.updated_at,
        "sort_order": row.sort_order,
    })
}

/// 恢复插件元数据（journal 恢复 + 升级回滚共用）：update_version 回旧值 +
/// set_plugin_enabled 回旧 enabled。
fn restore_plugin_metadata(
    db: &Arc<Mutex<Db>>,
    name: &str,
    value: &serde_json::Value,
) -> Result<(), String> {
    let db = lock(db);
    let row = db
        .plugin_find_by_name(name)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("plugin not found: {name}"))?;
    let id = row.id.clone();
    let get = |key: &str, fallback: &str| -> String {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(fallback)
            .to_string()
    };
    let fields = VersionFields {
        version: get("version", &row.version),
        display_name: get("display_name", &row.display_name),
        description: get("description", &row.description),
        author: get("author", &row.author),
        icon: get("icon", &row.icon),
        entry_main: get("entry_main", &row.entry_main),
        entry_renderer: get("entry_renderer", &row.entry_renderer),
        permissions: get("permissions", &row.permissions),
        config_schema: get("config_schema", &row.config_schema),
        installed_path: get("installed_path", &row.installed_path),
    };
    db.plugin_update_version(&id, &fields)?;
    let enabled = value
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(row.enabled);
    db.set_plugin_enabled(&id, enabled)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 组装 PluginMetaDto 形状（与 commands.rs plugin_list 同构，camelCase）。
fn plugin_meta_dto(db: &Arc<Mutex<Db>>, id: &str) -> Result<serde_json::Value, String> {
    let db = lock(db);
    let row = db
        .plugin_find_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("plugin not found: {id}"))?;
    Ok(json!({
        "id": row.id,
        "name": row.name,
        "version": row.version,
        "displayName": row.display_name,
        "description": row.description,
        "author": row.author,
        "icon": row.icon,
        "entryMain": row.entry_main,
        "entryRenderer": row.entry_renderer,
        "permissions": serde_json::from_str::<serde_json::Value>(&row.permissions)
            .unwrap_or_else(|_| json!([])),
        "configSchema": serde_json::from_str::<serde_json::Value>(&row.config_schema)
            .unwrap_or_else(|_| json!({})),
        "configData": serde_json::from_str::<serde_json::Value>(&row.config_data)
            .unwrap_or_else(|_| json!({})),
        "enabled": row.enabled,
        "installedPath": row.installed_path,
        "installedAt": row.installed_at,
        "updatedAt": row.updated_at,
        "sortOrder": row.sort_order,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cruciblebox-install-test-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_fixture(root: &Path, version: &str) {
        let manifest = json!({
            "name": "demo",
            "version": version,
            "displayName": "Demo Plugin",
            "description": "",
            "author": "",
            "main": "dist/main.js",
            "renderer": "dist/renderer.js",
            "manifestVersion": 2,
            "rendererApiVersion": 2,
            "backend": false,
            "permissions": ["database:read"],
            "config": {
                "count": { "type": "number", "label": "Count" },
                "flag": { "type": "boolean", "label": "Flag" },
                "name": { "type": "string", "label": "Name" },
                "tags": { "type": "multiselect", "label": "Tags" }
            }
        });
        std::fs::create_dir_all(root.join("dist")).unwrap();
        std::fs::write(root.join("plugin.json"), manifest.to_string()).unwrap();
        std::fs::write(root.join("dist/main.js"), "// main").unwrap();
        std::fs::write(root.join("dist/renderer.js"), "// renderer").unwrap();
    }

    fn setup(tag: &str) -> (Arc<InstallManager>, PathBuf, PathBuf) {
        let root = temp_root(tag);
        let plugins_dir = root.join("plugins");
        std::fs::create_dir_all(&plugins_dir).unwrap();
        let db_path = root.join("data").join("openbox.db");
        std::fs::create_dir_all(root.join("data")).unwrap();
        let db = Arc::new(Mutex::new(Db::open(&db_path).unwrap()));
        let backend = BackendProcessManager::new(db.clone());
        let mgr = InstallManager::new(plugins_dir.clone(), db, backend);
        (mgr, plugins_dir, root)
    }

    fn has_prefix(dir: &Path, prefix: &str) -> bool {
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .any(|e| e.file_name().to_string_lossy().starts_with(prefix))
            })
            .unwrap_or(false)
    }

    fn directory_source(path: &Path) -> InstallSource {
        InstallSource {
            source_type: "directory".into(),
            path: path.to_string_lossy().into_owned(),
        }
    }

    #[test]
    fn preview_rejects_untrusted_path() {
        let (mgr, _plugins, root) = setup("untrusted");
        let source_dir = root.join("source");
        write_fixture(&source_dir, "1.0.0");
        let err = mgr.preview(directory_source(&source_dir)).unwrap_err();
        assert!(err.contains("path not trusted"), "error: {err}");
    }

    #[test]
    fn preview_commit_full_flow() {
        let (mgr, plugins, root) = setup("full-flow");
        let source_dir = root.join("source");
        write_fixture(&source_dir, "1.0.0");
        mgr.remember_trusted_path(source_dir.clone());

        let preview = mgr.preview(directory_source(&source_dir)).unwrap();
        assert_eq!(preview["success"], true);
        assert_eq!(preview["data"]["isUpgrade"], false);
        assert_eq!(preview["data"]["version"], "1.0.0");
        assert_eq!(preview["data"]["permissions"][0], "database:read");
        assert_eq!(preview["data"]["addedPermissions"][0], "database:read");
        assert!(preview["data"]["removedPermissions"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(preview["data"]["legacyFullTrust"], false);
        let token = preview["installToken"].as_str().unwrap().to_string();

        let commit = mgr.commit(token).unwrap();
        assert_eq!(commit["success"], true);
        assert_eq!(commit["data"]["name"], "demo");
        assert_eq!(commit["data"]["version"], "1.0.0");
        assert_eq!(commit["data"]["enabled"], false);
        assert_eq!(commit["data"]["permissions"][0], "database:read");

        // 目录已落地
        assert!(plugins.join("demo").join("dist/main.js").exists());
        assert!(plugins.join("demo").join("plugin.json").exists());
        // stage 已清理
        assert!(!has_prefix(&plugins, ".demo.stage-"));
    }

    #[test]
    fn preview_rejects_duplicate_pending_install_for_same_plugin() {
        let (mgr, plugins, root) = setup("duplicate-preview");
        let source_dir = root.join("source");
        write_fixture(&source_dir, "1.0.0");
        mgr.remember_trusted_path(source_dir.clone());

        let first = mgr.preview(directory_source(&source_dir)).unwrap();
        let error = mgr.preview(directory_source(&source_dir)).unwrap_err();
        assert!(error.contains("another install preview"));
        mgr.discard(first["installToken"].as_str().unwrap().to_string())
            .unwrap();
        assert!(!has_prefix(&plugins, ".demo.stage-"));
    }

    #[test]
    fn commit_creates_db_record_disabled() {
        let (mgr, _plugins, root) = setup("db-record");
        let source_dir = root.join("source");
        write_fixture(&source_dir, "1.0.0");
        mgr.remember_trusted_path(source_dir.clone());
        let preview = mgr.preview(directory_source(&source_dir)).unwrap();
        let token = preview["installToken"].as_str().unwrap().to_string();
        mgr.commit(token).unwrap();

        let db = lock(&mgr.db);
        let row = db.plugin_find_by_name("demo").unwrap().unwrap();
        assert!(!row.enabled, "new install must be disabled");
        assert_eq!(row.version, "1.0.0");
        assert_eq!(row.permissions, r#"["database:read"]"#);
        // config_data 由 schema defaults 生成
        let config_data: serde_json::Value = serde_json::from_str(&row.config_data).unwrap();
        assert_eq!(config_data["count"], 0);
        assert_eq!(config_data["flag"], false);
        assert_eq!(config_data["name"], "");
        assert_eq!(config_data["tags"], json!([]));
        assert!(row.installed_path.ends_with("demo"));
    }

    #[test]
    fn upgrade_rejects_downgrade() {
        let (mgr, _plugins, root) = setup("downgrade");
        // 先安装 1.0.0
        let source_dir = root.join("source");
        write_fixture(&source_dir, "1.0.0");
        mgr.remember_trusted_path(source_dir.clone());
        let preview = mgr.preview(directory_source(&source_dir)).unwrap();
        let token = preview["installToken"].as_str().unwrap().to_string();
        mgr.commit(token).unwrap();

        // 尝试降级到 0.9.0
        let downgrade_dir = root.join("downgrade");
        write_fixture(&downgrade_dir, "0.9.0");
        mgr.remember_trusted_path(downgrade_dir.clone());
        let err = mgr.preview(directory_source(&downgrade_dir)).unwrap_err();
        assert!(err.contains("downgrade"), "error: {err}");
    }

    #[test]
    fn upgrade_commit_updates_version() {
        let (mgr, plugins, root) = setup("upgrade-ok");
        let source_dir = root.join("source");
        write_fixture(&source_dir, "1.0.0");
        mgr.remember_trusted_path(source_dir.clone());
        let preview = mgr.preview(directory_source(&source_dir)).unwrap();
        let token = preview["installToken"].as_str().unwrap().to_string();
        mgr.commit(token).unwrap();

        // 升级到 1.1.0
        let upgrade_dir = root.join("upgrade");
        write_fixture(&upgrade_dir, "1.1.0");
        mgr.remember_trusted_path(upgrade_dir.clone());
        let preview = mgr.preview(directory_source(&upgrade_dir)).unwrap();
        assert_eq!(preview["data"]["isUpgrade"], true);
        assert_eq!(preview["data"]["previousVersion"], "1.0.0");
        assert_eq!(
            preview["data"]["addedPermissions"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        let token = preview["installToken"].as_str().unwrap().to_string();
        let commit = mgr.commit(token).unwrap();
        assert_eq!(commit["data"]["version"], "1.1.0");

        let db = lock(&mgr.db);
        let row = db.plugin_find_by_name("demo").unwrap().unwrap();
        assert_eq!(row.version, "1.1.0");
        drop(db);
        // backup 已清理
        assert!(!has_prefix(&plugins, ".demo.backup-"));
    }

    #[test]
    fn enabled_upgrade_never_leaves_plugin_permanently_blocked() {
        let (mgr, plugins, root) = setup("enabled-upgrade");
        let source_dir = root.join("source");
        write_fixture(&source_dir, "1.0.0");
        mgr.remember_trusted_path(source_dir.clone());
        let preview = mgr.preview(directory_source(&source_dir)).unwrap();
        mgr.commit(preview["installToken"].as_str().unwrap().to_string())
            .unwrap();
        lock(&mgr.db).set_plugin_enabled("demo", true).unwrap();

        let upgrade_dir = root.join("upgrade");
        write_fixture(&upgrade_dir, "1.1.0");
        mgr.remember_trusted_path(upgrade_dir.clone());
        let preview = mgr.preview(directory_source(&upgrade_dir)).unwrap();
        let result = mgr.commit(preview["installToken"].as_str().unwrap().to_string());

        assert!(
            !mgr.is_blocked("demo"),
            "an activation failure must roll back instead of blocking the plugin"
        );
        let row = lock(&mgr.db).plugin_find_by_name("demo").unwrap().unwrap();
        if result.is_ok() {
            assert_eq!(row.version, "1.1.0");
        } else {
            assert_eq!(row.version, "1.0.0");
        }
        assert!(!has_prefix(&plugins, ".demo.backup-"));
        assert!(!has_prefix(&plugins, ".demo.stage-"));
        let _ = mgr.backend.deactivate("demo");
    }

    #[test]
    fn blocked_plugin_can_recover_and_uninstall_without_restart() {
        let (mgr, plugins, root) = setup("blocked-uninstall");
        let source_dir = root.join("source");
        write_fixture(&source_dir, "1.0.0");
        mgr.remember_trusted_path(source_dir.clone());
        let preview = mgr.preview(directory_source(&source_dir)).unwrap();
        mgr.commit(preview["installToken"].as_str().unwrap().to_string())
            .unwrap();
        mgr.block("demo");

        let result = mgr.uninstall("demo").unwrap();
        assert_eq!(result["success"], true);
        assert!(!mgr.is_blocked("demo"));
        assert!(!plugins.join("demo").exists());
        assert!(lock(&mgr.db).plugin_find_by_name("demo").unwrap().is_none());
    }

    #[test]
    fn discard_cleans_stage() {
        let (mgr, plugins, root) = setup("discard");
        let source_dir = root.join("source");
        write_fixture(&source_dir, "1.0.0");
        mgr.remember_trusted_path(source_dir.clone());
        let preview = mgr.preview(directory_source(&source_dir)).unwrap();
        let token = preview["installToken"].as_str().unwrap().to_string();

        // 确认 stage 存在
        assert!(has_prefix(&plugins, ".demo.stage-"));

        mgr.discard(token).unwrap();
        assert!(!has_prefix(&plugins, ".demo.stage-"));
        assert!(!plugins.join("demo").exists());
    }

    #[test]
    fn commit_fresh_tolerates_orphan_directory() {
        let (mgr, plugins, root) = setup("orphan-tolerate");
        let source_dir = root.join("source");
        write_fixture(&source_dir, "1.0.0");
        mgr.remember_trusted_path(source_dir.clone());

        // 首次安装
        let preview = mgr.preview(directory_source(&source_dir)).unwrap();
        let token = preview["installToken"].as_str().unwrap().to_string();
        mgr.commit(token).unwrap();

        // 模拟旧版本卸载：删 DB 行但保留目录（孤儿目录）
        {
            let db = lock(&mgr.db);
            db.plugin_delete("demo").unwrap();
        }
        assert!(plugins.join("demo").join("plugin.json").exists());

        // 重新安装：应隔离孤儿目录并成功（修复 "expected to be absent"）
        let preview2 = mgr.preview(directory_source(&source_dir)).unwrap();
        let token2 = preview2["installToken"].as_str().unwrap().to_string();
        mgr.commit(token2).unwrap();

        // 安装成功，孤儿被隔离到 .orphans
        assert!(plugins.join("demo").join("plugin.json").exists());
        assert!(plugins.join(".orphans").is_dir());
        let db = lock(&mgr.db);
        assert!(db.plugin_find_by_name("demo").unwrap().is_some());
    }

    #[test]
    fn migrate_legacy_plugin_root_moves_directory_and_updates_db() {
        let (mgr, plugins, root) = setup("migrate-legacy");
        let legacy_root = root.join("com.cruciblebox.app").join("plugins");
        std::fs::create_dir_all(&legacy_root).unwrap();
        let legacy_plugin = legacy_root.join("demo");
        write_fixture(&legacy_plugin, "1.0.0");

        // DB 记录指向遗留根
        {
            let db = lock(&mgr.db);
            db.plugin_create(&crate::db::PluginRow {
                id: "demo".into(),
                name: "demo".into(),
                version: "1.0.0".into(),
                display_name: "Demo".into(),
                description: "".into(),
                author: "".into(),
                icon: "".into(),
                entry_main: "dist/main.js".into(),
                entry_renderer: "".into(),
                permissions: "[]".into(),
                config_schema: "{}".into(),
                config_data: "{}".into(),
                enabled: false,
                installed_path: legacy_plugin.to_string_lossy().into_owned(),
                installed_at: String::new(),
                updated_at: String::new(),
                sort_order: 0,
            })
            .unwrap();
        }

        super::migrate_legacy_plugin_root(&plugins, &legacy_root, &mgr.db);

        // 目录已迁移到统一根
        assert!(plugins.join("demo").join("plugin.json").exists());
        assert!(!legacy_root.join("demo").exists());
        // DB 路径已更新
        let db = lock(&mgr.db);
        let row = db.plugin_find_by_name("demo").unwrap().unwrap();
        assert_eq!(
            row.installed_path,
            plugins.join("demo").to_string_lossy().into_owned()
        );
    }

    #[test]
    fn copy_dir_recursive_copies_tree() {
        let root = temp_root("copy-recursive");
        let src = root.join("src");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.txt"), "a").unwrap();
        std::fs::write(src.join("sub").join("b.txt"), "b").unwrap();
        let dst = root.join("dst");
        super::copy_dir_recursive(&src, &dst).unwrap();
        assert!(dst.join("a.txt").exists());
        assert!(dst.join("sub").join("b.txt").exists());
    }
}
