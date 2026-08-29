// 3 相位 journal + 原子写 + 简化启动恢复器（1.9.3）
// 对等 plugin-system/PluginTransactionJournal.ts + PluginTransactionRecovery.ts 的简化版：
// - write_journal：pending（create_new + 0o600）→ fsync → rename 原子提升；同目录已有
//   journal 必须同 identity（plugin_name+transaction_id+operation+created_at）
// - read_journal：正式名 ≤256KB；pending 存在且正式不存在时提升 pending
// - recover_interrupted：fail-closed，5 个确定性场景，歧义一律 blocked 绝不猜测删除
//
// 本模块由 1.9.3 安装链后续任务接线（当前仅测试引用），故允许 dead_code。

#![allow(dead_code)]

use crate::transaction::{
    canonicalize_plugins_dir, is_plugin_name, is_transaction_id, path_entry_exists,
    remove_internal_directory, rename_internal_directory,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const JOURNAL_FILENAME: &str = ".openbox-host-transaction.json";
pub const JOURNAL_VERSION: i64 = 1;

const JOURNAL_PENDING_FILENAME: &str = ".openbox-host-transaction.json.pending";
const MAX_JOURNAL_BYTES: u64 = 256 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Journal {
    pub version: i64,
    pub operation: String,
    pub phase: String,
    pub plugin_name: String,
    pub transaction_id: String,
    pub previous_metadata: Option<serde_json::Value>,
    pub created_at: String,
}

/// 原子写：pending（create_new + 0o600）→ fsync → rename 到正式名。
/// 写入前校验同目录已有 journal 必须同 identity，否则拒绝。
pub fn write_journal(root: &Path, j: &Journal) -> Result<(), String> {
    validate_journal(j)?;
    let path = root.join(JOURNAL_FILENAME);
    let pending = root.join(JOURNAL_PENDING_FILENAME);
    if path_entry_exists(&pending)? {
        return Err(format!(
            "Plugin transaction root contains reserved host marker: {JOURNAL_PENDING_FILENAME}"
        ));
    }
    if path_entry_exists(&path)? {
        let existing = read_journal_file(&path)?;
        if !same_identity(&existing, j) {
            return Err(
                "Plugin transaction root contains a journal from another transaction".into(),
            );
        }
    }
    let serialized =
        serde_json::to_string(j).map_err(|e| format!("journal is not serializable: {e}"))?;
    if serialized.len() as u64 > MAX_JOURNAL_BYTES {
        return Err("Plugin transaction journal is too large".into());
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&pending)
        .map_err(|e| format!("failed to create pending journal: {e}"))?;
    file.write_all(serialized.as_bytes())
        .map_err(|e| format!("failed to write pending journal: {e}"))?;
    file.sync_all()
        .map_err(|e| format!("failed to fsync pending journal: {e}"))?;
    drop(file);
    fs::rename(&pending, &path).map_err(|e| format!("failed to promote journal: {e}"))?;
    Ok(())
}

/// 读正式名 journal；pending 存在且正式不存在时提升 pending。
pub fn read_journal(root: &Path) -> Result<Option<Journal>, String> {
    let path = root.join(JOURNAL_FILENAME);
    let pending = root.join(JOURNAL_PENDING_FILENAME);
    if !path_entry_exists(&path)? {
        if path_entry_exists(&pending)? {
            fs::rename(&pending, &path)
                .map_err(|e| format!("failed to promote pending journal: {e}"))?;
        } else {
            return Ok(None);
        }
    }
    Ok(Some(read_journal_file(&path)?))
}

/// 删除正式名 + pending；正式名存在时校验 identity 一致。
pub fn clear_journal(root: &Path, j: &Journal) -> Result<(), String> {
    let path = root.join(JOURNAL_FILENAME);
    let pending = root.join(JOURNAL_PENDING_FILENAME);
    if path_entry_exists(&path)? {
        let existing = read_journal_file(&path)?;
        if !same_identity(&existing, j) {
            return Err("Refusing to clear a journal from another transaction".into());
        }
        fs::remove_file(&path).map_err(|e| format!("failed to remove journal: {e}"))?;
    }
    if path_entry_exists(&pending)? {
        fs::remove_file(&pending).map_err(|e| format!("failed to remove pending journal: {e}"))?;
    }
    Ok(())
}

#[derive(Debug, Default)]
pub struct RecoveryReport {
    pub blocked_plugins: Vec<String>,
    pub actions: Vec<String>,
}

/// 启动恢复器（fail-closed）。扫描 pluginsDir 直接子目录（target + stage/backup/remove
/// artifact），按 plugin_name 分组；有 journal 走 install/upgrade 场景，无 journal 走
/// 孤儿场景；任何歧义/错误 → blocked_plugins + actions，绝不猜测删除。
pub fn recover_interrupted(
    plugins_dir: &Path,
    find_metadata: &dyn Fn(&str) -> Option<serde_json::Value>,
    restore_metadata: &dyn Fn(&str, &serde_json::Value) -> Result<(), String>,
) -> RecoveryReport {
    let mut report = RecoveryReport::default();
    let plugins_dir = match canonicalize_plugins_dir(plugins_dir) {
        Ok(p) => p,
        Err(_) => return report, // pluginsDir 不存在 → 无插件可恢复
    };
    let mut ctx = RecoveryCtx {
        plugins_dir: plugins_dir.clone(),
        find_metadata,
        restore_metadata,
        report: &mut report,
    };
    let (targets, artifacts) = match scan_roots(&plugins_dir, &mut ctx) {
        Ok(v) => v,
        Err(e) => {
            block_plugin(&mut ctx, "*", &e);
            return report;
        }
    };
    let mut groups: HashMap<String, Vec<Artifact>> = HashMap::new();
    for artifact in artifacts {
        groups
            .entry(artifact.plugin_name.clone())
            .or_default()
            .push(artifact);
    }
    let mut plugin_names: Vec<String> = groups.keys().cloned().collect();
    for target in &targets {
        if let Some(name) = target.file_name().and_then(|n| n.to_str()) {
            if !plugin_names.iter().any(|p| p == name) {
                plugin_names.push(name.to_string());
            }
        }
    }
    plugin_names.sort();

    for plugin_name in plugin_names {
        let artifacts = groups.get(&plugin_name).cloned().unwrap_or_default();
        let target = plugins_dir.join(&plugin_name);
        let target_exists = path_entry_exists(&target).unwrap_or(false);
        let records = collect_journals(&target, target_exists, &artifacts, &mut ctx);
        if records.is_none() {
            continue; // journal 读取失败已 blocked
        }
        let records = records.unwrap();
        let mut unique: Vec<(Journal, PathBuf)> = Vec::new();
        for (j, root) in records {
            if unique.iter().any(|(ej, _)| same_identity(ej, &j)) {
                continue;
            }
            unique.push((j, root));
        }
        if unique.len() > 1 {
            block_plugin(
                &mut ctx,
                &plugin_name,
                "Multiple host journals exist for one transaction",
            );
            continue;
        }
        if let Some((journal, journal_root)) = unique.into_iter().next() {
            match journal.operation.as_str() {
                "install" => recover_install(
                    &mut ctx,
                    &plugin_name,
                    &journal,
                    &journal_root,
                    target_exists,
                    &artifacts,
                ),
                "upgrade" => recover_upgrade(
                    &mut ctx,
                    &plugin_name,
                    &journal,
                    &journal_root,
                    target_exists,
                    &artifacts,
                ),
                "uninstall" => recover_uninstall(
                    &mut ctx,
                    &plugin_name,
                    &journal,
                    &journal_root,
                    target_exists,
                    &artifacts,
                ),
                _ => block_plugin(&mut ctx, &plugin_name, "Unsupported journal operation"),
            }
        } else {
            recover_orphan(&mut ctx, &plugin_name, target_exists, &artifacts);
        }
    }
    ctx.report.blocked_plugins.sort();
    ctx.report.blocked_plugins.dedup();
    report
}

// ---------------------------------------------------------------------------
// 恢复内部实现
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArtifactKind {
    Stage,
    Backup,
    Remove,
}

#[derive(Debug, Clone)]
struct Artifact {
    kind: ArtifactKind,
    plugin_name: String,
    transaction_id: String,
    path: PathBuf,
}

struct RecoveryCtx<'a> {
    plugins_dir: PathBuf,
    find_metadata: &'a dyn Fn(&str) -> Option<serde_json::Value>,
    restore_metadata: &'a dyn Fn(&str, &serde_json::Value) -> Result<(), String>,
    report: &'a mut RecoveryReport,
}

