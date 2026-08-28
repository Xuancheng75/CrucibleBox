// 目录事务（1.9.3，对等 plugin-system/PluginDirectoryTransaction.ts）
// 语义：
// - target/stage/backup 必须是 pluginsDir 的直接子目录；删除/rename 前过 assert_direct_child
// - stage 预算拷贝（5000 条目 / 600MB），拒绝 symlink，文件复制前后 size 比对
// - swap 用同卷 rename（Windows 语义：目标必须不存在）；失败恢复 backup→target
// - commit 先标记相位再 best-effort 清理 backup（清理失败不误报）
// - rollback 相位感知：Created/Staged/Swapped 可回滚，Committed 拒绝
//
// 本模块由 1.9.3 安装链后续任务接线（当前仅测试引用），故允许 dead_code。

#![allow(dead_code)]

use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const DEFAULT_MAX_ENTRIES: u64 = 5_000;
pub const DEFAULT_MAX_TOTAL_BYTES: u64 = 600 * 1024 * 1024;

/// 事务相位（对等 PluginDirectoryTransactionPhase）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Created,
    Staged,
    Swapped,
    Committed,
    RolledBack,
}

pub struct TransactionOptions {
    pub plugins_dir: PathBuf,
    pub plugin_name: String,
    /// 调用方传入（通常 rand_token::random_token_hex()），须匹配 `[a-zA-Z0-9-]+`
    pub transaction_id: String,
    pub source_dir: PathBuf,
    /// 升级=true，全新安装=false
    pub expected_target_exists: bool,
    /// trusted:unienv 白名单（Some 时仅复制白名单文件）
    pub allowed_files: Option<Vec<String>>,
}

pub struct DirectoryTransaction {
    plugins_dir: PathBuf,
    plugin_name: String,
    transaction_id: String,
    source_dir: PathBuf,
    stage_dir: PathBuf,
    backup_dir: PathBuf,
    target_dir: PathBuf,
    allowed_files: Option<Vec<String>>,
    expected_target_exists: bool,
    phase: Phase,
    original_target_existed: bool,
}

impl DirectoryTransaction {
    pub fn new(opts: TransactionOptions) -> Result<Self, String> {
        if !is_plugin_name(&opts.plugin_name) {
            return Err("pluginName contains unsupported characters".into());
        }
        if !is_transaction_id(&opts.transaction_id) {
            return Err("transactionId contains unsupported characters".into());
        }
        let plugins_dir = canonicalize_plugins_dir(&opts.plugins_dir)?;
        assert_plain_directory(&opts.source_dir, "Plugin source")?;
        let source_dir = fs::canonicalize(&opts.source_dir)
            .map_err(|e| format!("failed to canonicalize source dir: {e}"))?;
        assert_plain_directory(&source_dir, "Plugin source")?;
        if source_dir == plugins_dir {
            return Err("Plugin source cannot be the plugins directory".into());
        }
        let stage_basename = format!(".{}.stage-{}", opts.plugin_name, opts.transaction_id);
        let backup_basename = format!(".{}.backup-{}", opts.plugin_name, opts.transaction_id);
        let stage_dir = plugins_dir.join(&stage_basename);
        let backup_dir = plugins_dir.join(&backup_basename);
        let target_dir = plugins_dir.join(&opts.plugin_name);
        assert_direct_child(&plugins_dir, &stage_dir, &stage_basename)?;
        assert_direct_child(&plugins_dir, &backup_dir, &backup_basename)?;
        assert_direct_child(&plugins_dir, &target_dir, &opts.plugin_name)?;
        let allowed_files = match &opts.allowed_files {
            Some(files) => Some(normalize_allowed_files(files)?),
            None => None,
        };
        Ok(DirectoryTransaction {
            plugins_dir,
            plugin_name: opts.plugin_name,
            transaction_id: opts.transaction_id,
            source_dir,
            stage_dir,
            backup_dir,
            target_dir,
            allowed_files,
            expected_target_exists: opts.expected_target_exists,
            phase: Phase::Created,
            original_target_existed: false,
        })
    }

