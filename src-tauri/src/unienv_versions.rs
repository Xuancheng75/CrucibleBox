// UniEnv 在线版本发现（1.9.12）
// 产品决策：node/go/java/ruby/zig/deno/bun 上游提供权威 SHA-256，开放
// 「在线新版本」安装；python/git/rust/php 继续使用内置目录（新版本等待
// 插件更新）。
//
// 安全边界：所有元数据与制品均走 HTTPS；下载后按上游声明的 SHA-256 校验，
// 校验失败即失败（fail-closed 语义不变，只是摘要来源从编译期固定改为运行期
// 官方端点声明）。网络不可达时静默回退内置目录。

use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const HTTP_TIMEOUT_SECS: u64 = 15;
/// 元数据缓存 TTL：打开插件页频繁轮询时避免打爆官方端点
const CACHE_TTL: Duration = Duration::from_secs(600);
/// 网络调用硬超时（含 DNS 解析挂起场景）：超过即放弃并回退内置目录。
/// 必须显著小于 renderer RPC 的 30s 上限。
const HARD_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Clone, Debug)]
pub struct OnlineArtifact {
    pub sha256: String,
    /// (url, label) 按优先级排列
    pub urls: UrlCandidates,
}

struct CacheEntry {
    at: Instant,
    /// 仅版本号列表（列表用）；安装时再按版本取 OnlineArtifact（含独立缓存）
    versions: Vec<String>,
}

fn list_cache() -> &'static Mutex<HashMap<String, CacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// (url, label) 候选列表类型别名（clippy type-complexity）
pub type UrlCandidates = Vec<(String, String)>;
type ArtifactCache = HashMap<(String, String), (Instant, OnlineArtifact)>;

fn artifact_cache() -> &'static Mutex<ArtifactCache> {
    static CACHE: OnceLock<Mutex<ArtifactCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn http_get_text(url: &str) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build();
    let response = agent
        .get(url)
        .set("User-Agent", "CrucibleBox/1.9.25")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("GET {url}: {e}"))?;
    let mut text = String::new();
    response
        .into_reader()
        .take(8 * 1024 * 1024)
        .read_to_string(&mut text)
        .map_err(|e| format!("read {url}: {e}"))?;
    Ok(text)
}

/// 支持在线发现的工具集合
pub fn provider_supports(tool: &str) -> bool {
    matches!(
        tool,
        "node" | "go" | "java" | "ruby" | "zig" | "deno" | "bun"
    )
}

/// 强制绕过缓存重新拉取（「检查语言新版本」按钮路径）。
pub fn online_versions_force(tool: &str) -> Vec<String> {
    online_versions_impl(tool, true)
}

fn online_versions_impl(tool: &str, force: bool) -> Vec<String> {
    if !force {
        let cache = list_cache().lock().unwrap();
        if let Some(entry) = cache.get(tool) {
            if entry.at.elapsed() < CACHE_TTL {
                return entry.versions.clone();
            }
        }
    }
    let tool_owned = tool.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name(format!("unienv-online-{tool}"))
        .spawn(move || {
            let result = (|| -> Result<Vec<String>, String> {
                match tool_owned.as_str() {
                    "node" => Ok(fetch_node_versions()?
                        .into_iter()
                        .map(|v| v.trim_start_matches('v').to_string())
                        .collect()),
                    "go" => fetch_go_versions(),
                    "java" => fetch_java_versions(),
                    "ruby" => fetch_github_versions("oneclick/rubyinstaller2", "RubyInstaller-"),
                    "deno" => fetch_github_versions("denoland/deno", "v"),
                    "bun" => fetch_github_versions("oven-sh/bun", "bun-v"),
                    "zig" => fetch_zig_versions(),
                    _ => Err(format!("provider unsupported: {tool_owned}")),
                }
            })();
            let _ = tx.send(result);
        })
        .ok();
    match rx.recv_timeout(HARD_TIMEOUT) {
        Ok(Ok(mut versions)) => {
            versions.sort_by(|a, b| compare_version_desc(b, a));
            versions.dedup();
            list_cache().lock().unwrap().insert(
                tool.to_string(),
                CacheEntry {
                    at: Instant::now(),
                    versions: versions.clone(),
                },
            );
            versions
        }
        Ok(Err(e)) => {
            eprintln!("[unienv] online version fetch failed for {tool}: {e}");
            Vec::new()
        }
        Err(_) => {
            eprintln!(
                "[unienv] online version fetch hard-timeout({}ms) for {tool}",
                HARD_TIMEOUT.as_millis()
            );
            Vec::new()
        }
    }
}

