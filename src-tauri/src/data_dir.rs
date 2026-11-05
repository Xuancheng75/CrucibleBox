// L3 数据路径迁移（1.8.1）
// 目标：把既有数据从 %APPDATA%\openbox 一次性搬迁到 %APPDATA%\cruciblebox。
// 语义（与 docs/tauri-migration-plan.md L3 对齐）：
// - 仅在源目录存在且目标目录尚未创建时执行（.migrated 表示已完成）
// - 搬迁 = 拷贝到同卷临时目录后原子 rename，保留源目录
// - DB 先做 wal_checkpoint(TRUNCATE) 合并 WAL；checkpoint 失败且 -wal 非空 → 中止报错
// - 拷贝子集：data/openbox.db、plugins/、logs/

use std::fs;
use std::path::{Path, PathBuf};

pub const OLD_DATA_DIR_NAME: &str = "openbox";
pub const NEW_DATA_DIR_NAME: &str = "cruciblebox";
const MIGRATED_MARKER: &str = ".migrated";

#[derive(Debug, Default)]
pub struct DataMigrationReport {
    pub source: Option<PathBuf>,
    pub target: PathBuf,
    pub migrated: bool,
    pub copied_entries: Vec<String>,
    pub checkpointed: bool,
}

/// 解析 APPDATA 根目录（Windows %APPDATA%）。测试可注入 base_dir。
fn appdata_root(base_dir: Option<&Path>) -> PathBuf {
    base_dir.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    })
}

pub fn resolve_paths(base_dir: Option<&Path>) -> (PathBuf, PathBuf) {
    let root = appdata_root(base_dir);
    (root.join(OLD_DATA_DIR_NAME), root.join(NEW_DATA_DIR_NAME))
}

