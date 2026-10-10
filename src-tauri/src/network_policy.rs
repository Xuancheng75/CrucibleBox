use crate::db::Db;
use serde::Serialize;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedRoute {
    pub mode: String,
    pub transport: String,
    pub proxy_endpoint: Option<String>,
    pub credential_ref: Option<String>,
    pub route_reason: String,
}

#[derive(Clone, Debug)]
pub struct NetworkPolicy {
    pub mode: String,
    pub proxy_url: Option<String>,
}

struct HttpProxyAuth {
    header: String,
}

impl ureq::Middleware for HttpProxyAuth {
    #[allow(clippy::result_large_err)] // Signature required by ureq's middleware trait.
    fn handle(
        &self,
        request: ureq::Request,
        next: ureq::MiddlewareNext,
    ) -> Result<ureq::Response, ureq::Error> {
        next.handle(request.set("Proxy-Authorization", &self.header))
    }
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

#[cfg(all(test, windows))]
thread_local! {
    #[allow(clippy::missing_const_for_thread_local)]
    static PAC_FIXTURE_URL: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

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
    let read = |key: &str| -> Option<String> { db.setting_get(key).ok().flatten() };
    let mode = read("downloadProxyMode")
        .filter(|value| matches!(value.as_str(), "auto" | "system" | "manual" | "direct"))
        .unwrap_or_else(|| "auto".into());
    let proxy_url = read("downloadProxyUrl").filter(|value| !value.trim().is_empty());
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
            if !matches!(parsed.scheme(), "http" | "socks5") {
                return Err("代理仅支持 http 或 socks5；本地 HTTP 代理请使用 http:// 地址".into());
            }
            ureq::Proxy::new(value).map_err(|error| format!("代理地址无效：{error}"))?;
        }
    }
    Ok(())
}

impl NetworkPolicy {
    pub fn resolve(&self, url: &str) -> Result<ResolvedRoute, String> {
        let target = url::Url::parse(url).map_err(|error| format!("网络地址无效：{error}"))?;
        if !matches!(target.scheme(), "http" | "https") {
            return Err("网络服务仅支持 HTTP 和 HTTPS".into());
        }
        let manual = self.mode == "manual" || (self.mode == "auto" && self.uses_manual_proxy());
        if self.mode == "direct" {
            return Ok(ResolvedRoute {
                mode: self.mode.clone(),
                transport: "rust-http".into(),
                proxy_endpoint: None,
                credential_ref: None,
                route_reason: "显式直连".into(),
            });
        }
        if manual {
            validate("manual", self.proxy_url.as_deref())?;
            let proxy = self
                .proxy_url
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or("手动代理模式需要填写代理地址")?;
            return Ok(ResolvedRoute {
                mode: self.mode.clone(),
                transport: "rust-http".into(),
                proxy_endpoint: Some(proxy.into()),
                credential_ref: None,
                route_reason: "手动代理".into(),
            });
        }
        let (proxy, reason) = system_proxy_for_url(&target)?;
        Ok(ResolvedRoute {
            mode: self.mode.clone(),
            transport: "rust-http".into(),
            proxy_endpoint: proxy,
            credential_ref: None,
            route_reason: reason,
        })
    }

