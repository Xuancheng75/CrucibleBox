//! Built-in capability providers registered by the trusted host composition.
use super::{error, network_service, save_task_outcome, Execution, Manager};
use crate::db::PluginRow;
use base64::{engine::general_purpose::STANDARD, Engine};
use cruciblebox_next2_protocol::{self as protocol, gateway::Route, Request, RpcError};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

struct Call<'a> {
    owner: &'a str,
    row: &'a PluginRow,
    request: &'a Request,
    execution: Execution<'a>,
}
type Handler = fn(&Arc<Manager>, &Call<'_>) -> Result<Value, RpcError>;

pub struct Platform {
    pub app_handle: Option<tauri::AppHandle>,
    providers: HashMap<&'static str, Handler>,
}
impl Platform {
    pub fn new(app_handle: Option<tauri::AppHandle>) -> Self {
        let providers: [(&str, Handler); 17] = [
            ("capabilities", Manager::capability_service),
            ("tasks", Manager::task_service),
            ("data", Manager::data_service),
            ("resources", Manager::resource_service),
            ("files", Manager::file_service),
            ("network", Manager::network_service),
            ("process", Manager::process_service),
            ("theme", Manager::theme_service),
            ("clipboard", Manager::clipboard_service),
            ("storage", Manager::storage_service),
            ("config", Manager::storage_service),
            ("runtime", Manager::runtime_service),
            ("dialog", Manager::desktop_service),
            ("notification", Manager::desktop_service),
            ("logging", Manager::logging_service),
            ("events", Manager::events_dispatch),
            ("rpc", Manager::rpc_service),
        ];
        let mut methods = HashMap::new();
        for (namespace, handler) in providers {
            for method in protocol::contract()["methods"].as_object().unwrap().keys() {
                if method.split_once('.').map(|value| value.0) == Some(namespace) {
                    methods.insert(method.as_str(), handler);
                }
            }
        }
        assert_eq!(
            methods.len(),
            protocol::contract()["methods"].as_object().unwrap().len(),
            "Unregistered platform method"
        );
        Self {
            app_handle,
            providers: methods,
        }
    }

    /// The protocol gateway has already admitted this owner, permission and context.
    pub(super) fn dispatch(
        &self,
        manager: &Arc<Manager>,
        owner: &str,
        request: &Request,
        execution: Execution<'_>,
    ) -> Result<Value, RpcError> {
        let row = manager.record(owner).map_err(error)?;
        let handler = self
            .providers
            .get(request.method.as_str())
            .ok_or_else(|| error("PLATFORM_METHOD_NOT_CONNECTED".into()))?;
        let value = handler(
            manager,
            &Call {
                owner,
                row: &row,
                request,
                execution,
            },
        )?;

        // The trusted main renderer controller fulfills UI operations.
        if (request.method.starts_with("theme.") && !request.method.starts_with("theme.assets."))
            || matches!(
                request.method.as_str(),
                "dialog.open" | "dialog.confirm" | "notification.show"
            )
            || protocol::validate_method_result(&request.method, &value)
        {
            Ok(value)
        } else {
            Err(error("INVALID_PLATFORM_RESPONSE".into()))
        }
    }
}

impl Manager {
    fn events_dispatch(self: &Arc<Self>, call: &Call<'_>) -> Result<Value, RpcError> {
        self.event_service(call.row, call.request)
    }

    fn rpc_service(self: &Arc<Self>, call: &Call<'_>) -> Result<Value, RpcError> {
        let session = self
            .sessions
            .lock()
            .map_err(|_| error("INTERNAL_ERROR".into()))?
            .get(&call.request.session)
            .map(|entry| entry.session.clone());
        let accepted = match session {
            Some(session) => session
                .lock()
                .map_err(|_| error("INTERNAL_ERROR".into()))?
                .cancel(call.request.params["requestId"].as_str().unwrap()),
            None => false,
        };
        Ok(json!(accepted))
    }
    fn capability_service(self: &Arc<Self>, call: &Call<'_>) -> Result<Value, RpcError> {
        if call.request.method == "capabilities.list" {
            return self
                .list_capabilities(call.owner, &call.request.params)
                .map_err(error);
        }
        if call.request.method == "capabilities.describe" {
            return self
                .describe_capability(call.owner, call.request.params["id"].as_str().unwrap())
                .map_err(error);
        }
        let registry = self.registry().map_err(error)?;
        let binding = registry
            .capability(call.owner, call.request.params["id"].as_str().unwrap())
            .map_err(error)?;
        serde_json::to_value(binding).map_err(|failure| error(failure.to_string()))
    }

    fn task_service(self: &Arc<Self>, call: &Call<'_>) -> Result<Value, RpcError> {
        let Call {
            owner,
            row,
            request,
            execution: execution_context,
        } = *call;
        let params = &request.params;
        let result: Result<Value, String> = match request.method.as_str() {
            "tasks.publishNative" => self.publish_native(row, params, execution_context),
            "tasks.start" => {
                if execution_context.task.is_some() {
                    return Err(error("NESTED_TASK_DENIED".into()));
                }
                let mut execution = request.clone();
                execution.target = serde_json::from_value(params["target"].clone())
                    .map_err(|e| error(e.to_string()))?;
                execution.method = params["method"].as_str().unwrap().to_owned();
                execution.params = params["params"].clone();
                let context = if execution_context.caller.is_some() {
                    "backend"
                } else {
                    "renderer"
                };
                let route = self
                    .registry()
                    .map_err(error)?
                    .resolve(owner, &execution, context)
                    .map_err(error)?;
                let Route::Backend { provider, .. } = &route else {
                    return Err(error("INVALID_TASK_TARGET".into()));
                };
                let resource = format!("next2:{provider}");
                let app = self.services.platform.app_handle.clone();
                let emitter = Arc::new(move |event: &str, payload: Value| {
                    if let Some(app) = &app {
                        use tauri::Emitter;
                        let _ = app.emit(event, payload);
                    }
                });
                self.services.runtime.set_observer(
                    &row.id,
                    crate::host_task::executor_observer(self.db.clone(), emitter, &row.id),
                );
                let task_provider = provider.clone();
                let timeout = params["timeoutMs"].as_u64().map(Duration::from_millis);
                let execution_owner = row.id.clone();
                let manager = self.clone();
                let owner = owner.to_owned();
                let id = self
                    .services
                    .runtime
                    .start(
                        &row.id,
                        &resource,
                        Box::new(move |ctx| {
                            ctx.check_cancelled()?;
                            let result = manager
                                .execute(
                                    &owner,
                                    &execution,
                                    route,
                                    Execution {
                                        task: Some((ctx, &task_provider)),
                                        deadline: timeout.map(|duration| Instant::now() + duration),
                                        ..Execution::default()
                                    },
                                )
                                .map_err(|error| format!("{}: {}", error.code, error.message));
                            let created = manager
                                .services
                                .runtime
                                .get(&execution_owner, &ctx.task_id())
                                .and_then(|value| value["createdAt"].as_u64())
                                .ok_or("TASK_METADATA_UNAVAILABLE")?;
                            save_task_outcome(
                                &*manager.db.lock().map_err(|_| "STORAGE_UNAVAILABLE")?,
                                &execution_owner,
                                &ctx.task_id(),
                                created,
                                &result,
                            )?;
                            result
                        }),
                    )
                    .map_err(error)?;
                Ok(json!({"taskId":id}))
            }
            "tasks.controlState" => {
                let (ctx, expected_provider) = execution_context
                    .task
                    .ok_or_else(|| error("TASK_EXECUTION_REQUIRED".into()))?;
                if params["taskId"] != ctx.task_id() || expected_provider != owner {
                    return Err(error("TASK_AUTHORITY_DENIED".into()));
                }
                if params["executionPaused"] == true {
                    ctx.confirm_paused().map_err(error)?;
                }
                Ok(json!({"paused":ctx.is_paused(), "cancelRequested":ctx.is_cancelled()}))
            }
            "tasks.pause" | "tasks.resume" => {
                let id = params["taskId"].as_str().unwrap();
                if self
                    .task(owner, &row.id, id, false)
                    .map_err(error)?
                    .is_none()
                {
                    return Err(error("TASK_AUTHORITY_DENIED".into()));
                }
                Ok(json!(if request.method == "tasks.pause" {
                    self.services.runtime.pause(&row.id, id)
                } else {
                    self.services.runtime.resume(&row.id, id)
                }))
            }
            "tasks.get" => self
                .task(owner, &row.id, params["taskId"].as_str().unwrap(), true)
                .map(|value| value.unwrap_or(Value::Null)),
            "tasks.list" => {
                let limit = params["limit"].as_u64().unwrap() as usize;
                let after = params["after"].as_str();
                let mut values = Vec::new();
                for value in self.services.runtime.list(&row.id) {
                    let id = value["taskId"]
                        .as_str()
                        .ok_or_else(|| error("INVALID_TASK_RECORD".into()))?;
                    if after.is_some_and(|after| id <= after) {
                        continue;
                    }
                    if let Some(task) = self.task(owner, &row.id, id, false).map_err(error)? {
                        values.push(task);
                    }
                }
                values.sort_by(|a, b| a["taskId"].as_str().cmp(&b["taskId"].as_str()));
                let next = if values.len() > limit {
                    values.truncate(limit);
                    values.last().map(|value| value["taskId"].clone())
                } else {
                    None
                };
                Ok(json!({"items":values,"nextCursor":next}))
            }
            "tasks.cancel" => {
                let id = params["taskId"].as_str().unwrap();
                if self
                    .task(owner, &row.id, id, false)
                    .map_err(error)?
                    .is_none()
                {
                    return Err(error("TASK_AUTHORITY_DENIED".into()));
                }
                let accepted = self.services.runtime.cancel(&row.id, id);
                Ok(
                    json!({"accepted":accepted,"task":self.task(owner, &row.id, id, false).map_err(error)?}),
                )
            }
            _ => Err("PLATFORM_METHOD_NOT_CONNECTED".into()),
        };
        result.map_err(error)
    }

    fn data_service(_: &Arc<Self>, call: &Call<'_>) -> Result<Value, RpcError> {
        Self::data(call.row).map(|path| json!(path)).map_err(error)
    }

    fn resource_service(self: &Arc<Self>, call: &Call<'_>) -> Result<Value, RpcError> {
        self.installed(call.row)
            .and_then(|plugin| self.mounts(call.row, &plugin))
            .map(|mounts| json!(mounts))
            .map_err(error)
    }

    fn file_service(self: &Arc<Self>, call: &Call<'_>) -> Result<Value, RpcError> {
        let Call {
            owner,
            row,
            request,
            execution,
        } = *call;
        let file_caller = execution.caller;
        let params = &request.params;
        let result: Result<Value, String> = match request.method.as_str() {
            "files.stream.open" => self
                .resolve_file(owner, file_caller, params["ref"].as_str().unwrap())
                .and_then(|path| {
                    self.streams
                        .open(owner, file_caller, &request.session, &path)
                }),
            "files.stream.read" => self.streams.read(
                &request.session,
                params["streamId"].as_str().unwrap(),
                params["offset"].as_u64().unwrap(),
                params["length"].as_u64().unwrap(),
            ),
            "files.stream.close" => self
                .streams
                .close(&request.session, params["streamId"].as_str().unwrap()),
            "files.choose" => self.choose_files(row, params, execution),
            "files.info" => self
                .resolve_file(owner, file_caller, params["ref"].as_str().unwrap())
                .and_then(|path| crate::next2_files::info(&path)),
            "files.list" => self
                .resolve_file(owner, file_caller, params["ref"].as_str().unwrap())
                .and_then(|path| crate::next2_files::list(&path)),
            "files.resolve" => self
                .resolve_file(owner, file_caller, params["ref"].as_str().unwrap())
                .map(|path| json!(path)),
            "files.read" => self
                .resolve_file(owner, file_caller, params["ref"].as_str().unwrap())
                .and_then(|path| {
                    crate::next2_files::read(
                        &path,
                        params["offset"].as_u64().unwrap(),
                        params["length"].as_u64().unwrap(),
                    )
                }),
            "files.publish" => self.publish_file(owner, params, execution),
            "files.writeResult" => self.write_output(
                owner,
                params["name"].as_str().unwrap(),
                execution,
                |stage, _| {
                    use std::io::Write;
                    let bytes = STANDARD
                        .decode(params["data"].as_str().unwrap())
                        .map_err(|_| "INVALID_FILE_DATA")?;
                    let mut file =
                        std::fs::File::create(stage).map_err(|error| error.to_string())?;
                    file.write_all(&bytes)
                        .and_then(|()| file.sync_all())
                        .map_err(|error| error.to_string())
                },
            ),
            _ => self.file_operation(owner, row, request, execution),
        };
        result.map_err(error)
    }

    fn network_service(self: &Arc<Self>, call: &Call<'_>) -> Result<Value, RpcError> {
        let Call {
            owner,
            request,
            execution,
            ..
        } = *call;
        let params = &request.params;
        let result: Result<Value, String> = match request.method.as_str() {
            "network.route" => crate::network_policy::current()
                .resolve(params["url"].as_str().unwrap())
                .map(|route| match route.proxy_endpoint {
                    Some(url) => json!({"kind":"proxy","url":url}),
                    None => json!({"kind":"direct"}),
                }),
            "network.fetch" => network_service::fetch(params),
            "network.download" => self.write_output(
                owner,
                params["name"].as_str().unwrap(),
                execution,
                |stage, context| network_service::download(stage, params, execution, context),
            ),
            _ => Err("PLATFORM_METHOD_NOT_CONNECTED".into()),
        };
        result.map_err(error)
    }

    fn process_service(self: &Arc<Self>, call: &Call<'_>) -> Result<Value, RpcError> {
        self.run_process(call.row, &call.request.params, call.execution)
            .map_err(error)
    }

    fn runtime_service(self: &Arc<Self>, call: &Call<'_>) -> Result<Value, RpcError> {
        match call.request.method.as_str() {
            "runtime.list" => self.runtime_status(call.row).map_err(error),
            "runtime.resolve" => {
                let description = self
                    .describe_capability(call.owner, call.request.params["id"].as_str().unwrap())
                    .map_err(error)?;
                if description.get("binding").is_some() && description["kind"] != "runtime" {
                    return Err(error("RUNTIME_CAPABILITY_REQUIRED".into()));
                }
                Ok(description)
            }
            _ => self.storage_service(call),
        }
    }

    fn theme_service(self: &Arc<Self>, call: &Call<'_>) -> Result<Value, RpcError> {
        let Call {
            row,
            request,
            execution,
            ..
        } = *call;
        let file_caller = execution.caller;
        let params = &request.params;
        let result: Result<Value, String> = match request.method.as_str() {
            "theme.assets.import" | "theme.assets.importBundled" => {
                let path = if request.method == "theme.assets.importBundled" {
                    let plugin = self.installed(row).map_err(error)?;
                    crate::theme_assets::bundled_path(
                        plugin.root(),
                        plugin.manifest(),
                        params["path"].as_str().unwrap(),
                    )
                    .map_err(error)?
                } else {
                    self.files
                        .resolve(&row.id, file_caller, params["fileRef"].as_str().unwrap())
                        .map_err(error)?
                };
                let app = self
                    .services
                    .platform
                    .app_handle
                    .as_ref()
                    .ok_or_else(|| error("APPLICATION_UNAVAILABLE".into()))?;
                crate::theme_assets::from_app(app).import(&path, params)
            }
            "theme.assets.get" => {
                let app = self
                    .services
                    .platform
                    .app_handle
                    .as_ref()
                    .ok_or_else(|| error("APPLICATION_UNAVAILABLE".into()))?;
                crate::theme_assets::from_app(app).get(params["id"].as_str().unwrap())
            }
            "theme.assets.list" => {
                let app = self
                    .services
                    .platform
                    .app_handle
                    .as_ref()
                    .ok_or_else(|| error("APPLICATION_UNAVAILABLE".into()))?;
                crate::theme_assets::from_app(app).list(
                    params["after"].as_str(),
                    params["limit"].as_u64().unwrap_or(16) as usize,
                )
            }
            method => return self.ui_service(call, method),
        };
        result.map_err(error)
    }

    fn clipboard_service(_: &Arc<Self>, call: &Call<'_>) -> Result<Value, RpcError> {
        let Call { request, .. } = *call;
        let params = &request.params;
        let result: Result<Value, String> = match request.method.as_str() {
            "clipboard.read" => arboard::Clipboard::new()
                .map_err(|e| format!("clipboard init failed: {e}"))
                .and_then(|mut clipboard| {
                    clipboard
                        .get_text()
                        .map_err(|e| format!("clipboard read failed: {e}"))
                })
                .map(Value::String),
            "clipboard.write" => arboard::Clipboard::new()
                .map_err(|e| format!("clipboard init failed: {e}"))
                .and_then(|mut clipboard| {
                    clipboard
                        .set_text(params["text"].as_str().unwrap())
                        .map_err(|e| format!("clipboard write failed: {e}"))
                })
                .map(|()| Value::Bool(true)),
            _ => Err("PLATFORM_METHOD_NOT_CONNECTED".into()),
        };
        result.map_err(error)
    }

    fn desktop_service(self: &Arc<Self>, call: &Call<'_>) -> Result<Value, RpcError> {
        self.ui_service(call, &call.request.method)
    }

    fn ui_service(self: &Arc<Self>, call: &Call<'_>, method: &str) -> Result<Value, RpcError> {
        let params = &call.request.params;
        if matches!(method, "theme.preview" | "theme.set")
            && params["theme"]["design"]["assets"]
                .as_array()
                .is_some_and(|assets| !assets.is_empty())
        {
            let app = self
                .services
                .platform
                .app_handle
                .as_ref()
                .ok_or_else(|| error("APPLICATION_UNAVAILABLE".into()))?;
            crate::theme_assets::from_app(app)
                .validate_theme(&params["theme"])
                .map_err(error)?;
        }
        Ok(json!({"operation":method,"params":params}))
    }

    fn storage_service(self: &Arc<Self>, call: &Call<'_>) -> Result<Value, RpcError> {
        let request = call.request;
        crate::platform_storage::dispatch(
            &*self
                .db
                .lock()
                .map_err(|_| error("STORAGE_UNAVAILABLE".into()))?,
            &call.row.id,
            &request.session,
            &request.method,
            &request.params,
        )
        .map_err(error)
    }

    fn logging_service(self: &Arc<Self>, call: &Call<'_>) -> Result<Value, RpcError> {
        let params = &call.request.params;
        let runtime = if call.execution.caller.is_none() {
            "renderer"
        } else if call.row.entry_main.ends_with(".exe") {
            "native"
        } else {
            "quickjs"
        };
        let level = params["level"].as_str().unwrap();
        let message = json!({
            "runtime": runtime,
            "requestId": call.request.request_id,
            "taskId": call.execution.task.map(|(context, _)| context.task_id()),
            "code": params["code"],
            "context": params["context"],
            "message": params["message"],
        })
        .to_string();
        self.db
            .lock()
            .map_err(|_| error("STORAGE_UNAVAILABLE".into()))?
            .log_write(&call.row.id, level, &message)
            .map_err(|failure| error(failure.to_string()))?;
        if let Some(app) = &self.services.platform.app_handle {
            use tauri::Emitter;
            let _ = app.emit(
                "plugin:log",
                json!({"pluginId":call.row.id,"level":level,"message":message}),
            );
        }
        Ok(json!(true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_public_platform_namespace_has_a_trusted_provider() {
        let platform = Platform::new(None);
        let namespaces: std::collections::HashSet<_> = protocol::contract()["methods"]
            .as_object()
            .unwrap()
            .keys()
            .map(|method| method.split_once('.').unwrap().0)
            .collect();
        assert_eq!(namespaces, platform.providers.keys().copied().collect());
    }
}