    /// 预算拷贝源树到 stageDir；失败删除 stageDir 并保持 Created 相位。
    pub fn stage(&mut self) -> Result<(), String> {
        if self.phase != Phase::Created {
            return Err(format!(
                "Cannot stage a transaction in phase {:?}",
                self.phase
            ));
        }
        if path_entry_exists(&self.stage_dir)? || path_entry_exists(&self.backup_dir)? {
            return Err("Plugin transaction path already exists".into());
        }
        self.original_target_existed = path_entry_exists(&self.target_dir)?;
        if self.original_target_existed != self.expected_target_exists {
            return Err(format!(
                "Installed plugin directory was expected to be {}",
                if self.expected_target_exists {
                    "present"
                } else {
                    "absent"
                }
            ));
        }
        if self.original_target_existed {
            assert_plain_directory(&self.target_dir, "Installed plugin")?;
        }
        fs::create_dir(&self.stage_dir).map_err(|e| {
            format!(
                "failed to create stage dir {}: {e}",
                self.stage_dir.display()
            )
        })?;
        if let Err(e) = self.copy_source_to_stage() {
            let _ = remove_internal_directory(
                &self.plugins_dir,
                &self.stage_dir,
                &self.stage_basename(),
            );
            return Err(e);
        }
        self.phase = Phase::Staged;
        Ok(())
    }

    /// target→backup（若原 target 存在），stage→target；失败恢复 backup→target。
    pub fn swap(&mut self) -> Result<(), String> {
        if self.phase != Phase::Staged {
            return Err(format!(
                "Cannot swap a transaction in phase {:?}",
                self.phase
            ));
        }
        if path_entry_exists(&self.target_dir)? != self.original_target_existed {
            return Err("Installed plugin directory changed during staging".into());
        }
        assert_plain_directory(&self.stage_dir, "Staged plugin")?;
        if self.original_target_existed {
            fs::rename(&self.target_dir, &self.backup_dir)
                .map_err(|e| format!("failed to move target to backup: {e}"))?;
        }
        match fs::rename(&self.stage_dir, &self.target_dir) {
            Ok(()) => {
                self.phase = Phase::Swapped;
                Ok(())
            }
            Err(e) => {
                if self.original_target_existed && path_entry_exists(&self.backup_dir)? {
                    let _ = fs::rename(&self.backup_dir, &self.target_dir);
                }
                Err(format!("failed to move stage to target: {e}"))
            }
        }
    }

    /// 先标记 committed 再 best-effort 删除 backup（清理失败不报错、不回滚）。
    pub fn commit(&mut self) -> Result<(), String> {
        if self.phase != Phase::Swapped {
            return Err(format!(
                "Cannot commit a transaction in phase {:?}",
                self.phase
            ));
        }
        assert_plain_directory(&self.target_dir, "Installed plugin")?;
        self.phase = Phase::Committed;
        if self.original_target_existed {
            let _ = remove_internal_directory(
                &self.plugins_dir,
                &self.backup_dir,
                &self.backup_basename(),
            );
        }
        Ok(())
    }

