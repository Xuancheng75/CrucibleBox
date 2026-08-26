// UniEnv 在线版本发现（1.9.12）
// 产品决策：node/go/java 上游提供权威 SHA-256，开放「在线新版本」安装；
// python/git 无机器可读校验源，仅内置目录（新版本等待插件更新）。
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
    matches!(tool, "node" | "go" | "java")
}

/// 在线可用版本号列表（降序）。失败返回空（调用方静默回退内置目录）。
pub fn online_versions(tool: &str) -> Vec<String> {
    {
        let cache = list_cache().lock().unwrap();
        if let Some(entry) = cache.get(tool) {
            if entry.at.elapsed() < CACHE_TTL {
                return entry.versions.clone();
            }
        }
    }
    let fetched = (|| -> Result<Vec<String>, String> {
        match tool {
            "node" => Ok(fetch_node_versions()?
                .into_iter()
                .map(|v| v.trim_start_matches('v').to_string())
                .collect()),
            "go" => fetch_go_versions(),
            "java" => fetch_java_versions(),
            _ => Err(format!("provider unsupported: {tool}")),
        }
    })();
    match fetched {
        Ok(mut versions) => {
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
        Err(e) => {
            eprintln!("[unienv] online version fetch failed for {tool}: {e}");
            Vec::new()
        }
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