fn block_plugin(ctx: &mut RecoveryCtx, plugin: &str, reason: &str) {
    if !ctx.report.blocked_plugins.iter().any(|p| p == plugin) {
        ctx.report.blocked_plugins.push(plugin.to_string());
    }
    ctx.report
        .actions
        .push(format!("blocked {plugin}: {reason}"));
}

fn stage_basename(name: &str, txid: &str) -> String {
    format!(".{name}.stage-{txid}")
}

fn backup_basename(name: &str, txid: &str) -> String {
    format!(".{name}.backup-{txid}")
}

fn remove_basename(name: &str, txid: &str) -> String {
    format!(".{name}.remove-{txid}")
}

/// 解析 artifact 目录名 `^\.([a-z0-9_-]+)\.(stage|backup|remove)-([a-zA-Z0-9-]+)$`。
fn parse_artifact(name: &str) -> Option<(String, ArtifactKind, String)> {
    let rest = name.strip_prefix('.')?;
    for (suffix, kind) in [
        (".stage-", ArtifactKind::Stage),
        (".backup-", ArtifactKind::Backup),
        (".remove-", ArtifactKind::Remove),
    ] {
        if let Some(index) = rest.find(suffix) {
            let plugin = &rest[..index];
            let txid = &rest[index + suffix.len()..];
            if is_plugin_name(plugin) && is_transaction_id(txid) {
                return Some((plugin.to_string(), kind, txid.to_string()));
            }
        }
    }
    None
}

