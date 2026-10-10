use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

pub const WIRE_VERSION: u32 = 1;
pub const MAX_FRAME: usize = 64 * 1024;
pub const MAX_REFERENCE: u64 = 64 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Operation {
    Status,
    #[cfg(feature = "acceptance-faults")]
    Fault {
        kind: String,
    },
    Parse {
        path: String,
    },
    Render {
        path: String,
        page: u32,
    },
    Split {
        path: String,
        pages_per_file: usize,
        ranges: Option<Vec<(usize, usize)>>,
    },
    Merge {
        paths: Vec<String>,
    },
    Rotate {
        path: String,
        pages: Vec<usize>,
        degrees: u16,
    },
    Reorder {
        path: String,
        pages: Vec<usize>,
    },
    ExtractImages {
        path: String,
    },
    Convert {
        document: Value,
        target: String,
    },
    Export {
        document: Value,
        stem: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub wire_version: u32,
    pub nonce: String,
    pub request_id: String,
    pub input: Reference,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub wire_version: u32,
    pub nonce: String,
    pub request_id: String,
    pub result: Option<Reference>,
    pub error: Option<String>,
}
pub fn write_reference(path: &Path, value: &impl Serialize) -> Result<Reference, String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_REFERENCE {
        return Err("DOCUMENT_REFERENCE_BUDGET".into());
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut file, &bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(Reference {
        bytes: bytes.len() as u64,
        sha256: format!("{:x}", Sha256::digest(&bytes)),
    })
}
pub fn read_reference<T: serde::de::DeserializeOwned>(
    path: &Path,
    reference: &Reference,
) -> Result<T, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file()
        || meta.file_type().is_symlink()
        || meta.len() != reference.bytes
        || reference.bytes > MAX_REFERENCE
    {
        return Err("INVALID_DOCUMENT_REFERENCE".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err("INVALID_DOCUMENT_REFERENCE".into());
        }
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(MAX_REFERENCE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 != reference.bytes
        || format!("{:x}", Sha256::digest(&bytes)) != reference.sha256
    {
        return Err("INVALID_DOCUMENT_REFERENCE".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
pub fn read_frame(input: impl Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut input = std::io::BufReader::new(input).take(MAX_FRAME as u64 + 1);
    use std::io::BufRead;
    input
        .read_until(b'\n', &mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME || bytes.last() != Some(&b'\n') {
        return Err("INVALID_DOCUMENT_FRAME".into());
    }
    Ok(bytes)
}

/// Bounded artifact inventory travels inside the digest-protected result reference.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Completed {
    pub value: serde_json::Value,
    pub artifacts: Vec<Artifact>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub path: std::path::PathBuf,
    pub reference: Reference,
}
pub fn artifact_reference(path: &Path) -> Result<Reference, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 512 * 1024 * 1024 {
        return Err("DOCUMENT_ARTIFACT_BUDGET_OR_TYPE".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err("DOCUMENT_ARTIFACT_LINK".into());
        }
    }
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        bytes += read as u64;
        if bytes > 512 * 1024 * 1024 {
            return Err("DOCUMENT_ARTIFACT_BUDGET".into());
        }
        digest.update(&buffer[..read]);
    }
    if bytes != meta.len() {
        return Err("DOCUMENT_ARTIFACT_CHANGED".into());
    }
    Ok(Reference {
        bytes,
        sha256: format!("{:x}", digest.finalize()),
    })
}
pub fn collect_artifacts(root: &Path) -> Result<Vec<Artifact>, String> {
    fn visit(
        root: &Path,
        directory: &Path,
        depth: usize,
        output: &mut Vec<Artifact>,
        bytes: &mut u64,
    ) -> Result<(), String> {
        if depth > 8 {
            return Err("DOCUMENT_ARTIFACT_DEPTH".into());
        }
        for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() {
                return Err("DOCUMENT_ARTIFACT_LINK".into());
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err("DOCUMENT_ARTIFACT_LINK".into());
                }
            }
            if meta.is_dir() {
                visit(root, &path, depth + 1, output, bytes)?;
            } else {
                if output.len() >= 4096 {
                    return Err("DOCUMENT_ARTIFACT_COUNT".into());
                }
                let reference = artifact_reference(&path)?;
                *bytes += reference.bytes;
                if *bytes > 2 * 1024 * 1024 * 1024 {
                    return Err("DOCUMENT_ARTIFACT_TOTAL".into());
                }
                output.push(Artifact {
                    path: path.strip_prefix(root).map_err(|e| e.to_string())?.into(),
                    reference,
                });
            }
        }
        Ok(())
    }
    let mut output = Vec::new();
    visit(root, root, 0, &mut output, &mut 0)?;
    Ok(output)
}
