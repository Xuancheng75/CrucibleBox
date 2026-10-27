//! API 6 storage dispatch. Envelope validation and permissions are enforced by admission.
use crate::db::Db;
use serde_json::Value;

pub fn dispatch(
    db: &Db,
    owner: &str,
    session: &str,
    method: &str,
    p: &Value,
) -> Result<Value, String> {
    let storage = db.repository();
    let key = || p["key"].as_str().unwrap();
    let result = match method {
        "runtime.ping" => return Ok(Value::String("pong".into())),
        "config.get" => return storage.config_get(owner),
        "config.patch" => return storage.config_patch(owner, &p["values"]),
        "storage.get" => return storage.get(owner, key()).map(|v| v.unwrap_or(Value::Null)),
        "storage.set" => storage.set(owner, key(), &p["value"]),
        "storage.delete" => storage.delete(owner, key()),
        "storage.batch" => storage.batch(owner, p["operations"].as_array().unwrap()),
        "storage.list" | "storage.keys" => {
            let prefix = p["prefix"].as_str().unwrap();
            let limit = p["limit"].as_u64().unwrap() as u32;
            let after = p["after"].as_str();
            return if method == "storage.list" {
                storage.list(owner, prefix, limit, after)
            } else {
                storage.keys(owner, prefix, limit, after)
            };
        }
        "storage.read.begin" => return storage.read_begin(owner, session, key()),
        "storage.read.chunk" => {
            return storage.read_chunk(
                owner,
                session,
                p["readId"].as_str().unwrap(),
                p["offset"].as_u64().unwrap(),
            )
        }
        "storage.read.close" => storage.read_close(owner, session, p["readId"].as_str().unwrap()),
        "storage.write.begin" => {
            return storage.write_begin(
                owner,
                session,
                p["writes"].as_array().unwrap(),
                p["deletes"].as_array().unwrap(),
            )
        }
        "storage.write.chunk" => {
            return storage.write_chunk(
                owner,
                session,
                p["transactionId"].as_str().unwrap(),
                key(),
                p["offset"].as_u64().unwrap(),
                p["data"].as_str().unwrap(),
            )
        }
        "storage.write.commit" => {
            storage.write_commit(owner, session, p["transactionId"].as_str().unwrap())
        }
        "storage.write.abort" => {
            storage.write_abort(owner, session, p["transactionId"].as_str().unwrap())
        }
        _ => return Err("PLATFORM_METHOD_NOT_CONNECTED".into()),
    };
    result.map(|()| Value::Null)
}