    /// 相位感知回滚：Created 直接标记；Staged 恢复 backup（若存在）并删 stage；
    /// Swapped 先 target→stage 再恢复 backup→target 并删 stage；Committed 拒绝。
    pub fn rollback(&mut self) -> Result<(), String> {
        if self.phase == Phase::RolledBack {
            return Ok(());
        }
        if self.phase == Phase::Committed {
            return Err("Cannot roll back a committed plugin transaction".into());
        }
        if self.phase == Phase::Created {
            self.phase = Phase::RolledBack;
            return Ok(());
        }
        if self.phase == Phase::Staged {
            if path_entry_exists(&self.backup_dir)? {
                if path_entry_exists(&self.target_dir)? {
                    return Err(
                        "Cannot restore plugin backup because the target already exists".into(),
                    );
                }
                fs::rename(&self.backup_dir, &self.target_dir)
                    .map_err(|e| format!("failed to restore backup: {e}"))?;
            }
            self.phase = Phase::RolledBack;
            remove_internal_directory(&self.plugins_dir, &self.stage_dir, &self.stage_basename())?;
            return Ok(());
        }
        // Swapped
        if path_entry_exists(&self.stage_dir)? {
            return Err("Cannot roll back because the staging path unexpectedly exists".into());
        }
        if path_entry_exists(&self.target_dir)? {
            fs::rename(&self.target_dir, &self.stage_dir)
                .map_err(|e| format!("failed to move target back to stage: {e}"))?;
        }
        let restore = (|| -> Result<(), String> {
            if self.original_target_existed {
                if !path_entry_exists(&self.backup_dir)? {
                    return Err("Plugin backup is missing during rollback".into());
                }
                fs::rename(&self.backup_dir, &self.target_dir)
                    .map_err(|e| format!("failed to restore backup: {e}"))?;
            }
            Ok(())
        })();
        if let Err(e) = restore {
            if !path_entry_exists(&self.target_dir)? && path_entry_exists(&self.stage_dir)? {
                let _ = fs::rename(&self.stage_dir, &self.target_dir);
            }
            return Err(e);
        }
        self.phase = Phase::RolledBack;
        remove_internal_directory(&self.plugins_dir, &self.stage_dir, &self.stage_basename())?;
        Ok(())
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn stage_dir(&self) -> &Path {
        &self.stage_dir
    }

    /// 供 1.9.3 安装链（journal 写入/恢复）使用
    #[allow(dead_code)]
    pub fn target_dir(&self) -> &Path {
        &self.target_dir
    }

    fn stage_basename(&self) -> String {
        format!(".{}.stage-{}", self.plugin_name, self.transaction_id)
    }

    fn backup_basename(&self) -> String {
        format!(".{}.backup-{}", self.plugin_name, self.transaction_id)
    }

    fn copy_source_to_stage(&self) -> Result<(), String> {
        let mut budget = CopyBudget {
            entries: 0,
            max_entries: DEFAULT_MAX_ENTRIES,
            total_bytes: 0,
            max_total_bytes: DEFAULT_MAX_TOTAL_BYTES,
        };
        match &self.allowed_files {
            Some(allowed) => {
                copy_allowed_files(&self.source_dir, &self.stage_dir, allowed, &mut budget)
            }
            None => copy_directory_contents(&self.source_dir, &self.stage_dir, &mut budget),
        }
    }
}

/// 插件卸载事务：先将目标目录原子隔离到 remove artifact，再删除隔离目录。
/// DB 删除发生在 quarantine 成功之后，因此崩溃恢复可依据 journal 判断回滚或提交。
pub struct RemovalTransaction {
    plugins_dir: PathBuf,
    plugin_name: String,
    transaction_id: String,
    target_dir: PathBuf,
    quarantine_dir: PathBuf,
    quarantined: bool,
}

impl RemovalTransaction {
    pub fn new(
        plugins_dir: PathBuf,
        plugin_name: String,
        transaction_id: String,
    ) -> Result<Self, String> {
        if !is_plugin_name(&plugin_name) {
            return Err("pluginName contains unsupported characters".into());
        }
        if !is_transaction_id(&transaction_id) {
            return Err("transactionId contains unsupported characters".into());
        }
        let plugins_dir = canonicalize_plugins_dir(&plugins_dir)?;
        let target_dir = plugins_dir.join(&plugin_name);
        let quarantine_name = format!(".{}.remove-{}", plugin_name, transaction_id);
        let quarantine_dir = plugins_dir.join(&quarantine_name);
        assert_direct_child(&plugins_dir, &target_dir, &plugin_name)?;
        assert_direct_child(&plugins_dir, &quarantine_dir, &quarantine_name)?;
        if path_entry_exists(&target_dir)? {
            assert_plain_directory(&target_dir, "Installed plugin")?;
        }
        if path_entry_exists(&quarantine_dir)? {
            return Err("Plugin removal quarantine path already exists".into());
        }
        Ok(Self {
            plugins_dir,
            plugin_name,
            transaction_id,
            target_dir,
            quarantine_dir,
            quarantined: false,
        })
    }

    pub fn quarantine(&mut self) -> Result<(), String> {
        if self.quarantined {
            return Err("Plugin removal is already quarantined".into());
        }
        if !path_entry_exists(&self.target_dir)? {
            return Err("Installed plugin directory is missing".into());
        }
        assert_plain_directory(&self.target_dir, "Installed plugin")?;
        fs::rename(&self.target_dir, &self.quarantine_dir).map_err(|e| {
            format!(
                "failed to quarantine plugin {}: {e}",
                self.target_dir.display()
            )
        })?;
        self.quarantined = true;
        Ok(())
    }

    pub fn rollback(&mut self) -> Result<(), String> {
        if !self.quarantined {
            return Ok(());
        }
        if path_entry_exists(&self.target_dir)? {
            return Err("Cannot restore removed plugin because target already exists".into());
        }
        if path_entry_exists(&self.quarantine_dir)? {
            rename_internal_directory(
                &self.plugins_dir,
                &self.quarantine_dir,
                &self.quarantine_basename(),
                &self.target_dir,
                &self.plugin_name,
            )?;
        }
        self.quarantined = false;
        Ok(())
    }

    pub fn commit(&mut self) -> Result<(), String> {
        if !self.quarantined {
            return Err("Cannot commit a removal before quarantine".into());
        }
        remove_internal_directory(
            &self.plugins_dir,
            &self.quarantine_dir,
            &self.quarantine_basename(),
        )?;
        self.quarantined = false;
        Ok(())
    }

    pub fn target_dir(&self) -> &Path {
        &self.target_dir
    }

    pub fn quarantine_dir(&self) -> &Path {
        &self.quarantine_dir
    }

