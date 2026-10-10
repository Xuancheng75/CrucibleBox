//! Small synchronous operations; storage owner comes exclusively from admission.
use crate::{gateway::Session, Call};
use serde_json::Value;

pub trait Storage {
    fn result_begin(
        &self,
        _owner: &str,
        _session: &str,
        _method: &str,
        _value: &Value,
    ) -> Result<Value, String> {
        Err("BUDGET_EXCEEDED".into())
    }
    fn result_chunk(
        &self,
        _owner: &str,
        _session: &str,
        _id: &str,
        _offset: u64,
    ) -> Result<Value, String> {
        Err("SESSION_DENIED".into())
    }
    fn result_close(&self, _owner: &str, _session: &str, _id: &str) -> Result<(), String> {
        Err("SESSION_DENIED".into())
    }

    fn ui_call(&self, _owner: &str, _method: &str, _params: &Value) -> Result<Value, String> {
        Err("SESSION_DENIED".into())
    }

    fn tasks_get(&self, _owner: &str, _id: &str) -> Result<Value, String> {
        Err("SESSION_DENIED".into())
    }
    fn tasks_cancel(&self, _owner: &str, _id: &str) -> Result<Value, String> {
        Err("SESSION_DENIED".into())
    }
    fn tasks_list(&self, _owner: &str, _limit: u32, _after: Option<&str>) -> Result<Value, String> {
        Err("SESSION_DENIED".into())
    }
    fn config_get(&self, _owner: &str) -> Result<Value, String> {
        Err("SESSION_DENIED".into())
    }
    fn config_patch(&self, _owner: &str, _values: &Value) -> Result<Value, String> {
        Err("SESSION_DENIED".into())
    }
    fn service_call(
        &self,
        _owner: &str,
        _service: &str,
        _payload: &Value,
    ) -> Result<Value, String> {
        Err("SESSION_DENIED".into())
    }
    fn backend_call(&self, _owner: &str, _method: &str, _args: &[Value]) -> Result<Value, String> {
        Err("SESSION_DENIED".into())
    }
    fn delete(&self, _owner: &str, _key: &str) -> Result<(), String> {
        Err("STORAGE_UNAVAILABLE".into())
    }
    fn batch(&self, _owner: &str, _operations: &[Value]) -> Result<(), String> {
        Err("STORAGE_UNAVAILABLE".into())
    }
    fn list(
        &self,
        _owner: &str,
        _prefix: &str,
        _limit: u32,
        _after: Option<&str>,
    ) -> Result<Value, String> {
        Err("STORAGE_UNAVAILABLE".into())
    }
    fn keys(
        &self,
        _owner: &str,
        _prefix: &str,
        _limit: u32,
        _after: Option<&str>,
    ) -> Result<Value, String> {
        Err("STORAGE_UNAVAILABLE".into())
    }
    fn read_begin(&self, _owner: &str, _session: &str, _key: &str) -> Result<Value, String> {
        Err("STORAGE_UNAVAILABLE".into())
    }
    fn read_chunk(
        &self,
        _owner: &str,
        _session: &str,
        _read_id: &str,
        _offset: u64,
    ) -> Result<Value, String> {
        Err("STORAGE_UNAVAILABLE".into())
    }
    fn read_close(&self, _owner: &str, _session: &str, _read_id: &str) -> Result<(), String> {
        Err("STORAGE_UNAVAILABLE".into())
    }
    fn write_begin(
        &self,
        _owner: &str,
        _session: &str,
        _writes: &[Value],
        _deletes: &[Value],
    ) -> Result<Value, String> {
        Err("STORAGE_UNAVAILABLE".into())
    }
    fn write_chunk(
        &self,
        _owner: &str,
        _session: &str,
        _transaction_id: &str,
        _key: &str,
        _offset: u64,
        _data: &str,
    ) -> Result<Value, String> {
        Err("STORAGE_UNAVAILABLE".into())
    }
    fn write_commit(
        &self,
        _owner: &str,
        _session: &str,
        _transaction_id: &str,
    ) -> Result<(), String> {
        Err("STORAGE_UNAVAILABLE".into())
    }
    fn write_abort(
        &self,
        _owner: &str,
        _session: &str,
        _transaction_id: &str,
    ) -> Result<(), String> {
        Err("STORAGE_UNAVAILABLE".into())
    }
    fn get(&self, owner: &str, key: &str) -> Result<Option<Value>, String>;
    fn set(&self, owner: &str, key: &str, value: &Value) -> Result<(), String>;
}