/// 安装前解析在线制品（带硬超时）：超时返回 Err，避免阻塞宿主 RPC。
pub fn online_artifact_bounded(
    tool: &str,
    version: &str,
    mirror: &str,
) -> Result<OnlineArtifact, String> {
    let tool_owned = tool.to_string();
    let version_owned = version.to_string();
    let mirror_owned = mirror.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name(format!("unienv-artifact-{tool}"))
        .spawn(move || {
            let _ = tx.send(online_artifact(&tool_owned, &version_owned, &mirror_owned));
        })
        .ok();
    match rx.recv_timeout(HARD_TIMEOUT) {
        Ok(result) => result,
        Err(_) => Err(format!(
            "TIMEOUT: resolving online artifact for {tool} {version}"
        )),
    }
}

/// 安装期解析：在线版本的下载 URL 列表与 SHA-256。带 10 分钟制品级缓存。
pub fn online_artifact(tool: &str, version: &str, mirror: &str) -> Result<OnlineArtifact, String> {
    let key = (tool.to_string(), version.to_string());
    {
        let cache = artifact_cache().lock().unwrap();
        if let Some((at, artifact)) = cache.get(&key) {
            if at.elapsed() < CACHE_TTL {
                return Ok(artifact.clone());
            }
        }
    }
    let artifact = match tool {
        "node" => node_artifact(version, mirror)?,
        "go" => go_artifact(version, mirror)?,
        "java" => java_artifact(version)?,
        "ruby" => ruby_artifact(version)?,
        "deno" => github_binary_artifact(
            "denoland/deno",
            &format!("v{version}"),
            |name| name == "deno-x86_64-pc-windows-msvc.zip",
            "Deno",
        )?,
        "bun" => github_binary_artifact(
            "oven-sh/bun",
            &format!("bun-v{version}"),
            |name| name == "bun-windows-x64.zip",
            "Bun",
        )?,
        "zig" => zig_artifact(version)?,
        _ => return Err(format!("provider unsupported: {tool}")),
    };
    artifact_cache()
        .lock()
        .unwrap()
        .insert(key, (Instant::now(), artifact.clone()));
    Ok(artifact)
}

// ---------------------------------------------------------------------------
// node：https://nodejs.org/dist/index.json + <base>/v<ver>/SHASUMS256.txt
// ---------------------------------------------------------------------------

const NODE_MIRROR_BASES: &[(&str, &str)] = &[
    ("npmmirror", "https://npmmirror.com/mirrors/node"),
    (
        "tuna",
        "https://mirrors.tuna.tsinghua.edu.cn/nodejs-release",
    ),
    ("direct", "https://nodejs.org/dist"),
];

fn fetch_node_versions() -> Result<Vec<String>, String> {
    let text = http_get_text("https://nodejs.org/dist/index.json")?;
    let arr: Value = serde_json::from_str(&text).map_err(|e| format!("bad index.json: {e}"))?;
    let mut out = Vec::new();
    if let Some(items) = arr.as_array() {
        for item in items {
            if let Some(v) = item.get("version").and_then(Value::as_str) {
                out.push(v.trim_start_matches('v').to_string());
            }
        }
    }
    Ok(out)
}

