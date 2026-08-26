// UniEnv 制品完整性目录（1.9.11 阶段 B）
// 静态移植自冻结线 plugin-system/trusted-services/unienv/artifact-integrity.ts 与
// tools/{python,node,git,go,java}.ts 的镜像 URL 列表。SHA-256 与 URL 模板必须与
// Electron 线逐字节一致；新增版本时两侧同步维护。

pub struct ToolArtifact {
    pub filename: &'static str,
    pub sha256: &'static str,
    pub release_tag: Option<&'static str>,
}

const PYTHON: [(&str, ToolArtifact); 6] = [
    (
        "3.8.10",
        ToolArtifact {
            filename: "python-3.8.10-amd64.exe",
            sha256: "7628244cb53408b50639d2c1287c659f4e29d3dfdb9084b11aed5870c0c6a48a",
            release_tag: None,
        },
    ),
    (
        "3.9.13",
        ToolArtifact {
            filename: "python-3.9.13-amd64.exe",
            sha256: "fb3d0466f3754752ca7fd839a09ffe53375ff2c981279fd4bc23a005458f7f5d",
            release_tag: None,
        },
    ),
    (
        "3.10.11",
        ToolArtifact {
            filename: "python-3.10.11-amd64.exe",
            sha256: "d8dede5005564b408ba50317108b765ed9c3c510342a598f9fd42681cbe0648b",
            release_tag: None,
        },
    ),
    (
        "3.11.9",
        ToolArtifact {
            filename: "python-3.11.9-amd64.exe",
            sha256: "5ee42c4eee1e6b4464bb23722f90b45303f79442df63083f05322f1785f5fdde",
            release_tag: None,
        },
    ),
    (
        "3.12.5",
        ToolArtifact {
            filename: "python-3.12.5-amd64.exe",
            sha256: "44810512af577ca70b3269b8570b10825ec2ace2b86e4297e767a0f4c0ee8bfd",
            release_tag: None,
        },
    ),
    (
        "3.14.7",
        ToolArtifact {
            filename: "python-3.14.7-amd64.exe",
            sha256: "9d9eb2709ef81bf5cd30db3c2096bdbc4ea10087c22e62f27d356b36f6ae9649",
            release_tag: None,
        },
    ),
];

const NODE: [(&str, ToolArtifact); 5] = [
    (
        "16.20.2",
        ToolArtifact {
            filename: "node-v16.20.2-win-x64.zip",
            sha256: "f8bb35f6c08dc7bf14ac753509c06ed1a7ebf5b390cd3fbdc8f8c1aedd020ec3",
            release_tag: None,
        },
    ),
    (
        "18.20.4",
        ToolArtifact {
            filename: "node-v18.20.4-win-x64.zip",
            sha256: "a2864d9048fb83cc85e3b2c3d18f5731b69cae8964bb029f5cdecbb0820eccd7",
            release_tag: None,
        },
    ),
    (
        "20.15.1",
        ToolArtifact {
            filename: "node-v20.15.1-win-x64.zip",
            sha256: "ba6c3711e2c3d0638c5f7cea3c234553808a73c52a5962a6cdb47b5210b70b04",
            release_tag: None,
        },
    ),
    (
        "22.5.1",
        ToolArtifact {
            filename: "node-v22.5.1-win-x64.zip",
            sha256: "71b74712aa5c6587c428b39d9ec9aa013bfcfa38a2a0ed8e68b3922dda1b69f4",
            release_tag: None,
        },
    ),
    (
        "24.18.1",
        ToolArtifact {
            filename: "node-v24.18.1-win-x64.zip",
            sha256: "ec56b84a7551893ab2324ebdfdc4ab974a63b4781162600b68a1293cc3e53765",
            release_tag: None,
        },
    ),
];

const GIT: [(&str, ToolArtifact); 5] = [
    (
        "2.43.0",
        ToolArtifact {
            filename: "Git-2.43.0-64-bit.exe",
            sha256: "a6058d7c4c16bfa5bcd6fde051a92de8c68535fd7ebade55fc0ab1c41be3c8d5",
            release_tag: Some("v2.43.0.windows.1"),
        },
    ),
    (
        "2.44.0",
        ToolArtifact {
            filename: "Git-2.44.0-64-bit.exe",
            sha256: "914ffc96cee0631d09049b9d87d4cd8ac9c98ead9a9f9a094d3341348324a9ec",
            release_tag: Some("v2.44.0.windows.1"),
        },
    ),
    (
        "2.45.2",
        ToolArtifact {
            filename: "Git-2.45.2-64-bit.exe",
            sha256: "ce022a6a19e58bbbd4823f51cf798b006b4a683b93b0616a7bb5beeee901da98",
            release_tag: Some("v2.45.2.windows.1"),
        },
    ),
    (
        "2.46.0",
        ToolArtifact {
            filename: "Git-2.46.0-64-bit.exe",
            sha256: "e6337d172590cea1f673acfeef218733e9352adeb863a3a9e8fa20ee0719a40f",
            release_tag: Some("v2.46.0.windows.1"),
        },
    ),
    (
        "2.54.0",
        ToolArtifact {
            filename: "Git-2.54.0-64-bit.exe",
            sha256: "2b96e7854f0520f0f6b709c21041d9801b1be44d5e1a0d9fa621b2fbc40f1983",
            release_tag: Some("v2.54.0.windows.1"),
        },
    ),
];

