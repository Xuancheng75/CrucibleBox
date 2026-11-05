//! Routing identity is supplied by the host, never by domain parameters.
use crate::{compatible, contract, validate_request, CapabilityBinding, Manifest, Request, Target};
use semver::Version;
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub struct Installed {
    pub manifest: Manifest,
    pub enabled: bool,
}
pub struct Registry {
    plugins: HashMap<String, Installed>,
    capabilities: HashMap<String, Vec<(Version, CapabilityBinding)>>,
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
    /// Index approved manifests. A broken dependency chain is local to its consumers.
    pub fn new(plugins: impl IntoIterator<Item = Installed>) -> Result<Self, String> {
        let mut map = HashMap::new();
        let mut capabilities: HashMap<String, Vec<(Version, CapabilityBinding)>> = HashMap::new();
        for plugin in plugins {
            if plugin.enabled {
                for export in &plugin.manifest.exports {
                    let version = Version::parse(&export.version).map_err(|_| "INVALID_VERSION")?;
                    capabilities.entry(export.id.clone()).or_default().push((
                        version,
                        CapabilityBinding {
                            provider_id: plugin.manifest.id.clone(),
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
        })
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
            self.enabled(provider)?;
        }
        let mut found = false;
        for (_, binding) in self.capabilities.get(&dependency.id).into_iter().flatten() {
            if binding.provider_id == caller
                || dependency.provider_id.as_ref().map_or_else(
                    || binding.export.interchangeable != Some(true),
                    |provider| provider != &binding.provider_id,
                )
            {
                continue;
            }
            found = true;
            if compatible(&binding.export.version, &dependency.version)? {
                return Ok(binding);
            }
        }
        Err(if found {
            "DEPENDENCY_INCOMPATIBLE"
        } else {
            "SERVICE_UNAVAILABLE"
        }
        .into())
    }
    pub fn capability(&self, caller: &str, id: &str) -> Result<&CapabilityBinding, String> {
        let dependency = self
            .enabled(caller)?
            .manifest
            .dependencies
            .iter()
            .find(|item| item.id == id)
            .ok_or("DEPENDENCY_UNDECLARED")?;
        let binding = self.select(caller, dependency)?;
        self.visit(
            &binding.provider_id,
            &mut HashSet::from([caller.to_owned()]),
            &mut HashSet::new(),
            &mut Vec::new(),
        )?;
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
                Ok(binding) => self.visit(&binding.provider_id, stack, visited, order)?,
                Err(error)
                    if dependency.optional == Some(true)
                        && matches!(
                            error.as_str(),
                            "DEPENDENCY_UNAVAILABLE"
                                | "SERVICE_UNAVAILABLE"
                                | "DEPENDENCY_INCOMPATIBLE"
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
    pub fn resolve(&self, caller_id: &str, target: &Target, method: &str) -> Result<Route, String> {
        let caller = self.enabled(caller_id)?;
        let (provider, service) = match target {
            Target::PlatformTarget(_) => return Ok(Route::Platform),
            Target::BackendTarget(_) => (caller, None),
            Target::ServiceTarget(target) => {
                let binding = self.capability(caller_id, &target.id)?;
                if !binding.export.methods.iter().any(|item| item == method) {
                    return Err("METHOD_UNAVAILABLE".into());
                }
                (self.enabled(&binding.provider_id)?, Some(target.id.clone()))
            }
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
    sequence: Option<u64>,
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
            sequence: None,
            inflight: HashSet::new(),
        }
    }
    /// The host-owned frame runtime assigns ordered wire IDs. A high-water mark
    /// rejects replay without retaining every request made by a long-lived page.
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
            if next <= self.sequence.unwrap_or(0) {
                return Err("REPLAY_DENIED".into());
            }
            Some(next)
        } else {
            if self.seen.contains(&request.request_id) {
                return Err("REPLAY_DENIED".into());
            }
            None
        };
        if self.inflight.len() >= crate::BUDGET_INFLIGHT {
            return Err("BUSY".into());
        }
        if sequence.is_none() && self.seen.len() >= crate::BUDGET_SESSION_REQUESTS {
            return Err("SESSION_EXHAUSTED".into());
        }
        if let Some(next) = sequence {
            self.sequence = Some(next);
        } else {
            self.seen.insert(request.request_id.clone());
        }
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