fn node_shasum(base_url: &str, version: &str) -> Result<String, String> {
    let url = format!("{base_url}/v{version}/SHASUMS256.txt");
    let text = http_get_text(&url)?;
    let needle = format!("node-v{version}-win-x64.zip");
    for line in text.lines() {
        // 格式：<hex>  two-spaces  <filename>
        if let Some((hash, name)) = line.split_once("  ") {
            if name.trim() == needle {
                let hash = hash.trim().to_lowercase();
                if hash.len() == 64 {
                    return Ok(hash);
                }
            }
        }
    }
    Err(format!("SHASUMS256.txt lacks {needle}"))
}

fn node_artifact(version: &str, mirror: &str) -> Result<OnlineArtifact, String> {
    let mut ordered_bases: Vec<&(&str, &str)> = NODE_MIRROR_BASES
        .iter()
        .filter(|(id, _)| *id == mirror)
        .collect();
    for b in NODE_MIRROR_BASES {
        if !ordered_bases.contains(&b) {
            ordered_bases.push(b);
        }
    }
    let sha256 = node_shasum(ordered_bases[0].1, version)?;
    let filename = format!("node-v{version}-win-x64.zip");
    let urls = ordered_bases
        .iter()
        .map(|(id, base)| {
            (
                format!("{base}/v{version}/{filename}"),
                format!("Node.js ({id})"),
            )
        })
        .collect();
    Ok(OnlineArtifact { sha256, urls })
}

// ---------------------------------------------------------------------------
// go：https://go.dev/dl/?mode=json&include=all（files[].sha256 权威）
// ---------------------------------------------------------------------------

const GO_JSON_SOURCES: &[&str] = &[
    "https://golang.google.cn/dl/?mode=json&include=all",
    "https://go.dev/dl/?mode=json&include=all",
];

fn fetch_go_payload() -> Result<Value, String> {
    let mut last_err = String::from("no source");
    for url in GO_JSON_SOURCES {
        match http_get_text(url) {
            Ok(text) => match serde_json::from_str::<Value>(&text) {
                Ok(v) => return Ok(v),
                Err(e) => last_err = format!("{url}: bad json {e}"),
            },
            Err(e) => last_err = e.to_string(),
        }
    }
    Err(last_err)
}

fn fetch_go_versions() -> Result<Vec<String>, String> {
    let arr = fetch_go_payload()?.as_array().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for release in &arr {
        let ver = release
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim_start_matches("go");
        if ver.is_empty() {
            continue;
        }
        out.push(ver.to_string());
    }
    Ok(out)
}

fn go_artifact(version: &str, mirror: &str) -> Result<OnlineArtifact, String> {
    let arr = fetch_go_payload()?.as_array().cloned().unwrap_or_default();
    for release in &arr {
        let ver = release
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim_start_matches('v')
            .trim_start_matches("go");
        if ver != version {
            continue;
        }
        for f in release
            .get("files")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new())
        {
            let kind = f.get("kind").and_then(Value::as_str).unwrap_or("");
            let os = f.get("os").and_then(Value::as_str).unwrap_or("");
            let arch = f.get("arch").and_then(Value::as_str).unwrap_or("");
            if kind != "archive" || os != "windows" || arch != "amd64" {
                continue;
            }
            let filename = f
                .get("filename")
                .and_then(Value::as_str)
                .ok_or("go file missing filename")?;
            let sha256 = f
                .get("sha256")
                .and_then(Value::as_str)
                .ok_or("go file missing sha256")?
                .to_lowercase();
            let mut urls = Vec::new();
            if mirror == "aliyun" {
                urls.push((
                    format!("https://mirrors.aliyun.com/golang/{filename}"),
                    "Go (阿里云)".into(),
                ));
            }
            urls.push((
                format!("https://golang.google.cn/dl/{filename}"),
                "Go (Google中国)".into(),
            ));
            urls.push((format!("https://go.dev/dl/{filename}"), "Go (官方)".into()));
            return Ok(OnlineArtifact { sha256, urls });
        }
    }
    Err(format!("go {version} not found in dl json"))
}

// ---------------------------------------------------------------------------
// java：Adoptium v3 latest-per-feature（package.checksum 权威）
// ---------------------------------------------------------------------------

