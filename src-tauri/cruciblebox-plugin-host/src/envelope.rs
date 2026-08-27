// 信封 v2 校验（对等 shared/plugin-backend-rpc.ts 的 validate 语义）
// sidecar 侧职责：
// - 收到宿主请求（WORKER_METHODS）时校验信封结构后执行
// - 向宿主发请求（HOST_METHODS）时构造合法信封
// 校验规则移植自 TS：token/requestId 正则、kind 三态、逐方法 params 校验、
// payload 预算（256KB/深度16/节点4096/数组512/对象键256/字符串64KB）。

use serde_json::{json, Value};
pub const RPC_VERSION: i64 = 2;

/// token 正则（32-128 位，[A-Za-z0-9_-]）
pub fn valid_token(token: &str) -> bool {
    if token.len() < 32 || token.len() > 128 {
        return false;
    }
    token
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// requestId 正则（1-64 位，[A-Za-z0-9._:-]）
pub fn valid_request_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 64 {
        return false;
    }
    id.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == ':' || c == '-')
}

pub fn is_known_error_code(code: &str) -> bool {
    matches!(
        code,
        "INVALID_MESSAGE" | "NOT_ALLOWED" | "TIMEOUT" | "DISPOSED" | "INTERNAL_ERROR"
    )
}

/// 宿主→sidecar 的 4 个 worker 方法
pub const WORKER_METHODS: [&str; 4] = [
    "lifecycle.initialize",
    "lifecycle.dispose",
    "plugin.message",
    "host.event",
];

/// sidecar→宿主的 host 方法（契约清单；宿主侧消费，sidecar 出站白名单）
/// v1.9.15：新增 clipboard.read / clipboard.write / system.info
pub const HOST_METHODS: [&str; 22] = [
    "db.query",
    "db.execute",
    "storage.get",
    "storage.set",
    "storage.delete",
    "storage.list",
    "storage.batch",
    "log.write",
    "notification.show",
    "dialog.open",
    "network.fetch",
    "file.read",
    "file.write",
    "shortcut.register",
    "shortcut.unregister",
    "event.emit",
    "event.subscribe",
    "event.unsubscribe",
    "trusted.invoke",
    "clipboard.read",
    "clipboard.write",
    "system.info",
];

/// 请求信封：{v:2, kind:'request', token, requestId, method, params}
pub fn make_request(token: &str, request_id: &str, method: &str, params: Value) -> Value {
    json!({ "v": 2, "kind": "request", "token": token, "requestId": request_id, "method": method, "params": params })
}

/// 响应信封：{v:2, kind:'response', token, requestId, ok:true, result} 或 ok:false,error
/// token 必填（对等 createPluginBackendRpcResponse / validatePluginBackendRpcEnvelope）
pub fn make_response(token: &str, request_id: &str, result: Value) -> Value {
    json!({ "v": 2, "kind": "response", "token": token, "requestId": request_id, "ok": true, "result": result })
}

pub fn make_error(token: &str, request_id: &str, code: &str, message: &str) -> Value {
    json!({ "v": 2, "kind": "response", "token": token, "requestId": request_id, "ok": false, "error": { "code": code, "message": message } })
}

/// 校验一个宿主→sidecar 的请求信封。返回 (requestId, method, params)。
pub fn validate_worker_request(
    envelope: &Value,
    token: &str,
) -> Result<(String, String, Value), String> {
    if !envelope.is_object() {
        return Err("envelope must be an object".into());
    }
    let obj = envelope.as_object().unwrap();
    if obj.get("v").and_then(Value::as_i64) != Some(RPC_VERSION) {
        return Err(format!("unsupported version, expected {RPC_VERSION}"));
    }
    if obj.get("kind").and_then(Value::as_str) != Some("request") {
        return Err("expected kind=request".into());
    }
    let tok = obj.get("token").and_then(Value::as_str).ok_or("missing token")?;
    if !valid_token(tok) {
        return Err("invalid token format".into());
    }
    if tok != token {
        return Err("token mismatch".into());
    }
    let request_id = obj
        .get("requestId")
        .and_then(Value::as_str)
        .ok_or("missing requestId")?;
    if !valid_request_id(request_id) {
        return Err("invalid requestId format".into());
    }
    let method = obj.get("method").and_then(Value::as_str).ok_or("missing method")?;
    if !WORKER_METHODS.contains(&method) {
        return Err(format!("unknown worker method: {method}"));
    }
    let params = obj.get("params").cloned().unwrap_or(Value::Null);
    check_payload_budget(&params)?;
    Ok((request_id.to_string(), method.to_string(), params))
}

