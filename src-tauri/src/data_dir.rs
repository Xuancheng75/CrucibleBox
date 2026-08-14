// L3 数据路径迁移（1.8.1）
// 目标：把既有数据从 %APPDATA%\openbox 一次性搬迁到 %APPDATA%\cruciblebox。
// 语义（与 docs/tauri-migration-plan.md L3 对齐）：
// - 仅当源目录存在且目标未被成功迁移过时执行（以 .migrated 标记文件判定，幂等）
// - 搬迁 = 拷贝到同卷 .tmp 目录后原子 rename（保留源目录可回滚；避免部分拷贝陷阱）
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
    base_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| {
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

    // 幂等判定：以标记文件为准（target 目录残留但不完整时也重新迁移）
    if marker.exists() {
        return Ok(report); // 已成功迁移过
    }
    if !source.exists() {
        return Ok(report); // 无旧数据，无需搬迁
    }

    // 1) DB：先 checkpoint 合并 WAL；失败且 -wal 非空 → 中止（避免拷贝陈旧主文件）
    let db_source = source.join("data").join("openbox.db");
    if db_source.exists() {
        match checkpoint_wal(&db_source) {
            CheckpointResult::Done => report.checkpointed = true,
            CheckpointResult::NoWal => {}
            CheckpointResult::Failed => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!(
                        "L3 migration aborted: wal_checkpoint failed for {} (WAL holds committed data); \
                         close the old app instance and retry",
                        db_source.display()
                    ),
                ));
            }
        }
    }

    // 2) 拷贝到同卷临时目录，成功后原子 rename（H3：避免部分拷贝 + target.exists() 幂等陷阱）
    let tmp = appdata_root(base_dir).join(format!("{}.tmp", NEW_DATA_DIR_NAME));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp)?;

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

    // 3) 原子落位 + 写标记（先写标记再 rename 会丢；故 rename 后写标记）
    if target.exists() {
        fs::remove_dir_all(&target)?;
    }
    fs::rename(&tmp, &target)?;
    fs::write(&marker, "1.8.1")?;

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
    match rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(conn) => {
            let ok = conn
                .pragma_update(None, "wal_checkpoint", "TRUNCATE")
                .is_ok();
            drop(conn);
            if ok {
                CheckpointResult::Done
            } else {
                CheckpointResult::Failed
            }
        }
        Err(_) => CheckpointResult::Failed,
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
        assert!(root.join(NEW_DATA_DIR_NAME).join("data/openbox.db").exists());
        assert!(root.join(NEW_DATA_DIR_NAME).join("plugins/note.txt").exists());
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
    fn retries_after_partial_copy_without_marker() {
        let root = temp_root("partial");
        let old = root.join(OLD_DATA_DIR_NAME);
        fs::create_dir_all(old.join("data")).unwrap();
        fs::create_dir_all(old.join("plugins")).unwrap();
        fs::write(old.join("data").join("openbox.db"), b"sqlite").unwrap();
        fs::write(old.join("plugins").join("note.txt"), b"hi").unwrap();
        // 模拟上次部分拷贝残留：target 存在但无标记
        let new = root.join(NEW_DATA_DIR_NAME);
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("truncated.db"), b"x").unwrap();

        let report = migrate(Some(&root)).unwrap();
        assert!(report.migrated);
        assert!(!new.join("truncated.db").exists()); // 残留被替换
        assert!(new.join("data/openbox.db").exists());
        assert!(new.join(MIGRATED_MARKER).exists());
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
