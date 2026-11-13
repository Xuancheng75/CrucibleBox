//! Discovery and availability use the same registry as admission and execution.
use super::{error, Instance, Manager};
use cruciblebox_next2_protocol::{self as protocol, gateway::Registry, CapabilityBinding};
use serde_json::{json, Value};
use std::sync::atomic::Ordering;

impl Manager {
    fn capability_description(
        &self,
        registry: &Registry,
        owner: &str,
        id: &str,
    ) -> Result<Value, String> {
        let binding = match registry.capability(owner, id) {
            Ok(binding) => binding,
            Err(failure) => {
                return Ok(
                    json!({"availability":{"id":id,"state":"unavailable","error":error(failure)}}),
                )
            }
        };
        self.binding_description(registry, binding)
    }
    fn binding_description(
        &self,
        registry: &Registry,
        binding: &CapabilityBinding,
    ) -> Result<Value, String> {
        let id = binding.export.id.as_str();
        let mut availability = json!({"id":id,"state":"registered","providerId":binding.provider_id,"providerVersion":binding.provider_version,"version":binding.export.version});
        let mut description =
            json!({"availability":availability,"binding":binding,"kind":"plugin"});
        if !registry.provider_enabled(&binding.provider_id) {
            availability["state"] = json!("stopped");
            description["availability"] = availability;
            return Ok(description);
        }
        if registry.is_platform_provider(&binding.provider_id) {
            description["kind"] = json!("host");
            availability["state"] = json!(if self.closing.load(Ordering::SeqCst) {
                "stopping"
            } else {
                "ready"
            });
            let mut methods = Vec::new();
            for method in &binding.export.methods {
                let name = method.name();
                let rule = &protocol::contract()["methods"][name];
                methods.push(json!({"name":name,"permission":rule["permission"],"contexts":rule["contexts"],"paramsSchema":format!("#/methods/{name}/params"),"resultSchema":format!("#/methods/{name}/result")}));
            }
            description["methodContracts"] = json!(methods);
        } else {
            let row = self.record(&binding.provider_id)?;
            if row.version != binding.provider_version {
                return Err("PROVIDER_CHANGED".into());
            }
            let workers = self.workers.lock().map_err(|_| "INTERNAL_ERROR")?;
            let instances: Vec<_> = workers
                .iter()
                .filter(|((owner, _), _)| owner == &row.id)
                .map(|(_, instance)| instance)
                .collect();
            let state = if instances
                .iter()
                .any(|instance| matches!(instance, Instance::Ready(_)))
            {
                "ready"
            } else if !instances.is_empty() {
                "starting"
            } else {
                "registered"
            };
            availability["state"] = json!(state);
            drop(workers);
            if state == "registered" {
                if let Some(code) = self
                    .provider_failures
                    .lock()
                    .map_err(|_| "INTERNAL_ERROR")?
                    .get(&row.id)
                {
                    availability["state"] = json!("failed");
                    availability["error"] = json!(error(code.clone()));
                }
            }
            if self.closing.load(Ordering::SeqCst)
                || self
                    .maintenance
                    .lock()
                    .map_err(|_| "INTERNAL_ERROR")?
                    .contains(&row.id)
            {
                availability["state"] = json!("stopping");
            }
            let manifest = crate::next2_install::load_manifest(
                &*self.db.lock().map_err(|_| "STORAGE_UNAVAILABLE")?,
                std::path::Path::new(&row.installed_path),
            )?;
            if let Some(runtime) = manifest
                .runtime_capabilities
                .as_deref()
                .unwrap_or_default()
                .iter()
                .find(|runtime| runtime.capability_id == id)
            {
                description["kind"] = json!("runtime");
                description["methodContracts"] = json!(binding.export.methods.iter().filter_map(|method| {
                    let name = method.name();
                    let (params, result, permission) = match name {
                        "run" => ("#/$defs/RuntimeRunParams", "#/$defs/ProcessResult", Some("runtime:execute")),
                        "environment" => ("#/$defs/Empty", "#/methods/runtime.list/result", None),
                        _ => return None,
                    };
                    Some(json!({"name":name,"permission":permission,"contexts":["renderer","backend"],"paramsSchema":params,"resultSchema":result}))
                }).collect::<Vec<_>>());
                match registry.runtime_environment(&binding.provider_id, id) {
                    Ok(Some(environment)) => {
                        description["environment"] = json!(environment);
                    }
                    Err(failure) => {
                        availability["state"] = json!("failed");
                        availability["error"] = json!(error(failure));
                    }
                    Ok(None) => {}
                }
                let status = self.runtime_status(&row)?;
                if !status
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|tool| tool["id"] == runtime.tool_id && tool["state"] == "ready")
                {
                    availability["state"] = json!("unavailable");
                    availability["error"] = json!(error("PROCESS_RESOURCE_MISSING".into()));
                }
            }
        }
        description["availability"] = availability;
        Ok(description)
    }

    pub(super) fn describe_capability(&self, owner: &str, id: &str) -> Result<Value, String> {
        self.capability_description(&self.registry()?, owner, id)
    }
    pub(super) fn list_providers(&self, params: &Value) -> Result<Value, String> {
        let registry = self.registry()?;
        let after = params["after"].as_str().unwrap_or("");
        let limit = params["limit"].as_u64().unwrap() as usize;
        let mut bindings: Vec<_> = registry
            .providers()
            .into_iter()
            .map(|binding| {
                (
                    format!("{}|{}", binding.export.id, binding.provider_id),
                    binding,
                )
            })
            .filter(|(key, binding)| {
                key.as_str() > after
                    && params["id"]
                        .as_str()
                        .is_none_or(|id| binding.export.id == id)
            })
            .collect();
        bindings.sort_by(|a, b| a.0.cmp(&b.0));
        let mut items = Vec::new();
        for (_, binding) in bindings.iter().take(limit) {
            items.push(self.binding_description(&registry, binding)?["availability"].take());
        }
        let mut page = json!({"items":items});
        if bindings.len() > limit {
            page["next"] = json!(bindings[limit - 1].0);
        }
        Ok(page)
    }

    pub(super) fn list_capabilities(&self, owner: &str, params: &Value) -> Result<Value, String> {
        let registry = self.registry()?;
        let ids = registry.capability_ids(owner)?;
        let after = params["after"].as_str().unwrap_or("");
        let limit = params["limit"].as_u64().unwrap() as usize;
        let selected: Vec<_> = ids
            .iter()
            .filter(|id| id.as_str() > after)
            .take(limit + 1)
            .collect();
        let mut items = Vec::new();
        for id in selected.iter().take(limit) {
            let mut description = self.capability_description(&registry, owner, id)?;
            items.push(description["availability"].take());
        }
        let mut page = json!({"items":items});
        if selected.len() > limit {
            page["next"] = json!(selected[limit - 1]);
        }
        Ok(page)
    }
}