    fn quarantine_basename(&self) -> String {
        format!(".{}.remove-{}", self.plugin_name, self.transaction_id)
    }
}

/// 校验 child 是 pluginsDir 的直接子目录且 basename 精确匹配 expected；
/// 若 child 已存在则 lstat 拒绝 symlink。删除/rename 前必须过此断言。
pub fn assert_direct_child(plugins_dir: &Path, child: &Path, expected: &str) -> Result<(), String> {
    let canonical_parent = canonicalize_plugins_dir(plugins_dir)?;
    let child_parent = child
        .parent()
        .ok_or_else(|| format!("path has no parent: {}", child.display()))?;
    let canonical_child_parent = fs::canonicalize(child_parent)
        .map_err(|e| format!("failed to canonicalize {}: {e}", child_parent.display()))?;
    if canonical_child_parent != canonical_parent {
        return Err(format!(
            "Transaction path escaped the plugin directory: {}",
            child.display()
        ));
    }
    let basename = child
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("invalid basename: {}", child.display()))?;
    if basename != expected {
        return Err(format!(
            "Transaction path escaped the plugin directory: {}",
            child.display()
        ));
    }
    if let Ok(meta) = fs::symlink_metadata(child) {
        if meta.file_type().is_symlink() {
            return Err(format!(
                "Transaction path is a symbolic link: {}",
                child.display()
            ));
        }
    }
    Ok(())
}

/// pluginsDir 必须是普通目录（非 symlink），返回 canonicalize 结果。
pub fn canonicalize_plugins_dir(p: &Path) -> Result<PathBuf, String> {
    let meta = fs::symlink_metadata(p)
        .map_err(|e| format!("pluginsDir is not accessible: {} ({e})", p.display()))?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(format!(
            "pluginsDir must be a regular directory: {}",
            p.display()
        ));
    }
    fs::canonicalize(p)
        .map_err(|e| format!("failed to canonicalize pluginsDir {}: {e}", p.display()))
}

/// permissions 含宿主固定可信服务权限 → 返回 pinned runtime 白名单，否则 None。
pub fn trusted_allowlist(permissions: &[String]) -> Option<Vec<String>> {
    if permissions
        .iter()
        .any(|p| matches!(p.as_str(), "trusted:unienv" | "trusted:document-engine"))
    {
        Some(vec![
            "dist/main.js".to_string(),
            "dist/renderer.js".to_string(),
            "plugin.json".to_string(),
        ])
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// 内部工具（journal.rs 复用）
// ---------------------------------------------------------------------------

pub(crate) fn is_plugin_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

pub(crate) fn is_transaction_id(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

pub(crate) fn path_entry_exists(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("failed to stat {}: {e}", path.display())),
    }
}

pub(crate) fn assert_plain_directory(path: &Path, label: &str) -> Result<(), String> {
    let meta = fs::symlink_metadata(path)
        .map_err(|e| format!("{label} is not accessible: {} ({e})", path.display()))?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(format!(
            "{label} must be a regular directory: {}",
            path.display()
        ));
    }
    Ok(())
}

/// 删除 pluginsDir 直接子目录（先过 assert_direct_child；不存在则 no-op）。
pub(crate) fn remove_internal_directory(
    plugins_dir: &Path,
    child: &Path,
    expected: &str,
) -> Result<(), String> {
    assert_direct_child(plugins_dir, child, expected)?;
    if !path_entry_exists(child)? {
        return Ok(());
    }
    assert_plain_directory(child, "Transaction directory")?;
    fs::remove_dir_all(child).map_err(|e| format!("failed to remove {}: {e}", child.display()))
}

/// rename 两个 pluginsDir 直接子目录；目标必须不存在（Windows 语义）。
pub(crate) fn rename_internal_directory(
    plugins_dir: &Path,
    source: &Path,
    source_expected: &str,
    dest: &Path,
    dest_expected: &str,
) -> Result<(), String> {
    assert_direct_child(plugins_dir, source, source_expected)?;
    assert_direct_child(plugins_dir, dest, dest_expected)?;
    assert_plain_directory(source, "Recovery source")?;
    if path_entry_exists(dest)? {
        return Err(format!(
            "Recovery destination already exists: {}",
            dest.display()
        ));
    }
    fs::rename(source, dest).map_err(|e| {
        format!(
            "failed to rename {} → {}: {e}",
            source.display(),
            dest.display()
        )
    })
}

// ---------------------------------------------------------------------------
// 预算拷贝
// ---------------------------------------------------------------------------

struct CopyBudget {
    entries: u64,
    max_entries: u64,
    total_bytes: u64,
    max_total_bytes: u64,
}