    pub fn agent_for_url(
        &self,
        url: &str,
        connect_secs: u64,
        read_secs: u64,
    ) -> Result<(ureq::Agent, ResolvedRoute), String> {
        let route = self.resolve(url)?;
        let mut builder = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(connect_secs))
            .timeout_read(Duration::from_secs(read_secs))
            .try_proxy_from_env(false);
        if let Some(endpoint) = route.proxy_endpoint.as_deref() {
            let proxy = ureq::Proxy::new(endpoint)
                .map_err(|error| format!("解析后的代理地址无效：{error}"))?;
            builder = builder.proxy(proxy);
            // ureq 2 authenticates HTTPS CONNECT but omits credentials for
            // ordinary HTTP proxy requests. Bind that header to this resolved
            // HTTP route and return redirects instead of forwarding credentials
            // into an HTTPS origin on an automatic redirect.
            let endpoint_url = url::Url::parse(endpoint).map_err(|error| error.to_string())?;
            if url::Url::parse(url).is_ok_and(|target| target.scheme() == "http")
                && endpoint_url.scheme() == "http"
                && !endpoint_url.username().is_empty()
            {
                use base64::Engine;
                let credentials = format!(
                    "{}:{}",
                    endpoint_url.username(),
                    endpoint_url.password().unwrap_or_default()
                );
                let header = format!(
                    "Basic {}",
                    base64::engine::general_purpose::STANDARD.encode(credentials)
                );
                builder = builder.redirects(0).middleware(HttpProxyAuth { header });
            }
        }
        Ok((builder.build(), route))
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
            .is_some_and(|url| matches!(url.scheme(), "http" | "socks5"))
            && self
                .proxy_url
                .as_deref()
                .is_some_and(|value| ureq::Proxy::new(value).is_ok())
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

fn select_windows_proxy(spec: &str, scheme: &str) -> Option<String> {
    let selected = if spec.contains('=') {
        spec.split([';', ' ']).find_map(|part| {
            part.split_once('=')
                .filter(|(kind, _)| *kind == scheme)
                .map(|(_, value)| value)
        })?
    } else {
        spec.split([';', ' ']).find(|part| !part.is_empty())?
    };
    if selected.is_empty() {
        return None;
    }
    Some(if selected.contains("://") {
        selected.into()
    } else {
        format!("http://{selected}")
    })
}

fn windows_proxy_bypassed(host: &str, rules: &str) -> bool {
    rules.split([';', ' ', ',']).any(|rule| {
        let rule = rule.trim();
        (rule.eq_ignore_ascii_case("<local>") && !host.contains('.'))
            || (!rule.is_empty() && host.eq_ignore_ascii_case(rule))
            || rule
                .strip_prefix("*.")
                .is_some_and(|suffix| host.to_ascii_lowercase().ends_with(&format!(".{suffix}")))
    })
}

#[cfg(windows)]
fn system_proxy_for_url(target: &url::Url) -> Result<(Option<String>, String), String> {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::Networking::WinHttp::{
        WinHttpCloseHandle, WinHttpGetIEProxyConfigForCurrentUser, WinHttpGetProxyForUrl,
        WinHttpOpen, WinHttpSetTimeouts, WINHTTP_ACCESS_TYPE_NAMED_PROXY,
        WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_AUTOPROXY_AUTO_DETECT, WINHTTP_AUTOPROXY_CONFIG_URL,
        WINHTTP_AUTOPROXY_OPTIONS, WINHTTP_AUTO_DETECT_TYPE_DHCP, WINHTTP_AUTO_DETECT_TYPE_DNS_A,
        WINHTTP_CURRENT_USER_IE_PROXY_CONFIG, WINHTTP_PROXY_INFO,
    };

    unsafe fn read_wide(ptr: *const u16) -> String {
        if ptr.is_null() {
            return String::new();
        }
        let mut length = 0;
        while unsafe { *ptr.add(length) } != 0 {
            length += 1;
        }
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(ptr, length) })
    }
    unsafe fn free_wide(ptr: *mut u16) {
        if !ptr.is_null() {
            unsafe {
                GlobalFree(ptr.cast::<c_void>());
            }
        }
    }
    struct Config(WINHTTP_CURRENT_USER_IE_PROXY_CONFIG);
    impl Drop for Config {
        fn drop(&mut self) {
            unsafe {
                free_wide(self.0.lpszAutoConfigUrl);
                free_wide(self.0.lpszProxy);
                free_wide(self.0.lpszProxyBypass);
            }
        }
    }
    struct ProxyInfo(WINHTTP_PROXY_INFO);
    impl Drop for ProxyInfo {
        fn drop(&mut self) {
            unsafe {
                free_wide(self.0.lpszProxy);
                free_wide(self.0.lpszProxyBypass);
            }
        }
    }
    struct Session(*mut c_void);
    impl Drop for Session {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    WinHttpCloseHandle(self.0);
                }
            }
        }
    }

    let mut config = Config(unsafe { std::mem::zeroed() });
    if unsafe { WinHttpGetIEProxyConfigForCurrentUser(&mut config.0) } == 0 {
        return Err(format!(
            "读取 Windows 系统代理失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    #[cfg(test)]
    let fixture_url = PAC_FIXTURE_URL.with(|value| {
        value.borrow().as_ref().map(|url| {
            url.encode_utf16()
                .chain(std::iter::once(0))
                .collect::<Vec<u16>>()
        })
    });
    #[cfg(not(test))]
    let fixture_url: Option<Vec<u16>> = None;
    let pac_url = fixture_url
        .as_ref()
        .map_or(config.0.lpszAutoConfigUrl.cast_const(), |url| url.as_ptr());
    let has_pac = !pac_url.is_null();
    if has_pac || config.0.fAutoDetect != 0 {
        let agent: Vec<u16> = "CrucibleBox NetworkService\0".encode_utf16().collect();
        let session = Session(unsafe {
            WinHttpOpen(
                agent.as_ptr(),
                WINHTTP_ACCESS_TYPE_NO_PROXY,
                std::ptr::null(),
                std::ptr::null(),
                0,
            )
        });
        if session.0.is_null() {
            return Err(format!(
                "初始化 Windows PAC 解析失败：{}",
                std::io::Error::last_os_error()
            ));
        }
        unsafe {
            WinHttpSetTimeouts(session.0, 8000, 8000, 8000, 8000);
        }
        let mut options: WINHTTP_AUTOPROXY_OPTIONS = unsafe { std::mem::zeroed() };
        if has_pac {
            options.dwFlags = WINHTTP_AUTOPROXY_CONFIG_URL;
            options.lpszAutoConfigUrl = pac_url;
        } else {
            options.dwFlags = WINHTTP_AUTOPROXY_AUTO_DETECT;
            options.dwAutoDetectFlags =
                WINHTTP_AUTO_DETECT_TYPE_DHCP | WINHTTP_AUTO_DETECT_TYPE_DNS_A;
        }
        let mut info = ProxyInfo(unsafe { std::mem::zeroed() });
        let wide_url: Vec<u16> = target
            .as_str()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        if unsafe { WinHttpGetProxyForUrl(session.0, wide_url.as_ptr(), &mut options, &mut info.0) }
            == 0
        {
            let error = std::io::Error::last_os_error();
            if !has_pac && error.raw_os_error() == Some(12180) {
                let proxy = unsafe { read_wide(config.0.lpszProxy) };
                let bypass = unsafe { read_wide(config.0.lpszProxyBypass) };
                return static_windows_route(target, &proxy, &bypass)
                    .map(|(proxy, reason)| (proxy, format!("Windows 未发现自动代理；{reason}")));
            }
            return Err(format!("Windows PAC 解析失败：{}", error));
        }
        if info.0.dwAccessType == WINHTTP_ACCESS_TYPE_NO_PROXY {
            return Ok((None, "Windows PAC 指定直连".into()));
        }
        if info.0.dwAccessType != WINHTTP_ACCESS_TYPE_NAMED_PROXY {
            return Err("Windows PAC 返回不支持的路由".into());
        }
        let proxy = unsafe { read_wide(info.0.lpszProxy) };
        let bypass = unsafe { read_wide(info.0.lpszProxyBypass) };
        if target
            .host_str()
            .is_some_and(|host| windows_proxy_bypassed(host, &bypass))
        {
            return Ok((None, "Windows PAC 代理排除规则".into()));
        }
        return select_windows_proxy(&proxy, target.scheme())
            .map(|proxy| (Some(proxy), "Windows PAC".into()))
            .ok_or("Windows PAC 未返回适用于目标协议的代理".into());
    }
    let proxy = unsafe { read_wide(config.0.lpszProxy) };
    let bypass = unsafe { read_wide(config.0.lpszProxyBypass) };
    static_windows_route(target, &proxy, &bypass)
}

fn static_windows_route(
    target: &url::Url,
    proxy: &str,
    bypass: &str,
) -> Result<(Option<String>, String), String> {
    if proxy.is_empty() {
        return Ok((None, "Windows 系统设置为直连".into()));
    }
    if target
        .host_str()
        .is_some_and(|host| windows_proxy_bypassed(host, bypass))
    {
        return Ok((None, "Windows 系统代理排除规则".into()));
    }
    select_windows_proxy(proxy, target.scheme())
        .map(|proxy| (Some(proxy), "Windows 系统手动代理".into()))
        .ok_or("Windows 系统代理未配置目标协议".into())
}

#[cfg(not(windows))]
fn system_proxy_for_url(_target: &url::Url) -> Result<(Option<String>, String), String> {
    Err("系统代理解析仅支持 Windows".into())
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
            let (agent, _) = policy.agent_for_url(url, 8, 15)?;
            let response = agent.head(url).call().map_err(|error| error.to_string())?;
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

#[cfg(test)]
mod tests {
    use super::{select_windows_proxy, windows_proxy_bypassed, NetworkPolicy};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    fn http_fixture(status: &str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let status = status.to_string();
        let worker = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(8);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            Instant::now() < deadline,
                            "HTTP fixture was never contacted"
                        );
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("HTTP fixture accept failed: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
                assert!(request.len() < 16_384);
            }
            let redirect = if status.starts_with("302") {
                "Location: https://fixture.invalid/redirected\r\n"
            } else {
                ""
            };
            write!(
                stream,
                "HTTP/1.1 {status}\r\n{redirect}Content-Length: 7\r\nConnection: close\r\n\r\nfixture"
            )
            .unwrap();
            String::from_utf8(request).unwrap()
        });
        (format!("http://{address}"), worker)
    }

    #[test]
    fn direct_http_ignores_configured_proxy_and_preserves_404() {
        let (url, worker) = http_fixture("404 Not Found");
        let policy = NetworkPolicy {
            mode: "direct".into(),
            proxy_url: Some("http://127.0.0.1:1".into()),
        };
        let (agent, route) = policy.agent_for_url(&url, 2, 2).unwrap();
        assert!(route.proxy_endpoint.is_none());
        assert!(matches!(
            agent.get(&format!("{url}/missing")).call(),
            Err(ureq::Error::Status(404, _))
        ));
        assert!(worker
            .join()
            .unwrap()
            .starts_with("GET /missing HTTP/1.1\r\n"));
    }

    #[test]
    fn manual_and_auto_http_use_authenticated_proxy() {
        for mode in ["manual", "auto"] {
            let (proxy, worker) = http_fixture("200 OK");
            let endpoint = proxy.replacen("http://", "http://user:password@", 1);
            let policy = NetworkPolicy {
                mode: mode.into(),
                proxy_url: Some(endpoint.clone()),
            };
            let target = "http://fixture.invalid/resource";
            let (agent, route) = policy.agent_for_url(target, 2, 2).unwrap();
            assert_eq!(route.proxy_endpoint.as_deref(), Some(endpoint.as_str()));
            assert_eq!(
                agent.get(target).call().unwrap().into_string().unwrap(),
                "fixture"
            );
            let request = worker.join().unwrap();
            assert!(request.starts_with("GET http://fixture.invalid/resource HTTP/1.1\r\n"));
            assert!(request
                .to_ascii_lowercase()
                .contains("proxy-authorization: basic dxnlcjpwyxnzd29yza=="));
            assert!(!policy.route().contains("password"));
        }
    }

    #[test]
    #[ignore = "requires CRUCIBLEBOX_TLS_ACCEPTANCE_URL and CRUCIBLEBOX_TLS_EXPECT_VALID"]
    fn verifies_tls_against_windows_system_trust() {
        let url =
            std::env::var("CRUCIBLEBOX_TLS_ACCEPTANCE_URL").expect("TLS fixture URL required");
        assert!(url.starts_with("https://"));
        let expected_valid = std::env::var("CRUCIBLEBOX_TLS_EXPECT_VALID").unwrap() == "true";
        let policy = NetworkPolicy {
            mode: "direct".into(),
            proxy_url: None,
        };
        let (agent, _) = policy.agent_for_url(&url, 5, 10).unwrap();
        let response = agent.head(&url).call();
        if expected_valid {
            let response = response.expect("system-trusted TLS fixture must connect");
            assert!((200..400).contains(&response.status()));
        } else {
            let error = response.expect_err("untrusted fixture certificate must be rejected");
            let detail = format!("{error:?}").to_ascii_lowercase();
            assert!(
                detail.contains("certificate") || detail.contains("unknownissuer"),
                "failure must be certificate validation, not unrelated connectivity: {detail}"
            );
        }
    }

    #[test]
    fn authenticated_socks5_proxy_transports_http_without_local_target_dns() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let worker = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(8);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline);
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("SOCKS fixture accept failed: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut greeting = [0; 2];
            stream.read_exact(&mut greeting).unwrap();
            assert_eq!(greeting[0], 5);
            let mut methods = vec![0; greeting[1] as usize];
            stream.read_exact(&mut methods).unwrap();
            assert!(methods.contains(&2));
            stream.write_all(&[5, 2]).unwrap();
            let mut auth = [0; 2];
            stream.read_exact(&mut auth).unwrap();
            assert_eq!(auth[0], 1);
            let mut user = vec![0; auth[1] as usize];
            stream.read_exact(&mut user).unwrap();
            let mut password_len = [0];
            stream.read_exact(&mut password_len).unwrap();
            let mut password = vec![0; password_len[0] as usize];
            stream.read_exact(&mut password).unwrap();
            assert_eq!(user, b"user");
            assert_eq!(password, b"password");
            stream.write_all(&[1, 0]).unwrap();
            let mut request = [0; 4];
            stream.read_exact(&mut request).unwrap();
            assert_eq!(request, [5, 1, 0, 3]);
            let mut length = [0];
            stream.read_exact(&mut length).unwrap();
            let mut domain = vec![0; length[0] as usize];
            stream.read_exact(&mut domain).unwrap();
            assert_eq!(domain, b"fixture.invalid");
            let mut port = [0; 2];
            stream.read_exact(&mut port).unwrap();
            assert_eq!(u16::from_be_bytes(port), 80);
            stream
                .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 80])
                .unwrap();
            let mut http = Vec::new();
            while !http.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                http.push(byte[0]);
                assert!(http.len() < 16_384);
            }
            assert!(http.starts_with(b"GET /resource HTTP/1.1\r\n"));
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nfixture",
                )
                .unwrap();
        });
        let policy = NetworkPolicy {
            mode: "manual".into(),
            proxy_url: Some(format!("socks5://user:password@{address}")),
        };
        let target = "http://fixture.invalid/resource";
        let (agent, _) = policy.agent_for_url(target, 2, 2).unwrap();
        assert_eq!(
            agent.get(target).call().unwrap().into_string().unwrap(),
            "fixture"
        );
        worker.join().unwrap();
    }

    #[test]
    #[cfg(windows)]
    #[ignore = "requires CRUCIBLEBOX_PAC_ACCEPTANCE_URL with a controlled PAC fixture"]
    fn windows_pac_resolves_proxy_and_direct_without_modifying_system_settings() {
        let fixture =
            std::env::var("CRUCIBLEBOX_PAC_ACCEPTANCE_URL").expect("PAC fixture URL required");
        super::PAC_FIXTURE_URL.with(|value| *value.borrow_mut() = Some(fixture));
        let policy = NetworkPolicy {
            mode: "system".into(),
            proxy_url: None,
        };
        let proxied = policy.resolve("http://fixture.invalid/resource").unwrap();
        assert_eq!(
            proxied.proxy_endpoint.as_deref(),
            Some("http://127.0.0.1:3456")
        );
        let direct = policy.resolve("http://localhost/resource").unwrap();
        assert!(direct.proxy_endpoint.is_none());
        super::PAC_FIXTURE_URL
            .with(|value| *value.borrow_mut() = Some("http://127.0.0.1:1/unavailable.pac".into()));
        assert!(policy
            .resolve("http://fixture.invalid/resource")
            .unwrap_err()
            .contains("PAC"));
        super::PAC_FIXTURE_URL.with(|value| *value.borrow_mut() = None);
    }

    #[test]
    #[cfg(windows)]
    #[ignore = "requires a Windows user profile with a local direct/bypass route"]
    fn system_and_auto_routes_connect_local_origin_using_windows_settings() {
        for mode in ["system", "auto"] {
            let (url, worker) = http_fixture("200 OK");
            let policy = NetworkPolicy {
                mode: mode.into(),
                proxy_url: None,
            };
            let (agent, route) = policy.agent_for_url(&url, 2, 2).unwrap();
            assert!(
                route.proxy_endpoint.is_none(),
                "local acceptance requires Windows direct/bypass route"
            );
            assert_eq!(
                agent.get(&url).call().unwrap().into_string().unwrap(),
                "fixture"
            );
            worker.join().unwrap();
        }
    }

    #[test]
    fn direct_route_ignores_environment_and_manual_proxy() {
        let policy = NetworkPolicy {
            mode: "direct".into(),
            proxy_url: Some("http://127.0.0.1:7890".into()),
        };
        let route = policy.resolve("https://example.com/path").unwrap();
        assert!(route.proxy_endpoint.is_none());
        assert_eq!(route.route_reason, "显式直连");
    }

    #[test]
    fn authenticated_http_redirect_is_returned_without_forwarding_proxy_credentials() {
        let (proxy, worker) = http_fixture("302 Found");
        let policy = NetworkPolicy {
            mode: "manual".into(),
            proxy_url: Some(proxy.replacen("http://", "http://user:password@", 1)),
        };
        let target = "http://fixture.invalid/resource";
        let (agent, _) = policy.agent_for_url(target, 2, 2).unwrap();
        let response = agent.get(target).call().unwrap();
        assert_eq!(response.status(), 302);
        assert_eq!(
            response.header("Location"),
            Some("https://fixture.invalid/redirected")
        );
        worker.join().unwrap();
    }

    #[test]
    fn manual_route_is_bound_to_explicit_proxy() {
        let policy = NetworkPolicy {
            mode: "manual".into(),
            proxy_url: Some("socks5://127.0.0.1:1080".into()),
        };
        let route = policy.resolve("https://example.com").unwrap();
        assert_eq!(
            route.proxy_endpoint.as_deref(),
            Some("socks5://127.0.0.1:1080")
        );
    }

    #[test]
    fn windows_proxy_list_selects_target_scheme_and_bypass() {
        assert_eq!(
            select_windows_proxy("http=127.0.0.1:8000;https=127.0.0.1:9000", "https").as_deref(),
            Some("http://127.0.0.1:9000")
        );
        assert!(windows_proxy_bypassed("printer", "<local>;*.example.com"));
        assert!(windows_proxy_bypassed(
            "api.example.com",
            "<local>;*.example.com"
        ));
        assert!(!windows_proxy_bypassed(
            "api.other.com",
            "<local>;*.example.com"
        ));
    }
}