const JAVA_FEATURES: &[u32] = &[17, 21, 25];

fn java_feature_for_version(version: &str) -> Option<u32> {
    let major: u32 = version.split('.').next()?.parse().ok()?;
    JAVA_FEATURES.iter().copied().find(|f| *f == major)
}

fn adoptium_url(feature: u32) -> String {
    format!(
        "https://api.adoptium.net/v3/assets/latest/{feature}/hotspot?architecture=x64&image_type=jdk&os=windows&vendor=eclipse"
    )
}

fn fetch_java_versions() -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for &feature in JAVA_FEATURES {
        let text = http_get_text(&adoptium_url(feature))?;
        let arr: Value = serde_json::from_str(&text).map_err(|e| format!("bad api json: {e}"))?;
        if let Some(items) = arr.as_array() {
            for item in items {
                let release = item
                    .get("release_name")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                // 形如 jdk-21.0.5+11
                let ver = release.trim_start_matches("jdk-");
                if !ver.is_empty() {
                    out.push(ver.to_string());
                }
            }
        }
    }
    Ok(out)
}

fn java_artifact(version: &str) -> Result<OnlineArtifact, String> {
    let feature = java_feature_for_version(version)
        .ok_or_else(|| format!("java {version}: unsupported feature"))?;
    let text = http_get_text(&adoptium_url(feature))?;
    let arr: Value = serde_json::from_str(&text).map_err(|e| format!("bad api json: {e}"))?;
    for item in arr.as_array().unwrap_or(&Vec::new()) {
        let release = item
            .get("release_name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim_start_matches("jdk-");
        if release != version {
            continue;
        }
        let package = item
            .pointer("/binaries/0/package")
            .ok_or("java binary package missing")?;
        let link = package
            .get("link")
            .and_then(Value::as_str)
            .ok_or("java package missing link")?;
        let sha256 = package
            .get("checksum")
            .and_then(Value::as_str)
            .map(str::to_lowercase)
            .ok_or("java package missing checksum")?;
        return Ok(OnlineArtifact {
            sha256,
            urls: vec![(link.to_string(), "JDK (官方)".into())],
        });
    }
    Err(format!("java {version} not found in Adoptium api"))
}

// ---------------------------------------------------------------------------
// 扩展运行时：RubyInstaller、Deno、Bun 使用 GitHub Release 的 digest，
// Zig 使用官方 download index.json 的 shasum。摘要来自发布方，缺失时拒绝安装。
// ---------------------------------------------------------------------------

fn fetch_github_versions(repo: &str, prefix: &str) -> Result<Vec<String>, String> {
    let url = format!("https://api.github.com/repos/{repo}/releases?per_page=100");
    let payload: Value = serde_json::from_str(&http_get_text(&url)?)
        .map_err(|e| format!("GitHub release json invalid: {e}"))?;
    let mut versions = Vec::new();
    for release in payload.as_array().unwrap_or(&Vec::new()) {
        if release
            .get("draft")
            .and_then(Value::as_bool)
            .unwrap_or(true)
            || release
                .get("prerelease")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        {
            continue;
        }
        let tag = release
            .get("tag_name")
            .and_then(Value::as_str)
            .unwrap_or("");
        if let Some(version) = tag.strip_prefix(prefix) {
            if !version.is_empty() && version.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                let normalized = if repo == "oneclick/rubyinstaller2" {
                    version.split('-').next().unwrap_or(version)
                } else {
                    version
                };
                if !versions.iter().any(|v| v == normalized) {
                    versions.push(normalized.to_string());
                }
            }
        }
    }
    Ok(versions)
}

