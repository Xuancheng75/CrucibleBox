//! Routing identity is supplied by the host, never by domain parameters.
use crate::{
    compatible, contract, schema, validate_request, CapabilityBinding, Manifest, Request,
    ServiceMethod, Target,
};
use semver::Version;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Used by session admission and by the inner operation of a host-owned task.
pub fn authorize_platform(manifest: &Manifest, name: &str, context: &str) -> Result<(), String> {
    authorize_platform_context(manifest, name, context, false)
}
pub fn authorize_platform_task(
    manifest: &Manifest,
    name: &str,
    context: &str,
) -> Result<(), String> {
    authorize_platform_context(manifest, name, context, true)
}
fn authorize_platform_context(
    manifest: &Manifest,
    name: &str,
    context: &str,
    task: bool,
) -> Result<(), String> {
    let method = contract()["methods"]
        .get(name)
        .ok_or("INVALID_PLATFORM_CALL")?;
    let contexts = if task {
        method.get("taskContexts").unwrap_or(&method["contexts"])
    } else {
        &method["contexts"]
    };
    if !contexts
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == context)
    {
        return Err("CONTEXT_DENIED".into());
    }
    if method["permission"]
        .as_str()
        .is_some_and(|permission| !manifest.permissions.iter().any(|value| value == permission))
    {
        return Err("PERMISSION_DENIED".into());
    }
    Ok(())
}

#[derive(Clone)]
pub struct Installed {
    pub manifest: Manifest,
    pub enabled: bool,
}
pub struct Registry {
    plugins: HashMap<String, Installed>,
    capabilities: HashMap<String, Vec<(Version, CapabilityBinding)>>,
    platform: HashMap<String, CapabilityBinding>,
    runtimes: HashMap<(String, String), Result<crate::RuntimeEnvironment, String>>,
}
#[derive(Debug, PartialEq)]
pub enum Route {
    Platform,
    Backend {
        initiator: String,
        provider: String,
        provider_version: String,
        service: Option<ServiceCall>,
    },
}

#[derive(Debug, PartialEq)]
pub struct ServiceCall {
    pub id: String,
    result: Option<Value>,
    runtime_result: Option<Value>,
}

impl ServiceCall {
    pub fn validate_result(&self, value: &Value) -> Result<(), String> {
        if self
            .result
            .iter()
            .chain(self.runtime_result.iter())
            .any(|rule| !schema::matches(rule, value))
        {
            return Err("INVALID_CAPABILITY_RESULT".into());
        }
        Ok(())
    }
}