fn scan_roots(
    plugins_dir: &Path,
    ctx: &mut RecoveryCtx,
) -> Result<(Vec<PathBuf>, Vec<Artifact>), String> {
    let mut targets = Vec::new();
    let mut artifacts = Vec::new();
    let entries =
        fs::read_dir(plugins_dir).map_err(|e| format!("failed to scan plugins dir: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.file_type().is_symlink() || !meta.is_dir() {
            if is_plugin_name(&name) || parse_artifact(&name).is_some() {
                block_plugin(
                    ctx,
                    &name,
                    "Plugin transaction entry is not a regular directory",
                );
            }
            continue;
        }
        if let Some((plugin, kind, txid)) = parse_artifact(&name) {
            artifacts.push(Artifact {
                kind,
                plugin_name: plugin,
                transaction_id: txid,
                path,
            });
        } else if is_plugin_name(&name) {
            targets.push(path);
        }
    }
    Ok((targets, artifacts))
}

/// 收集 target + 各 artifact 内的 journal；读取失败 → blocked 并返回 None。
fn collect_journals(
    target: &Path,
    target_exists: bool,
    artifacts: &[Artifact],
    ctx: &mut RecoveryCtx,
) -> Option<Vec<(Journal, PathBuf)>> {
    let mut records = Vec::new();
    let mut error: Option<String> = None;
    if target_exists {
        match read_journal(target) {
            Ok(Some(j)) => records.push((j, target.to_path_buf())),
            Ok(None) => {}
            Err(e) => error = Some(e),
        }
    }
    for artifact in artifacts {
        match read_journal(&artifact.path) {
            Ok(Some(j)) => records.push((j, artifact.path.clone())),
            Ok(None) => {}
            Err(e) => error = Some(e),
        }
    }
    if let Some(e) = error {
        let plugin = target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        block_plugin(ctx, &plugin, &format!("invalid journal: {e}"));
        return None;
    }
    Some(records)
}

fn recover_install(
    ctx: &mut RecoveryCtx,
    plugin_name: &str,
    journal: &Journal,
    journal_root: &Path,
    target_exists: bool,
    artifacts: &[Artifact],
) {
    let target = ctx.plugins_dir.join(plugin_name);
    let stage = artifacts.iter().find(|a| a.kind == ArtifactKind::Stage);
    let backup = artifacts.iter().find(|a| a.kind == ArtifactKind::Backup);
    let remove = artifacts.iter().find(|a| a.kind == ArtifactKind::Remove);
    let metadata = (ctx.find_metadata)(plugin_name);

    if let Some(stage) = stage {
        if target_exists || metadata.is_some() {
            block_plugin(ctx, plugin_name, "Prepared install is ambiguous");
            return;
        }
        let basename = stage_basename(plugin_name, &stage.transaction_id);
        match remove_internal_directory(&ctx.plugins_dir, &stage.path, &basename) {
            Ok(()) => {
                ctx.report
                    .actions
                    .push(format!("rollback-install {plugin_name}"));
                let _ = clear_journal(journal_root, journal);
            }
            Err(e) => block_plugin(ctx, plugin_name, &e),
        }
        return;
    }
    if backup.is_some() || remove.is_some() {
        block_plugin(ctx, plugin_name, "Install target has conflicting artifacts");
        return;
    }
    if metadata.is_some() {
        match clear_journal(journal_root, journal) {
            Ok(()) => ctx
                .report
                .actions
                .push(format!("commit-install {plugin_name}")),
            Err(e) => block_plugin(ctx, plugin_name, &e),
        }
        return;
    }
    if journal.phase == "committed" {
        block_plugin(ctx, plugin_name, "Committed install metadata is missing");
        return;
    }
    match remove_internal_directory(&ctx.plugins_dir, &target, plugin_name) {
        Ok(()) => {
            ctx.report
                .actions
                .push(format!("rollback-install {plugin_name}"));
            let _ = clear_journal(journal_root, journal);
        }
        Err(e) => block_plugin(ctx, plugin_name, &e),
    }
}