/// 校验一个宿主→sidecar 的响应信封（匹配 pending request）。返回 (ok, result/error)。
pub fn validate_host_response(
    envelope: &Value,
    request_id: &str,
) -> Result<(bool, Value, Option<String>), String> {
    if !envelope.is_object() {
        return Err("envelope must be an object".into());
    }
    let obj = envelope.as_object().unwrap();
    if obj.get("v").and_then(Value::as_i64) != Some(RPC_VERSION) {
        return Err(format!("unsupported version, expected {RPC_VERSION}"));
    }
    let tok = obj.get("token").and_then(Value::as_str).ok_or("missing token")?;
    if !valid_token(tok) {
        return Err("invalid token format".into());
    }
    if obj.get("kind").and_then(Value::as_str) != Some("response") {
        return Err("expected kind=response".into());
    }
    let rid = obj.get("requestId").and_then(Value::as_str).ok_or("missing requestId")?;
    if rid != request_id {
        return Err("response requestId mismatch".into());
    }
    let ok = obj.get("ok").and_then(Value::as_bool).ok_or("missing ok")?;
    if ok {
        Ok((true, obj.get("result").cloned().unwrap_or(Value::Null), None))
    } else {
        let err = obj
            .get("error")
            .ok_or("missing error object")?
            .clone();
        let code = err.get("code").and_then(Value::as_str).unwrap_or("INTERNAL_ERROR");
        if !is_known_error_code(code) {
            return Err("unknown error code".into());
        }
        let message = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if message.len() > 4096 {
            return Err("error message too long".into());
        }
        Ok((false, Value::Null, Some(message)))
    }
}

/// payload 预算校验（对等 packages/openbox-rpc/payload-budget.ts）
pub fn check_payload_budget(value: &Value) -> Result<(), String> {
    let serialized = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if serialized.len() > 256 * 1024 {
        return Err("payload exceeds 256KB".into());
    }
    check_depth(value, 0, 16)?;
    Ok(())
}

fn check_depth(value: &Value, depth: usize, max_depth: usize) -> Result<(), String> {
    if depth > max_depth {
        return Err("payload depth exceeds 16".into());
    }
    match value {
        Value::Array(items) => {
            if items.len() > 512 {
                return Err("payload array exceeds 512".into());
            }
            for item in items {
                check_depth(item, depth + 1, max_depth)?;
            }
        }
        Value::Object(map) => {
            if map.len() > 256 {
                return Err("payload object exceeds 256 keys".into());
            }
            for (k, v) in map {
                if k.len() > 256 {
                    return Err("payload key too long".into());
                }
                check_depth(v, depth + 1, max_depth)?;
            }
        }
        Value::String(s) => {
            if s.len() > 64 * 1024 {
                return Err("payload string exceeds 64KB".into());
            }
        }
        Value::Number(_) | Value::Bool(_) | Value::Null => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_rules() {
        assert!(valid_token(&"a".repeat(32)));
        assert!(valid_token(&"A-Z_0-9-".repeat(10)));
        assert!(!valid_token(&"short"));
        assert!(!valid_token(&"x".repeat(129)));
        assert!(!valid_token("has space 123456789012345678901234567890"));
    }

    #[test]
    fn request_id_rules() {
        assert!(valid_request_id("abc-123._:"));
        assert!(!valid_request_id(""));
        assert!(!valid_request_id("has space!!"));
        assert!(!valid_request_id(&"x".repeat(65)));
    }

    #[test]
    fn worker_request_validation() {
        let token = "t".repeat(32);
        let req = make_request(&token, "r1", "lifecycle.initialize", json!({"id": "p1"}));
        let (rid, method, _) = validate_worker_request(&req, &token).unwrap();
        assert_eq!(rid, "r1");
        assert_eq!(method, "lifecycle.initialize");

        // 错误 token
        let bad_token = "x".repeat(32);
        let bad = make_request(&bad_token, "r1", "lifecycle.initialize", json!({}));
        assert!(validate_worker_request(&bad, &token).is_err());
        // 未知方法
        let unknown = make_request(&token, "r1", "db.query", json!({}));
        assert!(validate_worker_request(&unknown, &token).is_err());
    }

    #[test]
    fn host_response_validation() {
        let token = "t".repeat(32);
        let ok_env = make_response(&token, "r1", json!([1, 2]));
        assert!(validate_host_response(&ok_env, "r1").unwrap().0);
        let err_env = make_error(&token, "r2", "TIMEOUT", "timed out");
        let (ok, _, msg) = validate_host_response(&err_env, "r2").unwrap();
        assert!(!ok);
        assert_eq!(msg.unwrap(), "timed out");
        // requestId 不匹配
        assert!(validate_host_response(&ok_env, "r-wrong").is_err());
        // 缺 token → 拒绝
        let no_token = json!({"v": 2, "kind": "response", "requestId": "r1", "ok": true, "result": null});
        assert!(validate_host_response(&no_token, "r1").is_err());
    }

    #[test]
    fn payload_budget() {
        assert!(check_payload_budget(&json!({"a": [1, 2, {"b": "c"}]})).is_ok());
        // 超深
        let mut deep = json!(1);
        for _ in 0..20 {
            deep = json!([deep]);
        }
        assert!(check_payload_budget(&deep).is_err());
        // 超大数组
        let big = json!(vec![0u8; 600]);
        assert!(check_payload_budget(&big).is_err());
    }
}