pub fn dispatch(
    session: &mut Session,
    storage: &impl Storage,
    raw: &str,
    caller: &str,
    now_ms: u64,
) -> Result<Value, String> {
    let admitted = session.admit(raw, caller, now_ms).map_err(str::to_owned)?;
    let id = admitted.request.request_id.clone();
    let scope = admitted.request.session.clone();
    let method = serde_json::to_value(&admitted.request).map_err(|_| "INVALID_REQUEST")?["method"]
        .as_str()
        .ok_or("INVALID_REQUEST")?
        .to_owned();
    let result = match admitted.request.call {
        Call::ResultReadChunk(params) => storage.result_chunk(
            &admitted.plugin_id,
            &scope,
            &params.read_id,
            params.offset as u64,
        ),
        Call::ResultReadClose(params) => storage
            .result_close(&admitted.plugin_id, &scope, &params.read_id)
            .map(|()| Value::Null),
        Call::DialogOpen(params) => storage.ui_call(
            &admitted.plugin_id,
            "dialog.open",
            &serde_json::to_value(params).map_err(|_| "INVALID_REQUEST")?,
        ),
        Call::DialogConfirm(params) => storage.ui_call(
            &admitted.plugin_id,
            "dialog.confirm",
            &serde_json::to_value(params).map_err(|_| "INVALID_REQUEST")?,
        ),
        Call::NotificationShow(params) => storage.ui_call(
            &admitted.plugin_id,
            "notification.show",
            &serde_json::to_value(params).map_err(|_| "INVALID_REQUEST")?,
        ),
        Call::ThemeGet(params) => storage.ui_call(
            &admitted.plugin_id,
            "theme.get",
            &serde_json::to_value(params).map_err(|_| "INVALID_REQUEST")?,
        ),
        Call::ThemeList(params) => storage.ui_call(
            &admitted.plugin_id,
            "theme.list",
            &serde_json::to_value(params).map_err(|_| "INVALID_REQUEST")?,
        ),
        Call::ThemePreview(params) => storage.ui_call(
            &admitted.plugin_id,
            "theme.preview",
            &serde_json::to_value(params).map_err(|_| "INVALID_REQUEST")?,
        ),
        Call::ThemeCommit(params) => storage.ui_call(
            &admitted.plugin_id,
            "theme.commit",
            &serde_json::to_value(params).map_err(|_| "INVALID_REQUEST")?,
        ),
        Call::ThemeRollback(params) => storage.ui_call(
            &admitted.plugin_id,
            "theme.rollback",
            &serde_json::to_value(params).map_err(|_| "INVALID_REQUEST")?,
        ),
        Call::ThemeSet(params) => storage.ui_call(
            &admitted.plugin_id,
            "theme.set",
            &serde_json::to_value(params).map_err(|_| "INVALID_REQUEST")?,
        ),
        Call::TasksGet(params) => storage.tasks_get(&admitted.plugin_id, &params.task_id),
        Call::TasksCancel(params) => storage.tasks_cancel(&admitted.plugin_id, &params.task_id),
        Call::TasksList(params) => storage.tasks_list(
            &admitted.plugin_id,
            params.limit as u32,
            params.after.as_deref(),
        ),
        Call::ConfigGet(_) => storage.config_get(&admitted.plugin_id),
        Call::ConfigPatch(params) => storage.config_patch(&admitted.plugin_id, &params.values),
        Call::DocumentCall(params) => {
            storage.service_call(&admitted.plugin_id, "document-engine", &params.payload)
        }
        Call::EnvironmentCall(params) => {
            storage.service_call(&admitted.plugin_id, "unienv", &params.payload)
        }
        Call::ArchiveCall(params) => {
            storage.service_call(&admitted.plugin_id, "archive-extractor", &params.payload)
        }
        Call::BackendCall(params) => {
            storage.backend_call(&admitted.plugin_id, &params.method, &params.args)
        }
        Call::RuntimePing(_) => Ok(Value::String("pong".into())),
        Call::StorageDelete(params) => storage
            .delete(&admitted.plugin_id, &params.key)
            .map(|()| Value::Null),
        Call::StorageBatch(params) => storage
            .batch(&admitted.plugin_id, &params.operations)
            .map(|()| Value::Null),
        Call::StorageList(params) => storage.list(
            &admitted.plugin_id,
            &params.prefix,
            params.limit as u32,
            params.after.as_deref(),
        ),
        Call::StorageKeys(params) => storage.keys(
            &admitted.plugin_id,
            &params.prefix,
            params.limit as u32,
            params.after.as_deref(),
        ),
        Call::StorageReadBegin(params) => {
            storage.read_begin(&admitted.plugin_id, &scope, &params.key)
        }
        Call::StorageReadChunk(params) => storage.read_chunk(
            &admitted.plugin_id,
            &scope,
            &params.read_id,
            params.offset as u64,
        ),
        Call::StorageReadClose(params) => storage
            .read_close(&admitted.plugin_id, &scope, &params.read_id)
            .map(|()| Value::Null),
        Call::StorageWriteBegin(params) => {
            storage.write_begin(&admitted.plugin_id, &scope, &params.writes, &params.deletes)
        }
        Call::StorageWriteChunk(params) => storage.write_chunk(
            &admitted.plugin_id,
            &scope,
            &params.transaction_id,
            &params.key,
            params.offset as u64,
            &params.data,
        ),
        Call::StorageWriteCommit(params) => storage
            .write_commit(&admitted.plugin_id, &scope, &params.transaction_id)
            .map(|()| Value::Null),
        Call::StorageWriteAbort(params) => storage
            .write_abort(&admitted.plugin_id, &scope, &params.transaction_id)
            .map(|()| Value::Null),
        Call::StorageGet(params) => storage
            .get(&admitted.plugin_id, &params.key)
            .map(|value| value.unwrap_or(Value::Null)),
        Call::StorageSet(params) => storage
            .set(&admitted.plugin_id, &params.key, &params.value)
            .map(|()| Value::Null),
    };
    let result = result.and_then(|value| {
        if matches!(
            method.as_str(),
            "document.call" | "environment.call" | "archive.call"
        ) && serde_json::to_vec(&value)
            .map_err(|_| "INVALID_RESPONSE")?
            .len()
            > 49152
        {
            storage.result_begin(&admitted.plugin_id, &scope, &method, &value)
        } else {
            Ok(value)
        }
    });
    session.finish(&id);
    result
}