fn recover_upgrade(
    ctx: &mut RecoveryCtx,
    plugin_name: &str,
    journal: &Journal,
    journal_root: &Path,
    target_exists: bool,
    artifacts: &[Artifact],
) {
    let target = ctx.plugins_dir.join(plugin_name);
    let stage = artifacts.iter().find(|a| a.kind == ArtifactKind::Stage);
    let backup = artifacts.iter().find(|a| a.kind == ArtifactKind::Backup);
    let mut displaced_target: Option<(PathBuf, String)> = None;

    if journal.phase == "committed" {
        if let Some(b) = backup {
            let basename = backup_basename(plugin_name, &b.transaction_id);
            if let Err(e) = remove_internal_directory(&ctx.plugins_dir, &b.path, &basename) {
                block_plugin(ctx, plugin_name, &e);
                return;
            }
        }
        if let Some(s) = stage {
            let basename = stage_basename(plugin_name, &s.transaction_id);
            if let Err(e) = remove_internal_directory(&ctx.plugins_dir, &s.path, &basename) {
                block_plugin(ctx, plugin_name, &e);
                return;
            }
        }
        match clear_journal(journal_root, journal) {
            Ok(()) => {
                ctx.report
                    .actions
                    .push(format!("cleanup-committed-upgrade {plugin_name}"));
            }
            Err(e) => block_plugin(ctx, plugin_name, &e),
        }
        return;
    }

    // 未提交升级：回滚 backup + 旧元数据 + 移除候选
    if let Some(b) = backup {
        if target_exists {
            if stage.is_some() {
                block_plugin(ctx, plugin_name, "Upgrade staging path already exists");
                return;
            }
            let stage_path = ctx
                .plugins_dir
                .join(stage_basename(plugin_name, &journal.transaction_id));
            let stage_name = stage_basename(plugin_name, &journal.transaction_id);
            if let Err(e) = rename_internal_directory(
                &ctx.plugins_dir,
                &target,
                plugin_name,
                &stage_path,
                &stage_name,
            ) {
                block_plugin(ctx, plugin_name, &e);
                return;
            }
            displaced_target = Some((stage_path, stage_name));
        }
        let backup_name = backup_basename(plugin_name, &b.transaction_id);
        if let Err(e) = rename_internal_directory(
            &ctx.plugins_dir,
            &b.path,
            &backup_name,
            &target,
            plugin_name,
        ) {
            block_plugin(ctx, plugin_name, &e);
            return;
        }
    } else {
        // 无 backup：仅当 journal 在 stage 且 target 存在（swap 前崩溃）才可安全回滚
        let journal_in_stage = artifacts
            .iter()
            .any(|a| a.kind == ArtifactKind::Stage && a.path == *journal_root);
        if !journal_in_stage || !target_exists {
            block_plugin(
                ctx,
                plugin_name,
                "Cannot roll back an upgrade without its backup",
            );
            return;
        }
    }

    let metadata = (ctx.find_metadata)(plugin_name);
    if let Some(prev) = &journal.previous_metadata {
        if metadata.as_ref() != Some(prev) {
            if let Err(e) = (ctx.restore_metadata)(plugin_name, prev) {
                block_plugin(ctx, plugin_name, &e);
                return;
            }
        }
    }
    if let Some(s) = stage {
        let basename = stage_basename(plugin_name, &s.transaction_id);
        if let Err(e) = remove_internal_directory(&ctx.plugins_dir, &s.path, &basename) {
            block_plugin(ctx, plugin_name, &e);
            return;
        }
    } else if let Some((path, basename)) = displaced_target {
        // target 中的候选版本刚在本次恢复中移到 stage_path；它不在启动扫描得到的
        // artifacts 集合里，必须本轮删除，避免需要第二次重启才能清理。
        if let Err(e) = remove_internal_directory(&ctx.plugins_dir, &path, &basename) {
            block_plugin(ctx, plugin_name, &e);
            return;
        }
    }
    match clear_journal(journal_root, journal) {
        Ok(()) => ctx
            .report
            .actions
            .push(format!("rollback-upgrade {plugin_name}")),
        Err(e) => block_plugin(ctx, plugin_name, &e),
    }
}

