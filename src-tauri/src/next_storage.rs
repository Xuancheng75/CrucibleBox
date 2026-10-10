//! Next transport adapter over repository-owned namespace storage.
use crate::db::Db;
use cruciblebox_next_protocol::runtime::Storage;
use serde_json::Value;
impl Storage for Db {
    fn config_get(&self, owner: &str) -> Result<Value, String> {
        self.repository().config_get(owner)
    }
    fn config_patch(&self, owner: &str, values: &Value) -> Result<Value, String> {
        self.repository().config_patch(owner, values)
    }
    fn keys(
        &self,
        owner: &str,
        prefix: &str,
        limit: u32,
        after: Option<&str>,
    ) -> Result<Value, String> {
        self.repository().keys(owner, prefix, limit, after)
    }
    fn read_begin(&self, owner: &str, session: &str, key: &str) -> Result<Value, String> {
        self.repository().read_begin(owner, session, key)
    }
    fn read_chunk(
        &self,
        owner: &str,
        session: &str,
        id: &str,
        offset: u64,
    ) -> Result<Value, String> {
        self.repository().read_chunk(owner, session, id, offset)
    }
    fn read_close(&self, owner: &str, session: &str, id: &str) -> Result<(), String> {
        self.repository().read_close(owner, session, id)
    }
    fn write_begin(
        &self,
        owner: &str,
        session: &str,
        writes: &[Value],
        deletes: &[Value],
    ) -> Result<Value, String> {
        self.repository()
            .write_begin(owner, session, writes, deletes)
    }
    fn write_chunk(
        &self,
        owner: &str,
        session: &str,
        id: &str,
        key: &str,
        offset: u64,
        data: &str,
    ) -> Result<Value, String> {
        self.repository()
            .write_chunk(owner, session, id, key, offset, data)
    }
    fn write_commit(&self, owner: &str, session: &str, id: &str) -> Result<(), String> {
        self.repository().write_commit(owner, session, id)
    }
    fn write_abort(&self, owner: &str, session: &str, id: &str) -> Result<(), String> {
        self.repository().write_abort(owner, session, id)
    }
    fn delete(&self, owner: &str, key: &str) -> Result<(), String> {
        self.repository().delete(owner, key)
    }
    fn batch(&self, owner: &str, operations: &[Value]) -> Result<(), String> {
        self.repository().batch(owner, operations)
    }
    fn list(
        &self,
        owner: &str,
        prefix: &str,
        limit: u32,
        after: Option<&str>,
    ) -> Result<Value, String> {
        self.repository().list(owner, prefix, limit, after)
    }
    fn get(&self, owner: &str, key: &str) -> Result<Option<Value>, String> {
        self.repository().get(owner, key)
    }
    fn set(&self, owner: &str, key: &str, value: &Value) -> Result<(), String> {
        self.repository().set(owner, key, value)
    }
}