fn github_binary_artifact(
    repo: &str,
    tag: &str,
    predicate: impl Fn(&str) -> bool,
    label: &str,
) -> Result<OnlineArtifact, String> {
    let url = format!("https://api.github.com/repos/{repo}/releases/tags/{tag}");
    let payload: Value = serde_json::from_str(&http_get_text(&url)?)
        .map_err(|e| format!("GitHub release json invalid: {e}"))?;
    for asset in payload
        .get("assets")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
    {
        let name = asset.get("name").and_then(Value::as_str).unwrap_or("");
        if !predicate(name) {
            continue;
        }
        let digest = asset
            .get("digest")
            .and_then(Value::as_str)
            .and_then(|v| v.strip_prefix("sha256:"))
            .map(str::to_lowercase)
            .filter(|v| v.len() == 64)
            .ok_or_else(|| format!("GitHub asset {name} has no SHA-256 digest"))?;
        let browser_download_url = asset
            .get("browser_download_url")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("GitHub asset {name} has no download URL"))?;
        return Ok(OnlineArtifact {
            sha256: digest,
            urls: vec![(browser_download_url.to_string(), format!("{label} (官方)"))],
        });
    }
    Err(format!(
        "GitHub release {repo}@{tag} has no matching Windows x64 asset"
    ))
}

fn ruby_artifact(version: &str) -> Result<OnlineArtifact, String> {
    let url = "https://api.github.com/repos/oneclick/rubyinstaller2/releases?per_page=100";
    let payload: Value = serde_json::from_str(&http_get_text(url)?)
        .map_err(|e| format!("RubyInstaller release json invalid: {e}"))?;
    for release in payload.as_array().unwrap_or(&Vec::new()) {
        let tag = release
            .get("tag_name")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !tag.starts_with(&format!("RubyInstaller-{version}-")) {
            continue;
        }
        if let Ok(artifact) = github_binary_artifact(
            "oneclick/rubyinstaller2",
            tag,
            |name| {
                name.starts_with(&format!("rubyinstaller-{version}-")) && name.ends_with("-x64.exe")
            },
            "Ruby",
        ) {
            return Ok(artifact);
        }
    }
    Err(format!(
        "RubyInstaller {version} Windows x64 asset not found"
    ))
}

fn fetch_zig_versions() -> Result<Vec<String>, String> {
    let payload: Value =
        serde_json::from_str(&http_get_text("https://ziglang.org/download/index.json")?)
            .map_err(|e| format!("Zig index json invalid: {e}"))?;
    Ok(payload
        .as_object()
        .map(|items| {
            items
                .keys()
                .filter(|version| version.chars().next().is_some_and(|c| c.is_ascii_digit()))
                .cloned()
                .collect()
        })
        .unwrap_or_default())
}

fn zig_artifact(version: &str) -> Result<OnlineArtifact, String> {
    let payload: Value =
        serde_json::from_str(&http_get_text("https://ziglang.org/download/index.json")?)
            .map_err(|e| format!("Zig index json invalid: {e}"))?;
    let target = payload
        .get(version)
        .and_then(|v| v.get("x86_64-windows"))
        .ok_or_else(|| format!("Zig {version} Windows x64 asset not found"))?;
    let tarball = target
        .get("tarball")
        .and_then(Value::as_str)
        .ok_or("Zig tarball URL missing")?;
    let sha256 = target
        .get("shasum")
        .and_then(Value::as_str)
        .map(str::to_lowercase)
        .filter(|v| v.len() == 64)
        .ok_or("Zig tarball SHA-256 missing")?;
    Ok(OnlineArtifact {
        sha256,
        urls: vec![(tarball.to_string(), "Zig (官方)".into())],
    })
}

// ---------------------------------------------------------------------------
// 版本比较（降序工具函数）：分段数值比较，段数不足补 0；非数字段按字符串排最后
// ---------------------------------------------------------------------------

pub fn compare_version_desc(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<(u64, String)> {
        s.trim_start_matches('v')
            .split(['.', '+'])
            .map(|seg| match seg.parse::<u64>() {
                Ok(n) => (n, String::new()),
                Err(_) => (u64::MAX, seg.to_string()), // 非数字段排最后
            })
            .collect()
    };
    let mut va = parse(a);
    let mut vb = parse(b);
    let len = va.len().max(vb.len());
    va.resize(len, (0, String::new()));
    vb.resize(len, (0, String::new()));
    va.cmp(&vb)
}