fn consume_entry(budget: &mut CopyBudget, size: u64) -> Result<(), String> {
    budget.entries += 1;
    if budget.entries > budget.max_entries {
        return Err(format!(
            "Plugin directory exceeds {} entries",
            budget.max_entries
        ));
    }
    if budget.total_bytes > budget.max_total_bytes.saturating_sub(size) {
        return Err(format!(
            "Plugin directory exceeds {} bytes",
            budget.max_total_bytes
        ));
    }
    budget.total_bytes += size;
    Ok(())
}

/// 复制单个普通文件：File::open + 写入 + 前后 size 比对；拒绝 symlink。
fn copy_regular_file(
    source: &Path,
    dest: &Path,
    before: &fs::Metadata,
    budget: &mut CopyBudget,
) -> Result<(), String> {
    let mut src =
        fs::File::open(source).map_err(|e| format!("failed to open {}: {e}", source.display()))?;
    let opened = src.metadata().map_err(|e| e.to_string())?;
    if !opened.is_file() || opened.len() != before.len() {
        return Err(format!(
            "Plugin source changed while it was being staged: {}",
            source.display()
        ));
    }
    consume_entry(budget, opened.len())?;
    let mut dst = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dest)
        .map_err(|e| format!("failed to create {}: {e}", dest.display()))?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut remaining = opened.len();
    while remaining > 0 {
        let n = src
            .read(&mut buf)
            .map_err(|e| format!("read failed: {e}"))?;
        if n == 0 {
            return Err(format!(
                "Plugin source was truncated while it was being staged: {}",
                source.display()
            ));
        }
        dst.write_all(&buf[..n])
            .map_err(|e| format!("write failed: {e}"))?;
        remaining -= n as u64;
    }
    dst.sync_all()
        .map_err(|e| format!("failed to fsync {}: {e}", dest.display()))?;
    let after = src.metadata().map_err(|e| e.to_string())?;
    if after.len() != opened.len() {
        return Err(format!(
            "Plugin source changed while it was being staged: {}",
            source.display()
        ));
    }
    Ok(())
}

fn copy_directory_contents(
    source: &Path,
    dest: &Path,
    budget: &mut CopyBudget,
) -> Result<(), String> {
    let entries =
        fs::read_dir(source).map_err(|e| format!("failed to read {}: {e}", source.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let source_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        let meta = fs::symlink_metadata(&source_path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            return Err(format!(
                "Plugin directory contains a symbolic link: {}",
                source_path.display()
            ));
        }
        if meta.is_dir() {
            consume_entry(budget, 0)?;
            fs::create_dir(&dest_path)
                .map_err(|e| format!("failed to create {}: {e}", dest_path.display()))?;
            copy_directory_contents(&source_path, &dest_path, budget)?;
            continue;
        }
        if !meta.is_file() {
            return Err(format!(
                "Plugin directory contains an unsupported entry: {}",
                source_path.display()
            ));
        }
        copy_regular_file(&source_path, &dest_path, &meta, budget)?;
    }
    Ok(())
}

/// 白名单路径规范化（对等 normalizeAllowedFilePaths）：拒绝绝对路径、控制字符、
/// 空/`.`/`..` 段、Windows 非法字符、重复项。
fn normalize_allowed_files(allowed: &[String]) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for raw in allowed {
        let portable = raw.replace('\\', "/");
        if portable.is_empty()
            || portable.len() > 512
            || portable.starts_with('/')
            || Path::new(&portable).is_absolute()
            || portable
                .chars()
                .any(|c| (c as u32) <= 0x1f || (c as u32) == 0x7f)
        {
            return Err(format!(
                "Restricted copy contains an unsafe file path: {raw}"
            ));
        }
        let segments: Vec<&str> = portable.split('/').collect();
        for segment in &segments {
            if segment.is_empty()
                || *segment == "."
                || *segment == ".."
                || segment
                    .chars()
                    .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
                || segment.ends_with('.')
                || segment.ends_with(' ')
            {
                return Err(format!(
                    "Restricted copy contains an unsafe file path: {raw}"
                ));
            }
        }
        if posix_normalize(&portable) != portable {
            return Err(format!(
                "Restricted copy contains an unsafe file path: {raw}"
            ));
        }
        if !seen.insert(portable.clone()) {
            return Err(format!(
                "Restricted copy contains a duplicate file path: {raw}"
            ));
        }
        out.push(portable);
    }
    Ok(out)
}

fn posix_normalize(path: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            s => stack.push(s),
        }
    }
    stack.join("/")
}