const GO: [(&str, ToolArtifact); 4] = [
    (
        "1.21.6",
        ToolArtifact {
            filename: "go1.21.6.windows-amd64.zip",
            sha256: "27ac9dd6e66fb3fd0acfa6792ff053c86e7d2c055b022f4b5d53bfddec9e3301",
            release_tag: None,
        },
    ),
    (
        "1.22.4",
        ToolArtifact {
            filename: "go1.22.4.windows-amd64.zip",
            sha256: "26321c4d945a0035d8a5bc4a1965b0df401ff8ceac66ce2daadabf9030419a98",
            release_tag: None,
        },
    ),
    (
        "1.23.0",
        ToolArtifact {
            filename: "go1.23.0.windows-amd64.zip",
            sha256: "d4be481ef73079ee0ad46081d278923aa3fd78db1b3cf147172592f73e14c1ac",
            release_tag: None,
        },
    ),
    (
        "1.26.5",
        ToolArtifact {
            filename: "go1.26.5.windows-amd64.zip",
            sha256: "97e6b2a833b6d89f9ff17d25419ac0a7e3b482a044e9ab18cdef834bd834fd38",
            release_tag: None,
        },
    ),
];

const JAVA: [(&str, ToolArtifact); 8] = [
    (
        "17.0.11",
        ToolArtifact {
            filename: "OpenJDK17U-jdk_x64_windows_hotspot_17.0.11_9.zip",
            sha256: "fdd6664d4131370398fbc8bfbb7b46dbfec4a22a090a511fe5c379dae188c390",
            release_tag: Some("jdk-17.0.11+9"),
        },
    ),
    (
        "17.0.12",
        ToolArtifact {
            filename: "OpenJDK17U-jdk_x64_windows_hotspot_17.0.12_7.zip",
            sha256: "052049d687ebfda6a4032d54afcd0da6549a23bc2ed04cfaa509746eeacbae71",
            release_tag: Some("jdk-17.0.12+7"),
        },
    ),
    (
        "17.0.20",
        ToolArtifact {
            filename: "OpenJDK17U-jdk_x64_windows_hotspot_17.0.20_8.zip",
            sha256: "418497be5cf585bdd2203d6486a565d66d3f5e992d5630d45104cb873fab8122",
            release_tag: Some("jdk-17.0.20+8"),
        },
    ),
    (
        "21.0.3",
        ToolArtifact {
            filename: "OpenJDK21U-jdk_x64_windows_hotspot_21.0.3_9.zip",
            sha256: "c43a66cff7a403d56c5c5e1ff10d3d5f95961abf80f97f0e35380594909f0e4d",
            release_tag: Some("jdk-21.0.3+9"),
        },
    ),
    (
        "21.0.5",
        ToolArtifact {
            filename: "OpenJDK21U-jdk_x64_windows_hotspot_21.0.5_11.zip",
            sha256: "6f09d4a3598542313cca1540106d537c7092a54e415d569f7b928160a90d3128",
            release_tag: Some("jdk-21.0.5+11"),
        },
    ),
    (
        "21.0.12",
        ToolArtifact {
            filename: "OpenJDK21U-jdk_x64_windows_hotspot_21.0.12_8.zip",
            sha256: "9ba963ee2371874a74185d18bc7bb2ab9407df7683300855ed7606e0662321d0",
            release_tag: Some("jdk-21.0.12+8"),
        },
    ),
    (
        "22.0.1",
        ToolArtifact {
            filename: "OpenJDK22U-jdk_x64_windows_hotspot_22.0.1_8.zip",
            sha256: "4cf9d3c7ed8ec72a8adcca290d90fdd775100a38670410e674b05233a0c8288e",
            release_tag: Some("jdk-22.0.1+8"),
        },
    ),
    (
        "25.0.4",
        ToolArtifact {
            filename: "OpenJDK25U-jdk_x64_windows_hotspot_25.0.4_7.zip",
            sha256: "7caab7db43bf4b94a2e6252c699e70d90084f9aa7c943cd3414761fd540937ae",
            release_tag: Some("jdk-25.0.4+7"),
        },
    ),
];

