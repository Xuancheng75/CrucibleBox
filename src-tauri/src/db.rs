//! Tauri application adapter over the standalone SQLite repository.
//! The local newtype supplies Next protocol trait implementations without coupling
//! the repository crate to transport or Tauri. Schema and data behavior live in the crate.
pub use cruciblebox_repository::{DbStatus, PluginBackendRecord, PluginRow, VersionFields};
#[derive(Clone)]
pub struct Db(cruciblebox_repository::Db);
impl Db {
    #[cfg(test)]
    pub(crate) fn conn(&self) -> &std::sync::Mutex<rusqlite::Connection> {
        self.0.fixture_connection()
    }

    pub(crate) fn repository(&self) -> &cruciblebox_repository::Db {
        &self.0
    }
    pub fn open(path: &std::path::Path) -> rusqlite::Result<Self> {
        cruciblebox_repository::Db::open(path).map(Self)
    }
}
impl std::ops::Deref for Db {
    type Target = cruciblebox_repository::Db;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