/// 执行搬迁。返回报告；任何失败以 io::Result 报出。
pub fn migrate(base_dir: Option<&Path>) -> std::io::Result<DataMigrationReport> {
    let (source, target) = resolve_paths(base_dir);
    let marker = target.join(MIGRATED_MARKER);
    let mut report = DataMigrationReport {
        source: Some(source.clone()),
        target: target.clone(),
        ..Default::default()
    };

    if marker.exists() {
        return Ok(report); // 已成功迁移过
    }
    if !source.exists() {
        return Ok(report); // 无旧数据，无需搬迁
    }
    match target.symlink_metadata() {
        Ok(_) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!(
                    "迁移目标 {} 已存在且缺少迁移标记。请核查源目录 {} 与目标目录中的数据，确认恢复方案后再继续。",
                    target.display(),
                    source.display()
                ),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    // 1) DB：先 checkpoint 合并 WAL；失败且 -wal 非空 → 中止（避免拷贝陈旧主文件）
    let db_source = source.join("data").join("openbox.db");
    if db_source.exists() {
        match checkpoint_wal(&db_source) {
            CheckpointResult::Done => report.checkpointed = true,
            CheckpointResult::NoWal => {}
            CheckpointResult::Failed => {
                return Err(std::io::Error::other(
                    format!(
                        "L3 migration aborted: wal_checkpoint failed for {} (WAL holds committed data); \
                         close the old app instance and retry",
                        db_source.display()
                    ),
                ));
            }
        }
    }

    // 标记与数据一起发布，失败时只清理由本次迁移创建的暂存目录。
    let staging = tempfile::Builder::new()
        .prefix("cruciblebox-migration-")
        .tempdir_in(appdata_root(base_dir))?;
    let tmp = staging.path();

    if source.join("data").exists() {
        copy_tree(&source.join("data"), &tmp.join("data"))?;
        report.copied_entries.push("data".into());
    }
    if source.join("plugins").exists() {
        copy_tree(&source.join("plugins"), &tmp.join("plugins"))?;
        report.copied_entries.push("plugins".into());
    }
    if source.join("logs").exists() {
        copy_tree(&source.join("logs"), &tmp.join("logs"))?;
        report.copied_entries.push("logs".into());
    }

    fs::write(tmp.join(MIGRATED_MARKER), "1.8.1")?;
    fs::rename(tmp, &target)?;

    report.migrated = !report.copied_entries.is_empty();
    Ok(report)
}

enum CheckpointResult {
    /// checkpoint 成功执行（-wal 存在并已合并）
    Done,
    /// 无 -wal 文件，无需合并
    NoWal,
    /// checkpoint 失败（-wal 存在但合并失败 → 数据完整性风险）
    Failed,
}

/// 打开 DB 执行 wal_checkpoint(TRUNCATE)，把 -wal 内容合并回主文件。
fn checkpoint_wal(db_path: &Path) -> CheckpointResult {
    let wal_path = PathBuf::from(format!("{}-wal", db_path.display()));
    if !wal_path.exists() {
        return CheckpointResult::NoWal;
    }
    if cruciblebox_repository::checkpoint_wal_before_copy(db_path).is_ok() {
        CheckpointResult::Done
    } else {
        CheckpointResult::Failed
    }
}

/// 递归拷贝目录树（保留结构，不保留属性；跳过 -wal/-shm 残留）。
fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !src.is_dir() {
        fs::create_dir_all(dst)?;
        fs::copy(src, dst)?;
        return Ok(());
    }
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with("-wal") || name.ends_with("-shm") {
                continue;
            }
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cruciblebox-l3-test-{}", name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn migrates_when_source_exists_and_target_missing() {
        let root = temp_root("basic");
        let old = root.join(OLD_DATA_DIR_NAME);
        fs::create_dir_all(old.join("data")).unwrap();
        fs::create_dir_all(old.join("plugins")).unwrap();
        fs::create_dir_all(old.join("logs")).unwrap();
        fs::write(old.join("data").join("openbox.db"), b"sqlite").unwrap();
        fs::write(old.join("plugins").join("note.txt"), b"hi").unwrap();

        let report = migrate(Some(&root)).unwrap();
        assert!(report.migrated);
        assert!(report.copied_entries.contains(&"data".into()));
        assert!(report.copied_entries.contains(&"plugins".into()));
        assert!(report.copied_entries.contains(&"logs".into()));
        assert!(root
            .join(NEW_DATA_DIR_NAME)
            .join("data/openbox.db")
            .exists());
        assert!(root
            .join(NEW_DATA_DIR_NAME)
            .join("plugins/note.txt")
            .exists());
        assert!(root.join(NEW_DATA_DIR_NAME).join(MIGRATED_MARKER).exists());
    }

    #[test]
    fn idempotent_when_marker_exists() {
        let root = temp_root("idempotent");
        let old = root.join(OLD_DATA_DIR_NAME);
        fs::create_dir_all(old.join("data")).unwrap();
        fs::write(old.join("data").join("openbox.db"), b"sqlite").unwrap();
        let new = root.join(NEW_DATA_DIR_NAME);
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("marker.txt"), b"keep").unwrap();
        fs::write(new.join(MIGRATED_MARKER), b"1.8.1").unwrap();

        let report = migrate(Some(&root)).unwrap();
        assert!(!report.migrated);
        assert!(new.join("marker.txt").exists());
    }

    #[test]
    fn noop_when_no_source() {
        let root = temp_root("nosource");
        let report = migrate(Some(&root)).unwrap();
        assert!(!report.migrated);
        assert!(!root.join(NEW_DATA_DIR_NAME).exists());
    }

    #[test]
    fn refuses_to_replace_existing_unmarked_data() {
        let root = temp_root("unmarked-target");
        let old = root.join(OLD_DATA_DIR_NAME);
        fs::create_dir_all(old.join("data")).unwrap();
        fs::create_dir_all(old.join("plugins")).unwrap();
        fs::write(old.join("data").join("openbox.db"), b"sqlite").unwrap();
        fs::write(old.join("plugins").join("note.txt"), b"hi").unwrap();
        let new = root.join(NEW_DATA_DIR_NAME);
        fs::create_dir_all(new.join("data")).unwrap();
        fs::write(new.join("data/openbox.db"), b"new user database").unwrap();

        let error = migrate(Some(&root)).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(
            fs::read(new.join("data/openbox.db")).unwrap(),
            b"new user database"
        );
        assert_eq!(fs::read(old.join("data/openbox.db")).unwrap(), b"sqlite");
        assert!(!new.join(MIGRATED_MARKER).exists());
    }

    #[test]
    fn aborts_when_wal_checkpoint_fails() {
        // H2 场景：-wal 存在但 checkpoint 无法执行（损坏 DB）→ 中止迁移而非静默拷贝陈旧主文件
        let root = temp_root("wal-abort");
        let old = root.join(OLD_DATA_DIR_NAME);
        fs::create_dir_all(old.join("data")).unwrap();
        // 损坏的"主文件" + 伪造 -wal，保证 checkpoint 打开失败
        fs::write(old.join("data").join("openbox.db"), b"not a sqlite db").unwrap();
        fs::write(old.join("data").join("openbox.db-wal"), b"fake-wal").unwrap();

        let err = migrate(Some(&root)).unwrap_err();
        assert!(err.to_string().contains("checkpoint failed"));
        assert!(!root.join(NEW_DATA_DIR_NAME).join(MIGRATED_MARKER).exists());
        assert!(!root.join(NEW_DATA_DIR_NAME).exists());
    }
}
