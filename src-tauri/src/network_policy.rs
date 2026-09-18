use crate::db::Db;
use serde::Serialize;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct NetworkPolicy {
    pub mode: String,
    pub proxy_url: Option<String>,
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self {
            mode: "auto".into(),
            proxy_url: None,
        }
    }
}

static POLICY: OnceLock<RwLock<NetworkPolicy>> = OnceLock::new();

fn current_lock() -> &'static RwLock<NetworkPolicy> {
    POLICY.get_or_init(|| RwLock::new(NetworkPolicy::default()))
}

pub fn current() -> NetworkPolicy {
    current_lock()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

pub fn reload(db: &Arc<Mutex<Db>>) {
    let db = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let connection = db.conn().lock().unwrap();
    let read = |key: &str| -> Option<String> {
        connection
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .ok()
    };
    let mode = read("downloadProxyMode")
        .filter(|value| matches!(value.as_str(), "auto" | "system" | "manual" | "direct"))
        .unwrap_or_else(|| "auto".into());
    let proxy_url = read("downloadProxyUrl").filter(|value| !value.trim().is_empty());
    drop(connection);
    drop(db);
    *current_lock()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = NetworkPolicy { mode, proxy_url };
}

pub fn validate(mode: &str, proxy_url: Option<&str>) -> Result<(), String> {
    if !matches!(mode, "auto" | "system" | "manual" | "direct") {
        return Err("代理模式无效".into());
    }
    if mode == "manual" {
        if let Some(value) = proxy_url.filter(|value| !value.trim().is_empty()) {
            let parsed =
                url::Url::parse(value).map_err(|error| format!("代理地址无效：{error}"))?;
            if !matches!(parsed.scheme(), "http" | "https" | "socks5") {
                return Err("代理仅支持 http、https 或 socks5".into());
            }
        }
    }
    Ok(())
}

impl NetworkPolicy {
    pub fn agent(&self, connect_secs: u64, read_secs: u64) -> Result<ureq::Agent, String> {
        validate(&self.mode, self.proxy_url.as_deref())?;
        if self.mode == "manual"
            && self
                .proxy_url
                .as_ref()
                .is_none_or(|value| value.trim().is_empty())
        {
            return Err("手动代理模式需要填写代理地址".into());
        }
        let mut builder = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(connect_secs))
            .timeout_read(Duration::from_secs(read_secs));
        match self.mode.as_str() {
            "direct" => builder = builder.try_proxy_from_env(false),
            "manual" => {
                let proxy = ureq::Proxy::new(self.proxy_url.as_deref().unwrap())
                    .map_err(|error| format!("代理地址无效：{error}"))?;
                builder = builder.try_proxy_from_env(false).proxy(proxy);
            }
            "auto" if self.uses_manual_proxy() => {
                let proxy = ureq::Proxy::new(self.proxy_url.as_deref().unwrap())
                    .map_err(|error| format!("代理地址无效：{error}"))?;
                builder = builder.try_proxy_from_env(false).proxy(proxy);
            }
            _ => builder = builder.try_proxy_from_env(true),
        }
        Ok(builder.build())
    }

    pub fn uses_system_proxy(&self) -> bool {
        self.mode == "system" || (self.mode == "auto" && !self.uses_manual_proxy())
    }

    pub fn route(&self) -> String {
        match self.mode.as_str() {
            "direct" => "直连".into(),
            "system" => "Windows 系统代理".into(),
            "manual" => format!("手动代理 {}", redact_proxy(self.proxy_url.as_deref())),
            _ if self.uses_manual_proxy() => {
                format!(
                    "自动选择手动代理 {}",
                    redact_proxy(self.proxy_url.as_deref())
                )
            }
            _ => "自动选择 Windows 系统代理".into(),
        }
    }

    pub fn cache_key(&self) -> String {
        format!(
            "{}:{}",
            self.mode,
            self.proxy_url.as_deref().unwrap_or_default()
        )
    }

    pub fn uses_manual_proxy(&self) -> bool {
        self.proxy_url
            .as_deref()
            .and_then(|value| url::Url::parse(value).ok())
            .is_some_and(|url| matches!(url.scheme(), "http" | "https" | "socks5"))
    }
}

fn redact_proxy(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "未设置".into();
    };
    let Ok(mut parsed) = url::Url::parse(value) else {
        return "地址无效".into();
    };
    if !parsed.username().is_empty() {
        let _ = parsed.set_username("***");
    }
    if parsed.password().is_some() {
        let _ = parsed.set_password(Some("***"));
    }
    parsed.to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkDiagnostic {
    pub mode: String,
    pub route: String,
    pub fallback_reason: Option<String>,
    pub stages: Vec<NetworkDiagnosticStage>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkDiagnosticStage {
    pub name: String,
    pub success: bool,
    pub elapsed_ms: u128,
    pub message: String,
}

fn stage(name: &str, operation: impl FnOnce() -> Result<String, String>) -> NetworkDiagnosticStage {
    let started = Instant::now();
    match operation() {
        Ok(message) => NetworkDiagnosticStage {
            name: name.into(),
            success: true,
            elapsed_ms: started.elapsed().as_millis(),
            message,
        },
        Err(message) => NetworkDiagnosticStage {
            name: name.into(),
            success: false,
            elapsed_ms: started.elapsed().as_millis(),
            message,
        },
    }
}

pub fn diagnose() -> NetworkDiagnostic {
    let policy = current();
    let mut stages = Vec::new();
    stages.push(stage("DNS", || {
        let count = ("github.com", 443)
            .to_socket_addrs()
            .map_err(|error| error.to_string())?
            .count();
        Ok(format!("解析到 {count} 个地址"))
    }));
    stages.push(stage("TCP", || {
        let address = ("github.com", 443)
            .to_socket_addrs()
            .map_err(|error| error.to_string())?
            .next()
            .ok_or_else(|| "没有可连接地址".to_string())?;
        TcpStream::connect_timeout(&address, Duration::from_secs(5))
            .map_err(|error| error.to_string())?;
        Ok("github.com:443 可连接".into())
    }));
    let agent = policy.agent(8, 15);
    for (name, url) in [
        ("TLS", "https://github.com"),
        (
            "插件市场",
            "https://github.com/Xuancheng75/CrucibleBox/releases",
        ),
        (
            "更新服务",
            "https://github.com/Xuancheng75/CrucibleBox/releases/latest",
        ),
    ] {
        stages.push(stage(name, || {
            let response = agent
                .as_ref()
                .map_err(|error| error.clone())?
                .head(url)
                .call()
                .map_err(|error| error.to_string())?;
            Ok(format!("HTTP {}", response.status()))
        }));
    }
    NetworkDiagnostic {
        mode: policy.mode.clone(),
        route: policy.route(),
        fallback_reason: if policy.mode == "auto"
            && policy.proxy_url.is_some()
            && !policy.uses_manual_proxy()
        {
            Some("手动代理地址无效，已改用 Windows 系统代理".into())
        } else {
            None
        },
        stages,
    }
}