impl Registry {
    /// Index approved manifests. A broken dependency chain is local to its consumers.
    pub fn new(plugins: impl IntoIterator<Item = Installed>) -> Result<Self, String> {
        let mut map = HashMap::new();
        let mut capabilities: HashMap<String, Vec<(Version, CapabilityBinding)>> = HashMap::new();
        let mut platform = HashMap::new();
        let builtins = &contract()["capabilities"]["builtins"];
        let provider = builtins["providerId"]
            .as_str()
            .expect("built-in provider identity");
        let mut namespaces: HashMap<&str, Vec<&str>> = HashMap::new();
        for method in contract()["methods"]
            .as_object()
            .expect("platform methods")
            .keys()
        {
            let namespace = method.split_once('.').expect("namespaced method").0;
            namespaces.entry(namespace).or_default().push(method);
        }
        for (namespace, mut methods) in namespaces {
            methods.sort_unstable();
            let binding: CapabilityBinding = serde_json::from_value(serde_json::json!({
                "providerId": provider,
                "providerVersion": builtins["providerVersion"],
                "lifecycle": "background",
                "export": {
                    "id": format!("host-{namespace}"),
                    "version": builtins["interfaceVersion"],
                    "methods": methods,
                    "interchangeable": true
                }
            }))
            .expect("generated built-in capability");
            for method in methods {
                platform.insert(method.into(), binding.clone());
            }
            capabilities
                .entry(binding.export.id.clone())
                .or_default()
                .push((
                    Version::parse(&binding.export.version).expect("built-in interface version"),
                    binding,
                ));
        }
        for plugin in plugins {
            if plugin.manifest.id == provider {
                return Err("RESERVED_PROVIDER_ID".into());
            }
            {
                for export in &plugin.manifest.exports {
                    let version = Version::parse(&export.version).map_err(|_| "INVALID_VERSION")?;
                    capabilities.entry(export.id.clone()).or_default().push((
                        version,
                        CapabilityBinding {
                            provider_id: plugin.manifest.id.clone(),
                            provider_version: plugin.manifest.version.clone(),
                            lifecycle: if plugin.manifest.background {
                                "background"
                            } else {
                                "on-demand"
                            }
                            .into(),
                            export: export.clone(),
                        },
                    ));
                }
            }
            if map.insert(plugin.manifest.id.clone(), plugin).is_some() {
                return Err("DUPLICATE_PLUGIN".into());
            }
        }
        for candidates in capabilities.values_mut() {
            candidates.sort_by(|(left, a), (right, b)| {
                right
                    .cmp(left)
                    .then_with(|| a.provider_id.cmp(&b.provider_id))
            });
        }
        Ok(Self {
            plugins: map,
            capabilities,
            platform,
            runtimes: HashMap::new(),
        })
    }
    pub fn is_platform_provider(&self, provider: &str) -> bool {
        provider
            == contract()["capabilities"]["builtins"]["providerId"]
                .as_str()
                .unwrap()
    }
    pub fn with_runtimes(
        mut self,
        runtimes: HashMap<(String, String), Result<crate::RuntimeEnvironment, String>>,
    ) -> Self {
        self.runtimes = runtimes;
        self
    }
    pub fn runtime_environment(
        &self,
        provider: &str,
        capability: &str,
    ) -> Result<Option<&crate::RuntimeEnvironment>, String> {
        self.runtimes
            .get(&(provider.into(), capability.into()))
            .map(|value| value.as_ref().map_err(Clone::clone))
            .transpose()
    }
    pub fn capability_ids(&self, caller: &str) -> Result<Vec<String>, String> {
        let plugin = self.enabled(caller)?;
        let mut ids: Vec<_> = self
            .platform
            .values()
            .map(|item| item.export.id.clone())
            .chain(
                plugin
                    .manifest
                    .dependencies
                    .iter()
                    .map(|item| item.id.clone()),
            )
            .collect();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }
    pub fn platform_binding(&self, method: &str) -> Result<&CapabilityBinding, String> {
        self.platform
            .get(method)
            .ok_or_else(|| "METHOD_UNAVAILABLE".into())
    }
    pub fn provider_enabled(&self, id: &str) -> bool {
        self.is_platform_provider(id) || self.plugins.get(id).is_some_and(|plugin| plugin.enabled)
    }
    pub fn providers(&self) -> Vec<&CapabilityBinding> {
        self.capabilities
            .values()
            .flat_map(|values| values.iter().map(|(_, binding)| binding))
            .collect()
    }
    fn enabled(&self, id: &str) -> Result<&Installed, String> {
        self.plugins
            .get(id)
            .filter(|plugin| plugin.enabled)
            .ok_or_else(|| "DEPENDENCY_UNAVAILABLE".into())
    }
    fn select(
        &self,
        caller: &str,
        dependency: &crate::ServiceDependency,
    ) -> Result<&CapabilityBinding, String> {
        if let Some(provider) = &dependency.provider_id {
            if !self.is_platform_provider(provider) {
                self.enabled(provider)?;
            }
        }
        let mut found = false;
        let runtime = self
            .enabled(caller)?
            .manifest
            .runtime_requirements
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|runtime| runtime.capability_id == dependency.id);
        let mut runtime_failure = None;
        for (_, binding) in self.capabilities.get(&dependency.id).into_iter().flatten() {
            if !self.provider_enabled(&binding.provider_id)
                || binding.provider_id == caller
                || dependency.provider_id.as_ref().map_or_else(
                    || binding.export.interchangeable != Some(true),
                    |provider| provider != &binding.provider_id,
                )
            {
                continue;
            }
            found = true;
            if compatible(&binding.export.version, &dependency.version)? {
                if let Some(requirement) = runtime {
                    let Ok(Some(environment)) =
                        self.runtime_environment(&binding.provider_id, &binding.export.id)
                    else {
                        runtime_failure = Some("RUNTIME_UNAVAILABLE");
                        continue;
                    };
                    if environment.engine != requirement.engine
                        || !compatible(&environment.version, &requirement.version)?
                    {
                        runtime_failure = Some("RUNTIME_VERSION_INCOMPATIBLE");
                        continue;
                    }
                }
                return Ok(binding);
            }
        }
        Err(if let Some(failure) = runtime_failure {
            failure
        } else if found {
            "DEPENDENCY_INCOMPATIBLE"
        } else {
            "SERVICE_UNAVAILABLE"
        }
        .into())
    }
    pub fn capability(&self, caller: &str, id: &str) -> Result<&CapabilityBinding, String> {
        let plugin = self.enabled(caller)?;
        let dependency = plugin
            .manifest
            .dependencies
            .iter()
            .find(|item| item.id == id);
        // API 7 declares the baseline built-ins. Explicit dependencies can pin
        // their interface range/provider without changing existing API 7 calls.
        if dependency.is_none() {
            return self
                .platform
                .values()
                .find(|item| item.export.id == id)
                .ok_or_else(|| "DEPENDENCY_UNDECLARED".into());
        }
        let dependency = dependency.unwrap();
        let binding = self.select(caller, dependency)?;
        if !self.is_platform_provider(&binding.provider_id) {
            self.visit(
                &binding.provider_id,
                &mut HashSet::from([caller.to_owned()]),
                &mut HashSet::new(),
                &mut Vec::new(),
            )?;
        }
        Ok(binding)
    }
    pub fn validate_plugin(&self, id: &str) -> Result<(), String> {
        self.activation_order(id).map(|_| ())
    }
    pub fn background_plugins(&self) -> Vec<&str> {
        let mut plugins: Vec<_> = self
            .plugins
            .values()
            .filter(|plugin| plugin.enabled && plugin.manifest.background)
            .map(|plugin| plugin.manifest.id.as_str())
            .collect();
        plugins.sort_unstable();
        plugins
    }
    pub fn activation_order(&self, id: &str) -> Result<Vec<&Manifest>, String> {
        let mut order = Vec::new();
        self.visit(id, &mut HashSet::new(), &mut HashSet::new(), &mut order)?;
        Ok(order)
    }
    fn visit<'a>(
        &'a self,
        id: &str,
        stack: &mut HashSet<String>,
        visited: &mut HashSet<String>,
        order: &mut Vec<&'a Manifest>,
    ) -> Result<(), String> {
        if visited.contains(id) {
            return Ok(());
        }
        if !stack.insert(id.to_owned()) {
            return Err("DEPENDENCY_CYCLE".into());
        }
        let plugin = self.enabled(id)?;
        for dependency in &plugin.manifest.dependencies {
            match self.select(id, dependency) {
                Ok(binding) if self.is_platform_provider(&binding.provider_id) => {}
                Ok(binding) => self.visit(&binding.provider_id, stack, visited, order)?,
                Err(error)
                    if dependency.optional == Some(true)
                        && matches!(
                            error.as_str(),
                            "DEPENDENCY_UNAVAILABLE"
                                | "SERVICE_UNAVAILABLE"
                                | "DEPENDENCY_INCOMPATIBLE"
                                | "RUNTIME_UNAVAILABLE"
                                | "RUNTIME_VERSION_INCOMPATIBLE"
                        ) => {}
                Err(error) => return Err(error),
            }
        }
        stack.remove(id);
        visited.insert(id.to_owned());
        order.push(&plugin.manifest);
        Ok(())
    }
    pub fn dependents(&self, provider: &str) -> Vec<String> {
        let mut affected = HashSet::from([provider.to_owned()]);
        loop {
            let before = affected.len();
            for (id, plugin) in &self.plugins {
                if plugin.enabled
                    && plugin.manifest.dependencies.iter().any(|item| {
                        self.select(id, item)
                            .is_ok_and(|binding| affected.contains(&binding.provider_id))
                    })
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
    pub fn resolve(
        &self,
        caller_id: &str,
        request: &Request,
        context: &str,
    ) -> Result<Route, String> {
        let caller = self.enabled(caller_id)?;
        if request.method == crate::BACKGROUND_METHOD {
            return Err("METHOD_UNAVAILABLE".into());
        }
        let (provider, service) = match &request.target {
            Target::PlatformTarget(_) => {
                let builtin = self.platform_binding(&request.method)?;
                let selected = self.capability(caller_id, &builtin.export.id)?;
                if selected != builtin {
                    return Err("PLATFORM_PROVIDER_MISMATCH".into());
                }
                return Ok(Route::Platform);
            }
            Target::BackendTarget(_) => (caller, None),
            Target::ServiceTarget(target) => {
                let binding = self.capability(caller_id, &target.id)?;
                if self.is_platform_provider(&binding.provider_id) {
                    let builtin = self.platform_binding(&request.method)?;
                    if binding != builtin {
                        return Err("METHOD_UNAVAILABLE".into());
                    }
                    let mut platform_request = request.clone();
                    platform_request.target =
                        serde_json::from_value(serde_json::json!({"kind":"platform"})).unwrap();
                    validate_request(&serde_json::to_string(&platform_request).unwrap())?;
                    return Ok(Route::Platform);
                }
                let provider = self.enabled(&binding.provider_id)?;
                let runtime = provider
                    .manifest
                    .runtime_capabilities
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|runtime| runtime.capability_id == target.id);
                if request.method == "run"
                    && runtime
                    && !caller
                        .manifest
                        .permissions
                        .iter()
                        .any(|permission| permission == "runtime:execute")
                {
                    return Err("PERMISSION_DENIED".into());
                }
                let runtime_result = if runtime && request.method == "run" {
                    if !schema::matches(&contract()["$defs"]["RuntimeRunParams"], &request.params) {
                        return Err("INVALID_RUNTIME_PARAMS".into());
                    }
                    Some(contract()["$defs"]["ProcessResult"].clone())
                } else if runtime && request.method == "environment" {
                    if !schema::matches(&contract()["$defs"]["Empty"], &request.params) {
                        return Err("INVALID_RUNTIME_PARAMS".into());
                    }
                    Some(contract()["methods"]["runtime.list"]["result"].clone())
                } else {
                    None
                };
                let method = binding
                    .export
                    .methods
                    .iter()
                    .find(|item| item.name() == request.method)
                    .ok_or("METHOD_UNAVAILABLE")?;
                let result = match method {
                    ServiceMethod::LegacyServiceMethod(_) => None,
                    ServiceMethod::CapabilityMethod(method) => {
                        if method
                            .contexts
                            .as_ref()
                            .is_some_and(|items| !items.iter().any(|item| item == context))
                        {
                            return Err("CONTEXT_DENIED".into());
                        }
                        if !schema::matches(&method.params, &request.params) {
                            return Err("INVALID_CAPABILITY_PARAMS".into());
                        }
                        Some(method.result.clone())
                    }
                };
                (
                    self.enabled(&binding.provider_id)?,
                    Some(ServiceCall {
                        id: target.id.clone(),
                        result,
                        runtime_result,
                    }),
                )
            }
        };
        if provider.manifest.backend.is_none() {
            return Err("BACKEND_UNAVAILABLE".into());
        }
        Ok(Route::Backend {
            initiator: caller_id.into(),
            provider: provider.manifest.id.clone(),
            provider_version: provider.manifest.version.clone(),
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
    sequence: Option<u64>,
    recent_renderer_ids: u64,
    inflight: HashMap<String, Option<Arc<AtomicBool>>>,
}
pub struct Admitted {
    pub owner: String,
    pub request: Request,
    pub route: Route,
    pub cancel: Option<Arc<AtomicBool>>,
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
            sequence: None,
            recent_renderer_ids: 0,
            inflight: HashMap::new(),
        }
    }
    /// The frame assigns ordered wire IDs. A fixed replay window admits delivery
    /// reordering without retaining every request made by a long-lived page.
    pub fn renderer(
        manifest: Manifest,
        token: String,
        binding: String,
        expires_at_ms: u64,
    ) -> Self {
        let mut session = Self::new(manifest, token, binding, "renderer".into(), expires_at_ms);
        session.sequence = Some(0);
        session
    }
    pub fn admit(
        &mut self,
        registry: &Registry,
        raw: &str,
        binding: &str,
        now_ms: u64,
    ) -> Result<Admitted, String> {
        let mut request = validate_request(raw)?;
        if request.session != self.token || binding != self.binding {
            return Err("SESSION_DENIED".into());
        }
        if now_ms >= self.expires_at_ms {
            return Err("SESSION_EXPIRED".into());
        }
        if registry.enabled(&self.manifest.id)?.manifest != self.manifest {
            return Err("SESSION_REPLACED".into());
        }
        let route = registry.resolve(&self.manifest.id, &request, &self.context)?;
        if self.context == "backend" && matches!(request.target, Target::BackendTarget(_)) {
            return Err("REENTRANT_BACKEND_CALL".into());
        }
        if route == Route::Platform {
            let method = &contract()["methods"][&request.method];
            if !matches!(request.target, Target::PlatformTarget(_)) {
                crate::schema::normalize(&method["params"], &mut request.params);
            }
            authorize_platform(&self.manifest, &request.method, &self.context)?;
        }
        // Ordered backend SDKs opt in on their first capability. Opaque clients
        // keep their bounded replay ledger for the lifetime of the process.
        let ordered_backend = self.context == "backend"
            && self.seen.is_empty()
            && request.request_id.starts_with("backend-seq-");
        let sequence = if self.sequence.is_some() || ordered_backend {
            let prefix = if self.context == "backend" {
                "backend-seq-"
            } else {
                "renderer-"
            };
            let next = request
                .request_id
                .strip_prefix(prefix)
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|v| *v > 0)
                .ok_or("INVALID_REQUEST")?;
            let high = self.sequence.unwrap_or(0);
            if self.context == "renderer" {
                Some(self.mark_renderer_id(next, high).ok_or("REPLAY_DENIED")?)
            } else if next > high {
                Some((next, 0))
            } else {
                return Err("REPLAY_DENIED".into());
            }
        } else {
            if self.seen.contains(&request.request_id) {
                return Err("REPLAY_DENIED".into());
            }
            None
        };
        let control = route == Route::Platform && request.method == "rpc.cancel";
        if self.inflight.len() >= crate::BUDGET_INFLIGHT + usize::from(control) {
            return Err("BUSY".into());
        }
        if sequence.is_none() && self.seen.len() >= crate::BUDGET_SESSION_REQUESTS {
            return Err("SESSION_EXHAUSTED".into());
        }
        if let Some((high, recent)) = sequence {
            self.sequence = Some(high);
            self.recent_renderer_ids = recent;
        } else {
            self.seen.insert(request.request_id.clone());
        }
        let cancellable = matches!(route, Route::Backend { .. })
            || (route == Route::Platform
                && matches!(
                    request.method.as_str(),
                    "files.choose" | "network.request" | "network.websocket.open"
                ));
        let cancel = cancellable.then(|| Arc::new(AtomicBool::new(false)));
        self.inflight
            .insert(request.request_id.clone(), cancel.clone());
        Ok(Admitted {
            owner: self.manifest.id.clone(),
            request,
            route,
            cancel,
        })
    }
    pub fn finish(&mut self, id: &str) {
        self.inflight.remove(id);
    }
    fn mark_renderer_id(&self, next: u64, high: u64) -> Option<(u64, u64)> {
        if next > high {
            let advance = next - high;
            return Some((
                next,
                if advance < u64::BITS.into() {
                    self.recent_renderer_ids << advance | 1
                } else {
                    1
                },
            ));
        }
        let age = high - next;
        if age >= u64::BITS.into() || self.recent_renderer_ids & (1 << age) != 0 {
            return None;
        }
        Some((high, self.recent_renderer_ids | (1 << age)))
    }

    pub fn cancel(&mut self, id: &str) -> bool {
        if let Some(Some(flag)) = self.inflight.get(id) {
            flag.store(true, Ordering::SeqCst);
            return true;
        }
        // The cancellation can reach the dispatcher before the earlier call.
        // Consume only an unseen ID inside this same session's replay window.
        if self.context == "renderer" {
            if let Some((next, high)) = id
                .strip_prefix("renderer-")
                .and_then(|id| id.parse::<u64>().ok())
                .zip(self.sequence)
            {
                if next > 0 && next < high {
                    if let Some((_, recent)) = self.mark_renderer_id(next, high) {
                        self.recent_renderer_ids = recent;
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn cancel_all(&self) {
        for flag in self.inflight.values().flatten() {
            flag.store(true, Ordering::SeqCst);
        }
    }

    /// The host has reauthorized the existing owner. Keep both replay and inflight
    /// ledgers: renewing a lease must not make a previously used request fresh.
    pub fn renew(&mut self, expires_at_ms: u64) {
        self.expires_at_ms = self.expires_at_ms.max(expires_at_ms);
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