pub fn artifact(tool: &str, version: &str) -> Result<ToolArtifact, String> {
    let table: &[(&str, ToolArtifact)] = match tool {
        "python" => &PYTHON,
        "node" => &NODE,
        "git" => &GIT,
        "go" => &GO,
        "java" => &JAVA,
        _ => return Err(format!("unsupported tool: {tool}")),
    };
    table
        .iter()
        .find(|(v, _)| *v == version)
        .map(|(_, a)| ToolArtifact {
            filename: a.filename,
            sha256: a.sha256,
            release_tag: a.release_tag,
        })
        .ok_or_else(|| format!("{tool} {version} 的制品完整性信息未维护"))
}

fn official_url(tool: &str, version: &str) -> Result<String, String> {
    let a = artifact(tool, version)?;
    Ok(match tool {
        "python" => format!("https://www.python.org/ftp/python/{version}/{}", a.filename),
        "node" => format!("https://nodejs.org/dist/v{version}/{}", a.filename),
        "go" => format!("https://go.dev/dl/{}", a.filename),
        "git" => {
            let tag = a
                .release_tag
                .ok_or_else(|| format!("Git {version} 的发布标签未维护"))?;
            format!(
                "https://github.com/git-for-windows/git/releases/download/{tag}/{}",
                a.filename
            )
        }
        "java" => {
            let tag = a
                .release_tag
                .ok_or_else(|| format!("JDK {version} 的发布标签未维护"))?;
            let major = version.split('.').next().unwrap_or("");
            // TS 侧用 encodeURIComponent(releaseTag)；'+' → %2B 是唯一实际差异点
            let encoded = tag.replace('+', "%2B");
            format!(
                "https://github.com/adoptium/temurin{major}-binaries/releases/download/{encoded}/{}",
                a.filename
            )
        }
        _ => return Err(format!("unsupported tool: {tool}")),
    })
}

