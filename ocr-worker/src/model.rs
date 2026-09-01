//! Model path resolution and integrity metadata.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

pub const LEGACY_DETECTION_MODEL: &str = "ch_PP-OCRv4_det.onnx";
pub const LEGACY_RECOGNITION_MODEL: &str = "ch_PP-OCRv4_rec.onnx";
pub const LEGACY_DICTIONARY: &str = "ppocr_keys_v1.txt";
pub const SMALL_DETECTION_MODEL: &str = "ppocrv6_small_det.onnx";
pub const MOBILE_RECOGNITION_MODEL: &str = "en_PP-OCRv5_mobile_rec.onnx";
pub const MOBILE_DICTIONARY: &str = "en_ppocrv5_dict.txt";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelProfile {
    PpOcrv6SmallDetV5MobileRec,
    PpOcrv4MobileZhEn,
}

impl ModelProfile {
    pub fn id(self) -> &'static str {
        match self {
            Self::PpOcrv6SmallDetV5MobileRec => "ppocrv6-small-det-v5-mobile-rec",
            Self::PpOcrv4MobileZhEn => "ppocrv4-mobile-zh-en",
        }
    }

    fn from_id(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ppocrv6-small-det-v5-mobile-rec" | "ppocrv6" | "default" | "auto" => {
                Some(Self::PpOcrv6SmallDetV5MobileRec)
            }
            "ppocrv4-mobile-zh-en" | "ppocrv4" | "legacy" => Some(Self::PpOcrv4MobileZhEn),
            _ => None,
        }
    }

    pub fn detection_name(self) -> &'static str {
        match self {
            Self::PpOcrv6SmallDetV5MobileRec => SMALL_DETECTION_MODEL,
            Self::PpOcrv4MobileZhEn => LEGACY_DETECTION_MODEL,
        }
    }

    pub fn recognition_name(self) -> &'static str {
        match self {
            Self::PpOcrv6SmallDetV5MobileRec => MOBILE_RECOGNITION_MODEL,
            Self::PpOcrv4MobileZhEn => LEGACY_RECOGNITION_MODEL,
        }
    }

    pub fn dictionary_name(self) -> &'static str {
        match self {
            Self::PpOcrv6SmallDetV5MobileRec => MOBILE_DICTIONARY,
            Self::PpOcrv4MobileZhEn => LEGACY_DICTIONARY,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelPaths {
    pub profile: ModelProfile,
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
        requested_profile: Option<&str>,
        requested_language: Option<&str>,
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
        let profile = if let Some(requested) = requested_profile
            .filter(|value| !value.trim().is_empty())
            .filter(|value| !value.eq_ignore_ascii_case("auto"))
        {
            ModelProfile::from_id(requested)
                .ok_or_else(|| format!("unsupported model profile: {requested}"))?
        } else if requested_language.is_some_and(|language| {
            matches!(language.to_ascii_lowercase().as_str(), "ch" | "zh" | "mix")
        }) && [
            directory.join(LEGACY_DETECTION_MODEL),
            directory.join(LEGACY_RECOGNITION_MODEL),
            directory.join(LEGACY_DICTIONARY),
        ]
        .iter()
        .all(|path| path.is_file())
        {
            ModelProfile::PpOcrv4MobileZhEn
        } else if [
            directory.join(SMALL_DETECTION_MODEL),
            directory.join(MOBILE_RECOGNITION_MODEL),
            directory.join(MOBILE_DICTIONARY),
        ]
        .iter()
        .all(|path| path.is_file())
        {
            ModelProfile::PpOcrv6SmallDetV5MobileRec
        } else {
            ModelProfile::PpOcrv4MobileZhEn
        };
        let dictionary = dictionary_path
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| directory.join(profile.dictionary_name()));
        let paths = Self {
            profile,
            detection: directory.join(profile.detection_name()),
            recognition: directory.join(profile.recognition_name()),
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
            if manifest
                .profile
                .as_deref()
                .is_some_and(|profile| profile != self.profile.id())
            {
                return Err(format!(
                    "model manifest profile does not match selected profile {}",
                    self.profile.id()
                ));
            }
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
    #[serde(default)]
    profile: Option<String>,
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
            profile: ModelProfile::PpOcrv4MobileZhEn,
            directory: PathBuf::from("m"),
            detection: PathBuf::from("m/det.onnx"),
            recognition: PathBuf::from("m/rec.onnx"),
            dictionary: PathBuf::from("m/dict.txt"),
        };
        assert_eq!(paths.directory, PathBuf::from("m"));
    }

    #[test]
    fn chinese_language_selects_legacy_profile_when_available() {
        let directory = std::env::temp_dir().join(format!(
            "cruciblebox-model-profile-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        for name in [
            LEGACY_DETECTION_MODEL,
            LEGACY_RECOGNITION_MODEL,
            LEGACY_DICTIONARY,
        ] {
            fs::write(directory.join(name), b"model").unwrap();
        }
        let paths =
            ModelPaths::resolve(directory.to_str(), None, Some("auto"), Some("zh")).unwrap();
        assert_eq!(paths.profile, ModelProfile::PpOcrv4MobileZhEn);
        let _ = fs::remove_dir_all(directory);
    }
}
