//! Staged file output shared by host file tools. A target is never deleted
//! before the staged result has been written and validated.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct OutputTransaction {
    requested: PathBuf,
    target: PathBuf,
    stage: PathBuf,
    replace: bool,
    published: bool,
}

impl OutputTransaction {
    pub fn new(target: &Path, replace: bool) -> Result<Self, String> {
        if target.is_dir() {
            return Err(format!("输出目标是文件夹：{}", target.display()));
        }
        let parent = target
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| format!("创建输出目录失败: {error}"))?;
        let requested = target.to_path_buf();
        let target = if replace || !target.exists() {
            target.to_path_buf()
        } else {
            next_available_sibling(target)?
        };
        let stage = reserve_sibling(&target, "tmp")?;
        Ok(Self {
            requested,
            target,
            stage,
            replace,
            published: false,
        })
    }

    pub fn write_bytes(&self, bytes: &[u8]) -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&self.stage)
            .map_err(|error| format!("打开输出暂存文件失败: {error}"))?;
        file.write_all(bytes)
            .map_err(|error| format!("写入输出暂存文件失败: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("同步输出暂存文件失败: {error}"))
    }

    pub fn stage_path(&self) -> &Path {
        &self.stage
    }

    pub fn publish_durable(
        mut self,
        ctx: &crate::task_runtime::Context,
        terminal: bool,
        validate: impl FnOnce(&Path) -> Result<(), String>,
    ) -> Result<PathBuf, String> {
        validate(&self.stage)?;
        let result = ctx.publish_file(&self.stage, &self.target, self.replace, terminal);
        self.published = result.is_ok() || ctx.publication_pending();
        result.and_then(|value| {
            value
                .get("path")
                .and_then(serde_json::Value::as_str)
                .map(PathBuf::from)
                .ok_or_else(|| "published path missing".into())
        })
    }

    pub fn publish_durable_with_reference(
        mut self,
        ctx: &crate::task_runtime::Context,
        reference: &Path,
        validate: impl FnOnce(&Path) -> Result<(), String>,
    ) -> Result<PathBuf, String> {
        validate(&self.stage)?;
        let result = ctx.publish_object(
            &self.stage,
            &self.target,
            self.replace,
            false,
            Some(reference),
        );
        self.published = result.is_ok() || ctx.publication_pending();
        result.map(|_| self.target.clone())
    }

    pub fn publish(
        mut self,
        validate: impl FnOnce(&Path) -> Result<(), String>,
    ) -> Result<PathBuf, String> {
        fs::File::open(&self.stage).map_err(|error| format!("读取输出暂存文件失败: {error}"))?;
        validate(&self.stage)?;
        if !self.replace {
            // Linking the validated staging file reserves the destination only if it
            // does not exist. A rename can overwrite a file created by another task.
            let mut candidate = self.target.clone();
            for _ in 0..=999 {
                match fs::hard_link(&self.stage, &candidate) {
                    Ok(()) => {
                        if let Err(error) = fs::remove_file(&self.stage) {
                            eprintln!("[output] retained stage {}: {error}", self.stage.display());
                        }
                        self.published = true;
                        return Ok(candidate);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        candidate = next_available_sibling(&self.requested)?;
                    }
                    Err(error) => return Err(format!("提交输出文件失败: {error}")),
                }
            }
            return Err("无法生成不冲突的输出文件名".into());
        }
        let backup = if self.replace && self.target.exists() {
            let backup = reserve_sibling(&self.target, "bak")?;
            fs::remove_file(&backup).map_err(|error| format!("准备输出备份失败: {error}"))?;
            fs::rename(&self.target, &backup)
                .map_err(|error| format!("备份原输出失败: {error}"))?;
            Some(backup)
        } else {
            None
        };
        if let Err(error) = fs::rename(&self.stage, &self.target) {
            if let Some(backup) = &backup {
                fs::rename(backup, &self.target).map_err(|restore| {
                    format!(
                        "提交输出失败: {error}; 恢复原文件失败: {restore}; 备份位于 {}",
                        backup.display()
                    )
                })?;
            }
            return Err(format!("提交输出文件失败: {error}"));
        }
        self.published = true;
        if let Some(backup) = backup {
            if let Err(error) = fs::remove_file(&backup) {
                eprintln!("[output] retained backup {}: {error}", backup.display());
            }
        }
        Ok(self.target.clone())
    }
}

impl Drop for OutputTransaction {
    fn drop(&mut self) {
        if !self.published {
            let _ = fs::remove_file(&self.stage);
        }
    }
}

fn reserve_sibling(target: &Path, suffix: &str) -> Result<PathBuf, String> {
    let stem = target
        .file_stem()
        .ok_or_else(|| "输出目标缺少文件名".to_string())?
        .to_string_lossy();
    let extension = target.extension().map(|value| value.to_string_lossy());
    for _ in 0..16 {
        let token = crate::rand_token::random_token_alnum(12)?;
        let stage_name = match &extension {
            Some(extension) => format!(".{stem}.{token}.{suffix}.{extension}"),
            None => format!(".{stem}.{token}.{suffix}"),
        };
        let candidate = target.with_file_name(stage_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("创建输出暂存文件失败: {error}")),
        }
    }
    Err("无法分配唯一输出暂存文件".into())
}

fn next_available_sibling(target: &Path) -> Result<PathBuf, String> {
    let stem = target
        .file_stem()
        .ok_or_else(|| "输出目标缺少文件名".to_string())?
        .to_string_lossy();
    let extension = target.extension().map(|value| value.to_string_lossy());
    for index in 1..=999u32 {
        let name = match &extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        let candidate = target.with_file_name(name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("无法生成不冲突的输出文件名".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "cb-output-{}-{}",
            std::process::id(),
            crate::rand_token::random_token_alnum(8).unwrap()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn existing_target_is_preserved_by_default() {
        let dir = temp_dir();
        let target = dir.join("报告.txt");
        fs::write(&target, b"old").unwrap();
        let transaction = OutputTransaction::new(&target, false).unwrap();
        assert_eq!(transaction.stage_path().extension().unwrap(), "txt");
        transaction.write_bytes(b"new").unwrap();
        let output = transaction.publish(|_| Ok(())).unwrap();
        assert_ne!(output, target);
        assert_eq!(fs::read(&target).unwrap(), b"old");
        assert_eq!(fs::read(output).unwrap(), b"new");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_validation_keeps_old_target() {
        let dir = temp_dir();
        let target = dir.join("report.pdf");
        fs::write(&target, b"old").unwrap();
        let transaction = OutputTransaction::new(&target, true).unwrap();
        transaction.write_bytes(b"bad").unwrap();
        assert!(transaction.publish(|_| Err("invalid".into())).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"old");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn concurrent_targets_are_published_without_overwriting() {
        let dir = temp_dir();
        let target = dir.join("报告.txt");
        let first = OutputTransaction::new(&target, false).unwrap();
        let second = OutputTransaction::new(&target, false).unwrap();
        first.write_bytes(b"first").unwrap();
        second.write_bytes(b"second").unwrap();
        let first_path = first.publish(|_| Ok(())).unwrap();
        let second_path = second.publish(|_| Ok(())).unwrap();
        assert_ne!(first_path, second_path);
        assert_eq!(fs::read(first_path).unwrap(), b"first");
        assert_eq!(fs::read(second_path).unwrap(), b"second");
        fs::remove_dir_all(dir).unwrap();
    }
}