/// 下载源列表（含镜像优先级），语义对齐各 tool ts 的 getXxxUrls：
/// mirror 只决定对应镜像是否前置；无镜像参数的固定中转源始终参与。
pub fn download_urls(
    tool: &str,
    version: &str,
    mirror: &str,
) -> Result<Vec<(String, String)>, String> {
    let a = artifact(tool, version)?;
    let mut urls: Vec<(String, String)> = Vec::new();
    match tool {
        "python" => {
            if mirror == "huawei" {
                urls.push((
                    format!(
                        "https://mirrors.huaweicloud.com/python/{version}/{}",
                        a.filename
                    ),
                    "Python (华为云)".into(),
                ));
            }
            if mirror == "tuna" {
                urls.push((
                    format!(
                        "https://mirrors.tuna.tsinghua.edu.cn/python/{version}/{}",
                        a.filename
                    ),
                    "Python (TUNA)".into(),
                ));
            }
            urls.push((official_url(tool, version)?, "Python (官方)".into()));
        }
        "node" => {
            if mirror == "huawei" {
                urls.push((
                    format!(
                        "https://mirrors.huaweicloud.com/nodejs/v{version}/{}",
                        a.filename
                    ),
                    "Node.js (华为云)".into(),
                ));
            }
            if mirror == "tuna" {
                urls.push((
                    format!(
                        "https://mirrors.tuna.tsinghua.edu.cn/nodejs-release/v{version}/{}",
                        a.filename
                    ),
                    "Node.js (TUNA)".into(),
                ));
            }
            urls.push((
                format!(
                    "https://npmmirror.com/mirrors/node/v{version}/{}",
                    a.filename
                ),
                "Node.js (淘宝NPM)".into(),
            ));
            urls.push((official_url(tool, version)?, "Node.js (官方)".into()));
        }
        "git" => {
            let tag = a
                .release_tag
                .ok_or_else(|| format!("Git {version} 的发布标签未维护"))?;
            if mirror == "tuna" {
                urls.push((
                    format!(
                        "https://mirrors.tuna.tsinghua.edu.cn/github-release/git-for-windows/git/{tag}/{}",
                        a.filename
                    ),
                    "Git (TUNA)".into(),
                ));
            }
            urls.push((official_url(tool, version)?, "Git (官方)".into()));
        }
        "go" => {
            if mirror == "aliyun" {
                urls.push((
                    format!("https://mirrors.aliyun.com/golang/{}", a.filename),
                    "Go (阿里云)".into(),
                ));
            }
            urls.push((
                format!("https://golang.google.cn/dl/{}", a.filename),
                "Go (Google中国)".into(),
            ));
            urls.push((official_url(tool, version)?, "Go (官方)".into()));
        }
        "java" => {
            let tag = a
                .release_tag
                .ok_or_else(|| format!("JDK {version} 的发布标签未维护"))?;
            let major = version.split('.').next().unwrap_or("");
            let encoded = tag.replace('+', "%2B");
            if mirror == "tuna" {
                urls.push((
                    format!(
                        "https://mirrors.tuna.tsinghua.edu.cn/github-release/adoptium/temurin{major}-binaries/{encoded}/{}",
                        a.filename
                    ),
                    "JDK (TUNA)".into(),
                ));
            }
            urls.push((official_url(tool, version)?, "JDK (官方)".into()));
        }
        _ => return Err(format!("unsupported tool: {tool}")),
    }
    Ok(urls)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_all_supported_versions() {
        let catalog: Vec<(&str, Vec<&str>)> = vec![
            (
                "python",
                vec!["3.8.10", "3.9.13", "3.10.11", "3.11.9", "3.12.5", "3.14.7"],
            ),
            (
                "node",
                vec!["16.20.2", "18.20.4", "20.15.1", "22.5.1", "24.18.1"],
            ),
            (
                "git",
                vec!["2.43.0", "2.44.0", "2.45.2", "2.46.0", "2.54.0"],
            ),
            ("go", vec!["1.21.6", "1.22.4", "1.23.0", "1.26.5"]),
            (
                "java",
                vec![
                    "17.0.11", "17.0.12", "17.0.20", "21.0.3", "21.0.5", "21.0.12", "22.0.1",
                    "25.0.4",
                ],
            ),
        ];
        for (tool, versions) in catalog {
            for v in versions {
                let a = artifact(tool, v).unwrap_or_else(|e| panic!("{tool} {v}: {e}"));
                assert_eq!(a.sha256.len(), 64);
                assert!(a
                    .sha256
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
            }
        }
        assert!(artifact("ruby", "1.0").is_err());
        assert!(artifact("go", "9.9.9").is_err());
    }

    #[test]
    fn official_urls_match_frozen_templates() {
        assert_eq!(
            official_url("python", "3.12.5").unwrap(),
            "https://www.python.org/ftp/python/3.12.5/python-3.12.5-amd64.exe"
        );
        assert_eq!(
            official_url("node", "24.18.1").unwrap(),
            "https://nodejs.org/dist/v24.18.1/node-v24.18.1-win-x64.zip"
        );
        assert_eq!(
            official_url("git", "2.54.0").unwrap(),
            "https://github.com/git-for-windows/git/releases/download/v2.54.0.windows.1/Git-2.54.0-64-bit.exe"
        );
        assert_eq!(
            official_url("java", "21.0.3").unwrap(),
            "https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.3%2B9/OpenJDK21U-jdk_x64_windows_hotspot_21.0.3_9.zip"
        );
    }

    #[test]
    fn mirror_lists_match_frozen_semantics() {
        let node_direct = download_urls("node", "24.18.1", "direct").unwrap();
        assert_eq!(node_direct.len(), 2);
        assert!(node_direct[0].0.contains("npmmirror.com"));
        let node_tuna = download_urls("node", "24.18.1", "tuna").unwrap();
        assert_eq!(node_tuna.len(), 3);
        assert!(node_tuna[0].0.contains("tuna"));

        let go_direct = download_urls("go", "1.26.5", "direct").unwrap();
        assert_eq!(go_direct.len(), 2);
        assert!(go_direct[0].0.contains("golang.google.cn"));
        let go_aliyun = download_urls("go", "1.26.5", "aliyun").unwrap();
        assert_eq!(go_aliyun.len(), 3);
        assert!(go_aliyun[0].0.contains("aliyun"));

        let py_huawei = download_urls("python", "3.12.5", "huawei").unwrap();
        assert_eq!(py_huawei.len(), 2);
        assert!(py_huawei[0].0.contains("huaweicloud"));

        let git_tuna = download_urls("git", "2.46.0", "tuna").unwrap();
        assert_eq!(git_tuna.len(), 2);
        assert!(git_tuna[0].0.contains("tuna"));

        let java_tuna = download_urls("java", "17.0.11", "tuna").unwrap();
        assert_eq!(java_tuna.len(), 2);
        assert!(java_tuna[0].0.contains("temurin17-binaries"));
    }
}