fn recover_orphan(
    ctx: &mut RecoveryCtx,
    plugin_name: &str,
    target_exists: bool,
    artifacts: &[Artifact],
) {
    let target = ctx.plugins_dir.join(plugin_name);
    let remove = artifacts.iter().find(|a| a.kind == ArtifactKind::Remove);
    let backup = artifacts.iter().find(|a| a.kind == ArtifactKind::Backup);
    let stage = artifacts.iter().find(|a| a.kind == ArtifactKind::Stage);

    if let Some(r) = remove {
        let metadata = (ctx.find_metadata)(plugin_name);
        if metadata.is_some() {
            if target_exists {
                block_plugin(ctx, plugin_name, "Target and quarantine both exist");
                return;
            }
            let basename = remove_basename(plugin_name, &r.transaction_id);
            match rename_internal_directory(
                &ctx.plugins_dir,
                &r.path,
                &basename,
                &target,
                plugin_name,
            ) {
                Ok(()) => {
                    ctx.report
                        .actions
                        .push(format!("restore-orphan-uninstall {plugin_name}"));
                }
                Err(e) => block_plugin(ctx, plugin_name, &e),
            }
        } else {
            let basename = remove_basename(plugin_name, &r.transaction_id);
            match remove_internal_directory(&ctx.plugins_dir, &r.path, &basename) {
                Ok(()) => {
                    ctx.report
                        .actions
                        .push(format!("commit-orphan-uninstall {plugin_name}"));
                }
                Err(e) => block_plugin(ctx, plugin_name, &e),
            }
        }
        return;
    }
    if let Some(b) = backup {
        if target_exists {
            block_plugin(ctx, plugin_name, "Orphan backup retained");
            return;
        }
        let basename = backup_basename(plugin_name, &b.transaction_id);
        match rename_internal_directory(&ctx.plugins_dir, &b.path, &basename, &target, plugin_name)
        {
            Ok(()) => {
                ctx.report
                    .actions
                    .push(format!("restore-orphan-backup {plugin_name}"));
            }
            Err(e) => block_plugin(ctx, plugin_name, &e),
        }
        return;
    }
    if let Some(s) = stage {
        let basename = stage_basename(plugin_name, &s.transaction_id);
        match remove_internal_directory(&ctx.plugins_dir, &s.path, &basename) {
            Ok(()) => {
                ctx.report
                    .actions
                    .push(format!("remove-orphan-stage {plugin_name}"));
            }
            Err(e) => block_plugin(ctx, plugin_name, &e),
        }
    }
}

/// 恢复卸载：DB 仍有记录时把 quarantine 目录还原；DB 已删除时完成清理。
fn recover_uninstall(
    ctx: &mut RecoveryCtx,
    plugin_name: &str,
    journal: &Journal,
    journal_root: &Path,
    target_exists: bool,
    artifacts: &[Artifact],
) {
    let target = ctx.plugins_dir.join(plugin_name);
    let remove = artifacts.iter().find(|a| a.kind == ArtifactKind::Remove);
    let metadata = (ctx.find_metadata)(plugin_name);
    if metadata.is_some() {
        if target_exists && remove.is_some() {
            block_plugin(
                ctx,
                plugin_name,
                "Uninstall target and quarantine both exist",
            );
            return;
        }
        if let Some(r) = remove {
            let basename = remove_basename(plugin_name, &r.transaction_id);
            if let Err(e) = rename_internal_directory(
                &ctx.plugins_dir,
                &r.path,
                &basename,
                &target,
                plugin_name,
            ) {
                block_plugin(ctx, plugin_name, &e);
                return;
            }
        }
        match clear_journal(
            if remove.is_some() {
                &target
            } else {
                journal_root
            },
            journal,
        ) {
            Ok(()) => ctx
                .report
                .actions
                .push(format!("rollback-uninstall {plugin_name}")),
            Err(e) => block_plugin(ctx, plugin_name, &e),
        }
        return;
    }

    // DB 已不存在：无论崩溃发生在 quarantine 前后，都只删除受保护的直接子目录。
    if target_exists {
        if let Err(e) = remove_internal_directory(&ctx.plugins_dir, &target, plugin_name) {
            block_plugin(ctx, plugin_name, &e);
            return;
        }
    }
    if let Some(r) = remove {
        let basename = remove_basename(plugin_name, &r.transaction_id);
        if let Err(e) = remove_internal_directory(&ctx.plugins_dir, &r.path, &basename) {
            block_plugin(ctx, plugin_name, &e);
            return;
        }
    }
    match clear_journal(journal_root, journal) {
        Ok(()) => ctx
            .report
            .actions
            .push(format!("commit-uninstall {plugin_name}")),
        Err(e) => block_plugin(ctx, plugin_name, &e),
    }
}

// ---------------------------------------------------------------------------
// journal 校验与读写内部实现
// ---------------------------------------------------------------------------

