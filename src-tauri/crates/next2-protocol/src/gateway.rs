//! Routing identity is supplied by the host, never by domain parameters.
use crate::{compatible, contract, validate_request, Manifest, Request, Target};
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub struct Installed {
    pub manifest: Manifest,
    pub enabled: bool,
}
pub struct Registry {
    plugins: HashMap<String, Installed>,
}
#[derive(Debug, PartialEq)]
pub enum Route {
    Platform,
    Backend {
        initiator: String,
        provider: String,
        service: Option<String>,
    },
}
impl Registry {
    /// Inputs have passed package validation. This only checks the installed dependency graph.
    pub fn new(plugins: impl IntoIterator<Item = Installed>) -> Result<Self, String> {
        let mut map = HashMap::new();
        for plugin in plugins {
            if map.insert(plugin.manifest.id.clone(), plugin).is_some() {
                return Err("DUPLICATE_PLUGIN".into());
            }
        }
        let registry = Self { plugins: map };
        let mut visited = HashSet::new();
        for (id, plugin) in &registry.plugins {
            if plugin.enabled {
                registry.visit(id, &mut HashSet::new(), &mut visited)?;
            }
        }
        Ok(registry)
    }
    fn enabled(&self, id: &str) -> Result<&Installed, String> {
        self.plugins
            .get(id)
            .filter(|plugin| plugin.enabled)
            .ok_or_else(|| "DEPENDENCY_UNAVAILABLE".into())
    }
    fn service(
        &self,
        caller: &Manifest,
        id: &str,
        method: Option<&str>,
    ) -> Result<&Installed, String> {
        let dependency = caller
            .dependencies
            .iter()
            .find(|item| item.id == id)
            .ok_or("DEPENDENCY_UNDECLARED")?;
        let provider = self.enabled(&dependency.provider_id)?;
        let export = provider
            .manifest
            .exports
            .iter()
            .find(|item| item.id == id)
            .ok_or("SERVICE_UNAVAILABLE")?;
        if !compatible(&export.version, &dependency.version)? {
            return Err("DEPENDENCY_INCOMPATIBLE".into());
        }
        if method.is_some_and(|name| !export.methods.iter().any(|item| item == name)) {
            return Err("METHOD_UNAVAILABLE".into());
        }
        Ok(provider)
    }
    fn visit(
        &self,
        id: &str,
        stack: &mut HashSet<String>,
        visited: &mut HashSet<String>,
    ) -> Result<(), String> {
        if visited.contains(id) {
            return Ok(());
        }
        if !stack.insert(id.to_owned()) {
            return Err("DEPENDENCY_CYCLE".into());
        }
        let plugin = self.enabled(id)?;
        for dependency in &plugin.manifest.dependencies {
            self.service(&plugin.manifest, &dependency.id, None)?;
            self.visit(&dependency.provider_id, stack, visited)?;
        }
        stack.remove(id);
        visited.insert(id.to_owned());
        Ok(())
    }
    pub fn dependents(&self, provider: &str) -> Vec<String> {
        let mut affected = HashSet::from([provider.to_owned()]);
        loop {
            let before = affected.len();
            for (id, plugin) in &self.plugins {
                if plugin.enabled
                    && plugin
                        .manifest
                        .dependencies
                        .iter()
                        .any(|item| affected.contains(&item.provider_id))
                {
                    affected.insert(id.clone());
                }
            }
            if affected.len() == before {
                break;
            }
        }
        affected.remove(provider);
        let mut result: Vec<_> = affected.into_iter().collect();
        result.sort();
        result
    }
    pub fn resolve(&self, caller_id: &str, target: &Target, method: &str) -> Result<Route, String> {
        let caller = &self.enabled(caller_id)?.manifest;
        let (provider, service) = match target {
            Target::PlatformTarget(_) => return Ok(Route::Platform),
            Target::BackendTarget(_) => (self.enabled(caller_id)?, None),
            Target::ServiceTarget(target) => (
                self.service(caller, &target.id, Some(method))?,
                Some(target.id.clone()),
            ),
        };
        if provider.manifest.backend.is_none() {
            return Err("BACKEND_UNAVAILABLE".into());
        }
        Ok(Route::Backend {
            initiator: caller_id.into(),
            provider: provider.manifest.id.clone(),
            service,
        })
    }
}
pub struct Session {
    manifest: Manifest,
    token: String,
    binding: String,
    context: String,
    expires_at_ms: u64,
    seen: HashSet<String>,
    inflight: HashSet<String>,
}
pub struct Admitted {
    pub owner: String,
    pub request: Request,
    pub route: Route,
}
impl Session {
    /// The installation, token, context and WebView/process binding are host-owned inputs.
    pub fn new(
        manifest: Manifest,
        token: String,
        binding: String,
        context: String,
        expires_at_ms: u64,
    ) -> Self {
        Self {
            manifest,
            token,
            binding,
            context,
            expires_at_ms,
            seen: HashSet::new(),
            inflight: HashSet::new(),
        }
    }
    pub fn admit(
        &mut self,
        registry: &Registry,
        raw: &str,
        binding: &str,
        now_ms: u64,
    ) -> Result<Admitted, String> {
        let request = validate_request(raw)?;
        if request.session != self.token || binding != self.binding {
            return Err("SESSION_DENIED".into());
        }
        if now_ms >= self.expires_at_ms {
            return Err("SESSION_EXPIRED".into());
        }
        if registry.enabled(&self.manifest.id)?.manifest != self.manifest {
            return Err("SESSION_REPLACED".into());
        }
        let route = registry.resolve(&self.manifest.id, &request.target, &request.method)?;
        if self.context == "backend" && matches!(request.target, Target::BackendTarget(_)) {
            return Err("REENTRANT_BACKEND_CALL".into());
        }
        if route == Route::Platform {
            let method = &contract()["methods"][&request.method];
            if !method["contexts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == &self.context)
            {
                return Err("CONTEXT_DENIED".into());
            }
            if method["permission"].as_str().is_some_and(|permission| {
                !self.manifest.permissions.iter().any(|v| v == permission)
            }) {
                return Err("PERMISSION_DENIED".into());
            }
        }
        if self.seen.contains(&request.request_id) {
            return Err("REPLAY_DENIED".into());
        }
        if self.inflight.len() >= crate::BUDGET_INFLIGHT {
            return Err("BUSY".into());
        }
        if self.seen.len() >= crate::BUDGET_SESSION_REQUESTS {
            return Err("SESSION_EXHAUSTED".into());
        }
        self.seen.insert(request.request_id.clone());
        self.inflight.insert(request.request_id.clone());
        Ok(Admitted {
            owner: self.manifest.id.clone(),
            request,
            route,
        })
    }
    pub fn finish(&mut self, id: &str) {
        self.inflight.remove(id);
    }
}

/// Transport ownership only; task state and terminal transitions remain in task-runtime.
pub struct TaskAuthority {
    initiator: String,
    provider: String,
    execution_session: String,
}
impl TaskAuthority {
    pub fn new(initiator: String, provider: String, execution_session: String) -> Self {
        Self {
            initiator,
            provider,
            execution_session,
        }
    }
    pub fn may_read_or_cancel(&self, caller: &str) -> bool {
        caller == self.initiator
    }
    pub fn may_report(&self, provider: &str, execution_session: &str) -> bool {
        provider == self.provider && execution_session == self.execution_session
    }
}
