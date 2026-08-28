//! Model path resolution and integrity metadata.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

pub const DETECTION_MODEL: &str = "ch_PP-OCRv4_det.onnx";
pub const RECOGNITION_MODEL: &str = "ch_PP-OCRv4_rec.onnx";
pub const DEFAULT_DICTIONARY: &str = "ppocr_keys_v1.txt";

#[derive(Debug, Clone)]
pub struct ModelPaths {
    pub directory: PathBuf,
    pub detection: PathBuf,
    pub recognition: PathBuf,
    pub dictionary: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ModelMetadata {
    pub detection_sha256: String,
    pub recognition_sha256: String,
    pub dictionary_sha256: String,
}

impl ModelPaths {
    pub fn resolve(
        model_directory: Option<&str>,
        dictionary_path: Option<&str>,
    ) -> Result<Self, String> {
        let directory = model_directory
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("OCR_WORKER_MODEL_DIR").map(PathBuf::from))
            .or_else(|| {
                std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|p| p.join("models")))
            })
            .ok_or_else(|| "model directory is not configured".to_string())?;
        let dictionary = dictionary_path
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| directory.join(DEFAULT_DICTIONARY));
        let paths = Self {
            detection: directory.join(DETECTION_MODEL),
            recognition: directory.join(RECOGNITION_MODEL),
            dictionary,
            directory,
        };
        paths.validate()?;
        Ok(paths)
    }

    pub fn validate(&self) -> Result<(), String> {
        for (label, path) in [
            ("detection model", &self.detection),
            ("recognition model", &self.recognition),
            ("dictionary", &self.dictionary),
        ] {
            let meta = fs::metadata(path)
                .map_err(|e| format!("{label} unavailable at {}: {e}", path.display()))?;
            if !meta.is_file() || meta.len() == 0 {
                return Err(format!(
                    "{label} is not a non-empty regular file: {}",
                    path.display()
                ));
            }
        }
        Ok(())
    }

    pub fn metadata(&self) -> Result<ModelMetadata, String> {
        self.validate()?;
        let metadata = ModelMetadata {
            detection_sha256: sha256_file(&self.detection)?,
            recognition_sha256: sha256_file(&self.recognition)?,
            dictionary_sha256: sha256_file(&self.dictionary)?,
        };
        let manifest_path = self.directory.join("model-manifest.json");
        if manifest_path.is_file() {
            let bytes =
                fs::read(&manifest_path).map_err(|e| format!("model manifest read failed: {e}"))?;
            let manifest: ModelManifest = serde_json::from_slice(&bytes)
                .map_err(|e| format!("model manifest is invalid JSON: {e}"))?;
            if manifest.detection_sha256 != metadata.detection_sha256
                || manifest.recognition_sha256 != metadata.recognition_sha256
                || manifest.dictionary_sha256 != metadata.dictionary_sha256
            {
                return Err("model manifest SHA-256 does not match installed files".into());
            }
        }
        Ok(metadata)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelManifest {
    detection_sha256: String,
    recognition_sha256: String,
    dictionary_sha256: String,
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("failed to hash {}: {e}", path.display()))?;
    let mut h = Sha256::new();
    h.update(bytes);
    Ok(format!("{:x}", h.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_model_directory_is_used_without_machine_specific_fallback() {
        let paths = ModelPaths {
            directory: PathBuf::from("m"),
            detection: PathBuf::from("m/det.onnx"),
            recognition: PathBuf::from("m/rec.onnx"),
            dictionary: PathBuf::from("m/dict.txt"),
        };
        assert_eq!(paths.directory, PathBuf::from("m"));
    }
}