fn validate_journal(j: &Journal) -> Result<(), String> {
    if j.version != JOURNAL_VERSION {
        return Err(format!(
            "Unsupported plugin transaction journal version: {}",
            j.version
        ));
    }
    if j.operation != "install" && j.operation != "upgrade" && j.operation != "uninstall" {
        return Err("Unsupported plugin transaction operation".into());
    }
    if j.phase != "prepared" && j.phase != "applied" && j.phase != "committed" {
        return Err("Unsupported plugin transaction phase".into());
    }
    if !is_plugin_name(&j.plugin_name) {
        return Err("Journal pluginName contains unsupported characters".into());
    }
    if !is_transaction_id(&j.transaction_id) {
        return Err("Journal transactionId contains unsupported characters".into());
    }
    if j.created_at.is_empty() {
        return Err("Journal createdAt must be a non-empty string".into());
    }
    Ok(())
}

fn same_identity(a: &Journal, b: &Journal) -> bool {
    a.plugin_name == b.plugin_name
        && a.transaction_id == b.transaction_id
        && a.operation == b.operation
        && a.created_at == b.created_at
}

fn read_journal_file(path: &Path) -> Result<Journal, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(format!(
            "Plugin transaction journal must be a regular file: {}",
            path.display()
        ));
    }
    if meta.len() > MAX_JOURNAL_BYTES {
        return Err("Plugin transaction journal is too large".into());
    }
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let journal: Journal =
        serde_json::from_str(&raw).map_err(|e| format!("invalid journal JSON: {e}"))?;
    validate_journal(&journal)?;
    Ok(journal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use serde_json::Value;
    use std::cell::RefCell;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cruciblebox-journal-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn journal(
        operation: &str,
        phase: &str,
        name: &str,
        txid: &str,
        prev: Option<Value>,
    ) -> Journal {
        Journal {
            version: JOURNAL_VERSION,
            operation: operation.into(),
            phase: phase.into(),
            plugin_name: name.into(),
            transaction_id: txid.into(),
            previous_metadata: prev,
            created_at: "2026-08-16T00:00:00.000Z".into(),
        }
    }

    fn write_tree(root: &Path, files: &[(&str, &str)]) {
        for (rel, content) in files {
            let path = root.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, content).unwrap();
        }
    }

    fn recovery_env(name: &str) -> (PathBuf, RefCell<HashMap<String, Value>>) {
        let root = temp_root(name);
        let plugins = root.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        (plugins, RefCell::new(HashMap::new()))
    }

    fn run_recovery(plugins: &Path, metadata: &RefCell<HashMap<String, Value>>) -> RecoveryReport {
        let find = |name: &str| metadata.borrow().get(name).cloned();
        let restore = |name: &str, value: &Value| {
            metadata
                .borrow_mut()
                .insert(name.to_string(), value.clone());
            Ok(())
        };
        recover_interrupted(plugins, &find, &restore)
    }

    #[test]
    fn write_read_roundtrip() {
        let root = temp_root("roundtrip");
        fs::create_dir_all(&root).unwrap();
        let j = journal("install", "prepared", "demo", "tx-1", None);
        write_journal(&root, &j).unwrap();
        let read = read_journal(&root).unwrap().unwrap();
        assert_eq!(read.plugin_name, "demo");
        assert_eq!(read.phase, "prepared");
        assert_eq!(read.operation, "install");
        assert_eq!(read.transaction_id, "tx-1");
        assert!(read.previous_metadata.is_none());
        assert!(!root.join(JOURNAL_PENDING_FILENAME).exists());
    }

    #[test]
    fn write_rejects_identity_mismatch() {
        let root = temp_root("identity-mismatch");
        fs::create_dir_all(&root).unwrap();
        let a = journal("install", "prepared", "demo", "tx-1", None);
        write_journal(&root, &a).unwrap();
        let b = journal("install", "prepared", "demo", "tx-2", None);
        let err = write_journal(&root, &b).unwrap_err();
        assert!(err.contains("another transaction"));
        // 原 journal 未被破坏
        assert_eq!(read_journal(&root).unwrap().unwrap().transaction_id, "tx-1");
    }

    #[test]
    fn write_same_identity_phase_transition_ok() {
        let root = temp_root("phase-transition");
        fs::create_dir_all(&root).unwrap();
        let a = journal("install", "prepared", "demo", "tx-1", None);
        write_journal(&root, &a).unwrap();
        let b = journal("install", "applied", "demo", "tx-1", None);
        write_journal(&root, &b).unwrap();
        assert_eq!(read_journal(&root).unwrap().unwrap().phase, "applied");
    }

    #[test]
    fn read_promotes_pending() {
        let root = temp_root("pending-promote");
        fs::create_dir_all(&root).unwrap();
        let j = journal(
            "upgrade",
            "applied",
            "demo",
            "tx-1",
            Some(json!({"id": "p1"})),
        );
        let pending = root.join(JOURNAL_PENDING_FILENAME);
        fs::write(&pending, serde_json::to_string(&j).unwrap()).unwrap();
        let read = read_journal(&root).unwrap().unwrap();
        assert_eq!(read.phase, "applied");
        assert!(!pending.exists());
        assert!(root.join(JOURNAL_FILENAME).exists());
    }

    #[test]
    fn clear_journal_removes_files() {
        let root = temp_root("clear");
        fs::create_dir_all(&root).unwrap();
        let j = journal("install", "prepared", "demo", "tx-1", None);
        write_journal(&root, &j).unwrap();
        clear_journal(&root, &j).unwrap();
        assert!(read_journal(&root).unwrap().is_none());
    }

    #[test]
    fn clear_journal_rejects_identity_mismatch() {
        let root = temp_root("clear-identity");
        fs::create_dir_all(&root).unwrap();
        let a = journal("install", "prepared", "demo", "tx-1", None);
        write_journal(&root, &a).unwrap();
        let b = journal("install", "prepared", "demo", "tx-2", None);
        let err = clear_journal(&root, &b).unwrap_err();
        assert!(err.contains("another transaction"));
        assert!(read_journal(&root).unwrap().is_some());
    }

    #[test]
    fn recover_install_stage_rollback() {
        let (plugins, metadata) = recovery_env("recover-install-stage");
        let stage = plugins.join(".demo.stage-tx-1");
        fs::create_dir_all(&stage).unwrap();
        fs::write(stage.join("plugin.json"), "new").unwrap();
        write_journal(
            &stage,
            &journal("install", "prepared", "demo", "tx-1", None),
        )
        .unwrap();

        let report = run_recovery(&plugins, &metadata);
        assert!(report.blocked_plugins.is_empty());
        assert!(report
            .actions
            .iter()
            .any(|a| a.contains("rollback-install demo")));
        assert!(!stage.exists());
    }

    #[test]
    fn recover_install_target_commit() {
        let (plugins, metadata) = recovery_env("recover-install-commit");
        let target = plugins.join("demo");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("plugin.json"), "new").unwrap();
        write_journal(
            &target,
            &journal("install", "applied", "demo", "tx-1", None),
        )
        .unwrap();
        metadata
            .borrow_mut()
            .insert("demo".into(), json!({"id": "p1"}));

        let report = run_recovery(&plugins, &metadata);
        assert!(report.blocked_plugins.is_empty());
        assert!(report
            .actions
            .iter()
            .any(|a| a.contains("commit-install demo")));
        assert!(target.exists());
        assert!(read_journal(&target).unwrap().is_none());
    }

    #[test]
    fn recover_install_target_rollback() {
        let (plugins, metadata) = recovery_env("recover-install-rollback");
        let target = plugins.join("demo");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("plugin.json"), "new").unwrap();
        write_journal(
            &target,
            &journal("install", "prepared", "demo", "tx-1", None),
        )
        .unwrap();

        let report = run_recovery(&plugins, &metadata);
        assert!(report.blocked_plugins.is_empty());
        assert!(report
            .actions
            .iter()
            .any(|a| a.contains("rollback-install demo")));
        assert!(!target.exists());
    }

    #[test]
    fn recover_upgrade_committed_cleanup() {
        let (plugins, metadata) = recovery_env("recover-upgrade-committed");
        let target = plugins.join("demo");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("plugin.json"), "new").unwrap();
        write_journal(
            &target,
            &journal(
                "upgrade",
                "committed",
                "demo",
                "tx-1",
                Some(json!({"id": "p1"})),
            ),
        )
        .unwrap();
        metadata
            .borrow_mut()
            .insert("demo".into(), json!({"id": "p1"}));
        let backup = plugins.join(".demo.backup-tx-1");
        fs::create_dir_all(&backup).unwrap();
        fs::write(backup.join("plugin.json"), "old").unwrap();

        let report = run_recovery(&plugins, &metadata);
        assert!(report.blocked_plugins.is_empty());
        assert!(report
            .actions
            .iter()
            .any(|a| a.contains("cleanup-committed-upgrade demo")));
        assert!(!backup.exists());
        assert!(target.exists());
        assert!(read_journal(&target).unwrap().is_none());
    }

    #[test]
    fn recover_upgrade_rollback_restores_backup_and_metadata() {
        let (plugins, metadata) = recovery_env("recover-upgrade-rollback");
        let target = plugins.join("demo");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("plugin.json"), "new").unwrap();
        write_journal(
            &target,
            &journal(
                "upgrade",
                "applied",
                "demo",
                "tx-1",
                Some(json!({"id": "p1", "version": "1.0.0"})),
            ),
        )
        .unwrap();
        let backup = plugins.join(".demo.backup-tx-1");
        fs::create_dir_all(&backup).unwrap();
        fs::write(backup.join("plugin.json"), "old").unwrap();
        metadata
            .borrow_mut()
            .insert("demo".into(), json!({"id": "p1", "version": "2.0.0"}));

        let report = run_recovery(&plugins, &metadata);
        assert!(report.blocked_plugins.is_empty());
        assert!(report
            .actions
            .iter()
            .any(|a| a.contains("rollback-upgrade demo")));
        assert_eq!(
            fs::read_to_string(target.join("plugin.json")).unwrap(),
            "old"
        );
        assert!(!backup.exists());
        assert!(!plugins.join(".demo.stage-tx-1").exists());
        assert_eq!(metadata.borrow().get("demo").unwrap()["version"], "1.0.0");
    }

    #[test]
    fn recover_orphan_stage_removed() {
        let (plugins, metadata) = recovery_env("recover-orphan-stage");
        let stage = plugins.join(".demo.stage-tx-1");
        fs::create_dir_all(&stage).unwrap();
        fs::write(stage.join("plugin.json"), "new").unwrap();

        let report = run_recovery(&plugins, &metadata);
        assert!(report.blocked_plugins.is_empty());
        assert!(report
            .actions
            .iter()
            .any(|a| a.contains("remove-orphan-stage demo")));
        assert!(!stage.exists());
    }

    #[test]
    fn recover_orphan_backup_restored() {
        let (plugins, metadata) = recovery_env("recover-orphan-backup");
        let backup = plugins.join(".demo.backup-tx-1");
        fs::create_dir_all(&backup).unwrap();
        fs::write(backup.join("plugin.json"), "old").unwrap();

        let report = run_recovery(&plugins, &metadata);
        assert!(report.blocked_plugins.is_empty());
        assert!(report
            .actions
            .iter()
            .any(|a| a.contains("restore-orphan-backup demo")));
        assert!(plugins.join("demo").join("plugin.json").exists());
        assert!(!backup.exists());
    }

    #[test]
    fn recover_orphan_remove_restored_with_metadata() {
        let (plugins, metadata) = recovery_env("recover-orphan-remove-restore");
        let quarantine = plugins.join(".demo.remove-tx-1");
        fs::create_dir_all(&quarantine).unwrap();
        fs::write(quarantine.join("plugin.json"), "old").unwrap();
        metadata
            .borrow_mut()
            .insert("demo".into(), json!({"id": "p1"}));

        let report = run_recovery(&plugins, &metadata);
        assert!(report.blocked_plugins.is_empty());
        assert!(report
            .actions
            .iter()
            .any(|a| a.contains("restore-orphan-uninstall demo")));
        assert!(plugins.join("demo").join("plugin.json").exists());
        assert!(!quarantine.exists());
    }

    #[test]
    fn recover_orphan_remove_committed_without_metadata() {
        let (plugins, metadata) = recovery_env("recover-orphan-remove-commit");
        let quarantine = plugins.join(".demo.remove-tx-1");
        fs::create_dir_all(&quarantine).unwrap();
        fs::write(quarantine.join("plugin.json"), "old").unwrap();

        let report = run_recovery(&plugins, &metadata);
        assert!(report.blocked_plugins.is_empty());
        assert!(report
            .actions
            .iter()
            .any(|a| a.contains("commit-orphan-uninstall demo")));
        assert!(!quarantine.exists());
        assert!(!plugins.join("demo").exists());
    }

    #[test]
    fn recover_ambiguous_install_blocks() {
        let (plugins, metadata) = recovery_env("recover-ambiguous");
        let stage = plugins.join(".demo.stage-tx-1");
        fs::create_dir_all(&stage).unwrap();
        write_journal(
            &stage,
            &journal("install", "prepared", "demo", "tx-1", None),
        )
        .unwrap();
        fs::create_dir_all(plugins.join("demo")).unwrap();

        let report = run_recovery(&plugins, &metadata);
        assert!(report.blocked_plugins.contains(&"demo".to_string()));
        assert!(stage.exists());
        assert!(plugins.join("demo").exists());
    }

    #[test]
    fn recover_orphan_backup_with_target_blocks() {
        let (plugins, metadata) = recovery_env("recover-orphan-backup-conflict");
        let backup = plugins.join(".demo.backup-tx-1");
        fs::create_dir_all(&backup).unwrap();
        fs::write(backup.join("plugin.json"), "old").unwrap();
        write_tree(&plugins.join("demo"), &[("plugin.json", "current")]);

        let report = run_recovery(&plugins, &metadata);
        assert!(report.blocked_plugins.contains(&"demo".to_string()));
        assert!(backup.exists());
        assert!(plugins.join("demo").exists());
    }
}
