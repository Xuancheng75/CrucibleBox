//! One host owns a data profile. Windows releases this handle on exit or crash.
use std::{fs::File, io, os::windows::fs::OpenOptionsExt, path::Path};

pub struct InstanceLock {
    _file: File,
}

pub fn acquire(appdata: &Path) -> io::Result<InstanceLock> {
    std::fs::create_dir_all(appdata)?;
    let file = File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .share_mode(0)
        .open(appdata.join(".cruciblebox-host.lock"))?;
    Ok(InstanceLock { _file: file })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_has_one_owner_and_releases_without_deleting_a_lock_file() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let owner = acquire(first.path()).unwrap();
        assert_eq!(
            acquire(first.path()).err().unwrap().raw_os_error(),
            Some(32)
        );
        let independent = acquire(second.path()).unwrap();
        drop(owner);
        assert!(first.path().join(".cruciblebox-host.lock").is_file());
        let reopened = acquire(first.path()).unwrap();
        drop(reopened);
        drop(independent);
    }
}