/// Malformed envelopes fail at transport admission; valid IDs receive correlated replies.
pub fn dispatch_response(
    session: &mut Session,
    storage: &impl Storage,
    raw: &str,
    caller: &str,
    now_ms: u64,
) -> Result<crate::Response, String> {
    let request = crate::validate_typed_request(raw).map_err(str::to_owned)?;
    let method = crate::validate_request(raw).map_err(str::to_owned)?["method"]
        .as_str()
        .ok_or("INVALID_REQUEST")?
        .to_string();
    let contract = crate::contract();
    let version = &contract["versions"]["wireVersion"];
    let response = match dispatch(session, storage, raw, caller, now_ms) {
        Ok(result) if crate::validate_method_result(&method, &result) => {
            serde_json::json!({"wireVersion":version,"requestId":request.request_id,"ok":true,"result":result})
        }
        Ok(_) => {
            serde_json::json!({"wireVersion":version,"requestId":request.request_id,"ok":false,"error":{"code":"INVALID_RESPONSE","message":"INVALID_RESPONSE"}})
        }
        Err(error) => {
            let codes = contract["responses"]["failure"]["properties"]["error"]["properties"]
                ["code"]["enum"]
                .as_array()
                .unwrap();
            let code = if codes.contains(&Value::String(error.clone())) {
                error.as_str()
            } else {
                "INTERNAL_ERROR"
            };
            serde_json::json!({"wireVersion":version,"requestId":request.request_id,"ok":false,"error":{"code":code,"message":code}})
        }
    };
    match crate::validate_response(&response.to_string(), &request.request_id) {
        Ok(reply) => Ok(reply),
        Err(_) => {
            let bounded = serde_json::json!({"wireVersion":version,"requestId":request.request_id,"ok":false,
                "error":{"code":"BUDGET_EXCEEDED","message":"BUDGET_EXCEEDED"}});
            crate::validate_response(&bounded.to_string(), &request.request_id)
                .map_err(str::to_owned)
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn trusted_service_routes_require_exact_capability_and_host_owned_identity() {
        struct ServiceStorage;
        impl Storage for ServiceStorage {
            fn get(&self, _: &str, _: &str) -> Result<Option<Value>, String> {
                Ok(None)
            }
            fn set(&self, _: &str, _: &str, _: &Value) -> Result<(), String> {
                Ok(())
            }
            fn service_call(
                &self,
                owner: &str,
                service: &str,
                payload: &Value,
            ) -> Result<Value, String> {
                Ok(serde_json::json!({"owner":owner,"service":service,"payload":payload}))
            }
        }
        for (method, service) in [
            ("document.call", "document-engine"),
            ("environment.call", "unienv"),
            ("archive.call", "archive-extractor"),
        ] {
            let mut denied = session(service);
            let raw = request(
                0,
                method,
                serde_json::json!({"payload":{"type":"getStatus"}}),
            );
            assert_eq!(
                dispatch(&mut denied, &ServiceStorage, &raw, "main", 1).unwrap_err(),
                "PERMISSION_DENIED"
            );
            let mut admitted = Session::new(
                "a".repeat(64),
                "main".into(),
                service.into(),
                100,
                [format!("trusted:{service}")],
            );
            let value = dispatch(&mut admitted, &ServiceStorage, &raw, "main", 1).unwrap();
            assert_eq!(value["owner"], service);
            assert_eq!(value["service"], service);
            let wrong = request(
                1,
                method,
                serde_json::json!({"owner":"other","payload":{"type":"getStatus"}}),
            );
            assert_eq!(
                dispatch(&mut admitted, &ServiceStorage, &wrong, "main", 1).unwrap_err(),
                "INVALID_REQUEST"
            );
        }
    }

    use super::*;
    use std::{cell::RefCell, collections::HashMap};
    #[derive(Default)]
    struct Memory(RefCell<HashMap<(String, String), Value>>);
    impl Storage for Memory {
        fn get(&self, owner: &str, key: &str) -> Result<Option<Value>, String> {
            Ok(self.0.borrow().get(&(owner.into(), key.into())).cloned())
        }
        fn set(&self, owner: &str, key: &str, value: &Value) -> Result<(), String> {
            if key == "fail" {
                return Err("STORAGE_UNAVAILABLE".into());
            }
            self.0
                .borrow_mut()
                .insert((owner.into(), key.into()), value.clone());
            Ok(())
        }
    }
    fn session(owner: &str) -> Session {
        Session::new(
            "a".repeat(64),
            "main".into(),
            owner.into(),
            100,
            ["storage:read".into(), "storage:write".into()],
        )
    }
    fn request(id: usize, method: &str, params: Value) -> String {
        serde_json::json!({"wireVersion":3,"requestId":format!("r-{id}"),
            "session":"a".repeat(64),"method":method,"params":params})
        .to_string()
    }
    #[test]
    fn same_key_isolated_by_verified_installation_owner() {
        let storage = Memory::default();
        let mut a = session("a");
        let mut b = session("b");
        let set = request(
            0,
            "storage.set",
            serde_json::json!({"key":"note.v1","value":{"text":"中文"}}),
        );
        dispatch(&mut a, &storage, &set, "main", 1).unwrap();
        let get = request(1, "storage.get", serde_json::json!({"key":"note.v1"}));
        assert_eq!(
            dispatch(&mut a, &storage, &get, "main", 1).unwrap(),
            serde_json::json!({"text":"中文"})
        );
        assert_eq!(
            dispatch(&mut b, &storage, &get, "main", 1).unwrap(),
            Value::Null
        );
    }
    #[test]
    fn storage_failure_releases_admission_but_request_cannot_be_replayed() {
        let storage = Memory::default();
        let mut s = session("a");
        for id in 0..40 {
            let raw = request(
                id,
                "storage.set",
                serde_json::json!({"key":"fail","value":null}),
            );
            assert_eq!(
                dispatch(&mut s, &storage, &raw, "main", 1).unwrap_err(),
                "STORAGE_UNAVAILABLE"
            );
        }
        let replay = request(
            0,
            "storage.set",
            serde_json::json!({"key":"fail","value":null}),
        );
        assert_eq!(
            dispatch(&mut s, &storage, &replay, "main", 1).unwrap_err(),
            "REPLAY_DENIED"
        );
    }
    #[test]
    fn structured_denial_correlates_and_oversized_result_fails_closed() {
        let storage = Memory::default();
        let raw = request(0, "storage.get", serde_json::json!({"key":"note.v1"}));
        let reply = dispatch_response(&mut session("a"), &storage, &raw, "other", 1).unwrap();
        let value = serde_json::to_value(reply).unwrap();
        assert_eq!(value["requestId"], "r-0");
        assert_eq!(value["error"]["code"], "SESSION_DENIED");
        storage.0.borrow_mut().insert(
            ("a".into(), "note.v1".into()),
            Value::String("x".repeat(65536)),
        );
        let reply = dispatch_response(&mut session("a"), &storage, &raw, "main", 1).unwrap();
        assert_eq!(
            serde_json::to_value(reply).unwrap()["error"]["code"],
            "BUDGET_EXCEEDED"
        );
        assert_eq!(
            storage
                .0
                .borrow()
                .get(&("a".into(), "note.v1".into()))
                .unwrap()
                .as_str()
                .unwrap()
                .len(),
            65536
        );
    }
}