/// 校验白名单文件在源树中逐段存在且为普通文件（拒绝 symlink）。
fn assert_trusted_runtime_file(source: &Path, relative: &str) -> Result<(), String> {
    let segments: Vec<&str> = relative.split('/').collect();
    let mut current = source.to_path_buf();
    for (index, segment) in segments.iter().enumerate() {
        current = current.join(segment);
        let is_leaf = index == segments.len() - 1;
        let meta = fs::symlink_metadata(&current).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!(
                    "Trusted plugin directory is missing required runtime file {relative}; \
                     it should contain only the pinned runtime files"
                )
            } else {
                format!("failed to stat {}: {e}", current.display())
            }
        })?;
        if meta.file_type().is_symlink() {
            return Err(format!(
                "Trusted plugin directory contains a symbolic link at: {relative}"
            ));
        }
        if is_leaf {
            if !meta.is_file() {
                return Err(format!(
                    "Trusted plugin runtime file is not a regular file: {relative}"
                ));
            }
        } else if !meta.is_dir() {
            return Err(format!(
                "Trusted plugin directory structure is invalid at: {relative}"
            ));
        }
    }
    Ok(())
}

fn copy_allowed_files(
    source: &Path,
    dest: &Path,
    allowed: &[String],
    budget: &mut CopyBudget,
) -> Result<(), String> {
    for file in allowed {
        assert_trusted_runtime_file(source, file)?;
        let rel = Path::new(file);
        let source_path = source.join(rel);
        let dest_path = dest.join(rel);
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
        }
        let before = fs::symlink_metadata(&source_path).map_err(|e| e.to_string())?;
        copy_regular_file(&source_path, &dest_path, &before, budget)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cruciblebox-txn-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
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

    fn make_source(root: &Path) -> PathBuf {
        let src = root.join("source");
        write_tree(
            &src,
            &[
                ("plugin.json", r#"{"name":"demo"}"#),
                ("dist/main.js", "main"),
                ("dist/renderer.js", "renderer"),
                ("assets/icon.png", "icon"),
            ],
        );
        src
    }

    fn opts(plugins_dir: &Path, name: &str, source: &Path, expected: bool) -> TransactionOptions {
        TransactionOptions {
            plugins_dir: plugins_dir.to_path_buf(),
            plugin_name: name.to_string(),
            transaction_id: "tx-123".to_string(),
            source_dir: source.to_path_buf(),
            expected_target_exists: expected,
            allowed_files: None,
        }
    }

    #[test]
    fn fresh_install_full_flow() {
        let root = temp_root("install-flow");
        let plugins = root.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        let source = make_source(&root);
        let mut txn = DirectoryTransaction::new(opts(&plugins, "demo", &source, false)).unwrap();
        assert_eq!(txn.phase(), Phase::Created);
        txn.stage().unwrap();
        assert_eq!(txn.phase(), Phase::Staged);
        assert!(txn.stage_dir().join("plugin.json").exists());
        assert!(txn.stage_dir().join("dist/main.js").exists());
        txn.swap().unwrap();
        assert_eq!(txn.phase(), Phase::Swapped);
        assert!(plugins.join("demo").join("dist/main.js").exists());
        assert!(!txn.stage_dir().exists());
        txn.commit().unwrap();
        assert_eq!(txn.phase(), Phase::Committed);
        assert!(plugins.join("demo").exists());
    }

    #[test]
    fn removal_quarantine_rolls_back_and_commits() {
        let root = temp_root("removal-flow");
        let plugins = root.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        write_tree(&plugins.join("demo"), &[("plugin.json", "old")]);

        let mut rollback =
            RemovalTransaction::new(plugins.clone(), "demo".into(), "tx-remove-1".into()).unwrap();
        rollback.quarantine().unwrap();
        assert!(!plugins.join("demo").exists());
        assert!(rollback.quarantine_dir().exists());
        rollback.rollback().unwrap();
        assert!(plugins.join("demo").exists());

        let mut commit =
            RemovalTransaction::new(plugins.clone(), "demo".into(), "tx-remove-2".into()).unwrap();
        commit.quarantine().unwrap();
        commit.commit().unwrap();
        assert!(!plugins.join("demo").exists());
        assert!(!plugins.join(".demo.remove-tx-remove-2").exists());
    }

    #[test]
    fn upgrade_full_flow_cleans_backup() {
        let root = temp_root("upgrade-flow");
        let plugins = root.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        write_tree(&plugins.join("demo"), &[("plugin.json", "old")]);
        let source = root.join("source");
        write_tree(&source, &[("plugin.json", "new")]);
        let mut txn = DirectoryTransaction::new(opts(&plugins, "demo", &source, true)).unwrap();
        txn.stage().unwrap();
        txn.swap().unwrap();
        assert!(plugins.join(".demo.backup-tx-123").exists());
        txn.commit().unwrap();
        assert!(!plugins.join(".demo.backup-tx-123").exists());
        assert_eq!(
            fs::read_to_string(plugins.join("demo").join("plugin.json")).unwrap(),
            "new"
        );
    }

    #[test]
    fn rollback_staged_removes_stage() {
        let root = temp_root("rollback-staged");
        let plugins = root.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        let source = make_source(&root);
        let mut txn = DirectoryTransaction::new(opts(&plugins, "demo", &source, false)).unwrap();
        txn.stage().unwrap();
        txn.rollback().unwrap();
        assert_eq!(txn.phase(), Phase::RolledBack);
        assert!(!txn.stage_dir().exists());
        assert!(!plugins.join("demo").exists());
    }

    #[test]
    fn rollback_staged_with_backup_restores_target() {
        // 模拟崩溃窗口：target→backup 已完成、stage→target 未执行
        let root = temp_root("rollback-staged-backup");
        let plugins = root.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        write_tree(&plugins.join("demo"), &[("plugin.json", "old")]);
        let source = root.join("source");
        write_tree(&source, &[("plugin.json", "new")]);
        let mut txn = DirectoryTransaction::new(opts(&plugins, "demo", &source, true)).unwrap();
        txn.stage().unwrap();
        fs::rename(plugins.join("demo"), plugins.join(".demo.backup-tx-123")).unwrap();
        txn.rollback().unwrap();
        assert_eq!(txn.phase(), Phase::RolledBack);
        assert_eq!(
            fs::read_to_string(plugins.join("demo").join("plugin.json")).unwrap(),
            "old"
        );
        assert!(!txn.stage_dir().exists());
    }

    #[test]
    fn rollback_swapped_restores_old_version() {
        let root = temp_root("rollback-swapped");
        let plugins = root.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        write_tree(&plugins.join("demo"), &[("plugin.json", "old")]);
        let source = root.join("source");
        write_tree(&source, &[("plugin.json", "new")]);
        let mut txn = DirectoryTransaction::new(opts(&plugins, "demo", &source, true)).unwrap();
        txn.stage().unwrap();
        txn.swap().unwrap();
        assert!(plugins.join(".demo.backup-tx-123").exists());
        txn.rollback().unwrap();
        assert_eq!(txn.phase(), Phase::RolledBack);
        assert_eq!(
            fs::read_to_string(plugins.join("demo").join("plugin.json")).unwrap(),
            "old"
        );
        assert!(!plugins.join(".demo.backup-tx-123").exists());
        assert!(!txn.stage_dir().exists());
    }

    #[test]
    fn rollback_created_is_noop() {
        let root = temp_root("rollback-created");
        let plugins = root.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        let source = make_source(&root);
        let mut txn = DirectoryTransaction::new(opts(&plugins, "demo", &source, false)).unwrap();
        txn.rollback().unwrap();
        assert_eq!(txn.phase(), Phase::RolledBack);
    }

    #[test]
    fn rollback_after_commit_errors() {
        let root = temp_root("rollback-committed");
        let plugins = root.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        let source = make_source(&root);
        let mut txn = DirectoryTransaction::new(opts(&plugins, "demo", &source, false)).unwrap();
        txn.stage().unwrap();
        txn.swap().unwrap();
        txn.commit().unwrap();
        let err = txn.rollback().unwrap_err();
        assert!(err.contains("committed"));
    }

    #[test]
    fn swap_fails_when_target_appears_during_staging() {
        let root = temp_root("swap-target-appears");
        let plugins = root.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        let source = make_source(&root);
        let mut txn = DirectoryTransaction::new(opts(&plugins, "demo", &source, false)).unwrap();
        txn.stage().unwrap();
        fs::create_dir_all(plugins.join("demo")).unwrap();
        let err = txn.swap().unwrap_err();
        assert!(err.contains("changed during staging"));
    }

    #[test]
    fn stage_rejects_expected_target_mismatch() {
        let root = temp_root("stage-mismatch");
        let plugins = root.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        let source = make_source(&root);
        let mut txn = DirectoryTransaction::new(opts(&plugins, "demo", &source, true)).unwrap();
        let err = txn.stage().unwrap_err();
        assert!(err.contains("expected to be present"));
        write_tree(&plugins.join("demo"), &[("plugin.json", "x")]);
        let mut txn2 = DirectoryTransaction::new(opts(&plugins, "demo", &source, false)).unwrap();
        let err2 = txn2.stage().unwrap_err();
        assert!(err2.contains("expected to be absent"));
    }

    #[test]
    fn stage_rejects_symlink() {
        let root = temp_root("stage-symlink");
        let plugins = root.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        let source = root.join("source");
        write_tree(&source, &[("plugin.json", "x")]);
        let link = source.join("evil");
        #[cfg(windows)]
        let created = std::os::windows::fs::symlink_dir(&source, &link).is_ok();
        #[cfg(not(windows))]
        let created = std::os::unix::fs::symlink(&source, &link).is_ok();
        if !created {
            return; // 当前环境无法创建 symlink，跳过
        }
        let mut txn = DirectoryTransaction::new(opts(&plugins, "demo", &source, false)).unwrap();
        let err = txn.stage().unwrap_err();
        assert!(err.contains("symbolic link"));
        assert!(!txn.stage_dir().exists());
    }

    #[test]
    fn stage_with_allowed_files_copies_only_whitelist() {
        let root = temp_root("allowed-files");
        let plugins = root.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        let source = make_source(&root);
        let mut o = opts(&plugins, "demo", &source, false);
        o.allowed_files = Some(vec!["dist/main.js".into(), "plugin.json".into()]);
        let mut txn = DirectoryTransaction::new(o).unwrap();
        txn.stage().unwrap();
        assert!(txn.stage_dir().join("dist/main.js").exists());
        assert!(txn.stage_dir().join("plugin.json").exists());
        assert!(!txn.stage_dir().join("dist/renderer.js").exists());
        assert!(!txn.stage_dir().join("assets").exists());
    }

    #[test]
    fn stage_with_allowed_files_missing_file_errors() {
        let root = temp_root("allowed-missing");
        let plugins = root.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        let source = make_source(&root);
        let mut o = opts(&plugins, "demo", &source, false);
        o.allowed_files = Some(vec!["dist/main.js".into(), "missing.js".into()]);
        let mut txn = DirectoryTransaction::new(o).unwrap();
        let err = txn.stage().unwrap_err();
        assert!(err.contains("missing required runtime file"));
        assert!(!txn.stage_dir().exists());
    }

    #[test]
    fn stage_rejects_too_many_entries() {
        let root = temp_root("budget-entries");
        let plugins = root.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        for i in 0..=DEFAULT_MAX_ENTRIES {
            fs::write(source.join(format!("f{i:05}.txt")), b"x").unwrap();
        }
        let mut txn = DirectoryTransaction::new(opts(&plugins, "demo", &source, false)).unwrap();
        let err = txn.stage().unwrap_err();
        assert!(err.contains("exceeds"));
        assert!(!txn.stage_dir().exists());
    }

    #[test]
    fn trusted_allowlist_matches_permissions() {
        assert_eq!(
            trusted_allowlist(&["storage:read".into(), "trusted:unienv".into()]),
            Some(vec![
                "dist/main.js".into(),
                "dist/renderer.js".into(),
                "plugin.json".into()
            ])
        );
        assert_eq!(
            trusted_allowlist(&["trusted:document-engine".into()]),
            Some(vec![
                "dist/main.js".into(),
                "dist/renderer.js".into(),
                "plugin.json".into()
            ])
        );
        assert_eq!(trusted_allowlist(&["storage:read".into()]), None);
        assert_eq!(trusted_allowlist(&[]), None);
    }

    #[test]
    fn assert_direct_child_rejects_escape() {
        let root = temp_root("direct-child");
        let plugins = root.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        let outside = root.join("outside");
        fs::create_dir_all(&outside).unwrap();
        assert!(assert_direct_child(&plugins, &outside, "outside").is_err());
        assert!(assert_direct_child(&plugins, &plugins.join("demo"), "other").is_err());
        assert!(assert_direct_child(&plugins, &plugins.join("demo"), "demo").is_ok());
    }

    #[test]
    fn new_rejects_invalid_plugin_name_and_txid() {
        let root = temp_root("bad-name");
        let plugins = root.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        let source = make_source(&root);
        let mut o = opts(&plugins, "Bad Name!", &source, false);
        o.plugin_name = "Bad Name!".into();
        assert!(DirectoryTransaction::new(o).is_err());
        let mut o2 = opts(&plugins, "demo", &source, false);
        o2.transaction_id = "bad tx id".into();
        assert!(DirectoryTransaction::new(o2).is_err());
    }

    #[test]
    fn rename_internal_directory_rejects_existing_destination() {
        let root = temp_root("rename-dest-exists");
        let plugins = root.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        write_tree(&plugins.join("a"), &[("f", "1")]);
        write_tree(&plugins.join("b"), &[("f", "2")]);
        let err =
            rename_internal_directory(&plugins, &plugins.join("a"), "a", &plugins.join("b"), "b")
                .unwrap_err();
        assert!(err.contains("already exists"));
        assert!(plugins.join("a").exists());
        assert!(plugins.join("b").exists());
    }
}
