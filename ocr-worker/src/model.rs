//! Model path resolution and integrity metadata.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

pub const LEGACY_DETECTION_MODEL: &str = "ch_PP-OCRv4_det.onnx";
pub const LEGACY_RECOGNITION_MODEL: &str = "ch_PP-OCRv4_rec.onnx";
pub const LEGACY_DICTIONARY: &str = "ppocr_keys_v1.txt";
pub const SMALL_DETECTION_MODEL: &str = "ppocrv6_small_det.onnx";
pub const MOBILE_RECOGNITION_MODEL: &str = "PP-OCRv5_mobile_rec.onnx";
pub const MOBILE_DICTIONARY: &str = "ppocrv5_dict.txt";
pub const ENGLISH_RECOGNITION_MODEL: &str = "en_PP-OCRv5_mobile_rec.onnx";
pub const ENGLISH_DICTIONARY: &str = "en_ppocrv5_dict.txt";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelProfile {
    PpOcrv6SmallDetV5MobileRec,
    PpOcrv6SmallDetV5EnglishRec,
    PpOcrv4MobileZhEn,
}

impl ModelProfile {
    pub fn id(self) -> &'static str {
        match self {
            Self::PpOcrv6SmallDetV5MobileRec => "ppocrv6-small-det-v5-mobile-rec",
            Self::PpOcrv6SmallDetV5EnglishRec => "ppocrv6-small-det-v5-en-rec",
            Self::PpOcrv4MobileZhEn => "ppocrv4-mobile-zh-en",
        }
    }

    fn from_id(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ppocrv6-small-det-v5-mobile-rec" | "ppocrv6" | "default" | "auto" => {
                Some(Self::PpOcrv6SmallDetV5MobileRec)
            }
            "ppocrv6-small-det-v5-en-rec" | "ppocrv6-en" | "english" | "en" => {
                Some(Self::PpOcrv6SmallDetV5EnglishRec)
            }
            "ppocrv4-mobile-zh-en" | "ppocrv4" | "legacy" => Some(Self::PpOcrv4MobileZhEn),
            _ => None,
        }
    }

    pub fn detection_name(self) -> &'static str {
        match self {
            Self::PpOcrv6SmallDetV5MobileRec => SMALL_DETECTION_MODEL,
            Self::PpOcrv6SmallDetV5EnglishRec => SMALL_DETECTION_MODEL,
            Self::PpOcrv4MobileZhEn => LEGACY_DETECTION_MODEL,
        }
    }

    pub fn recognition_name(self) -> &'static str {
        match self {
            Self::PpOcrv6SmallDetV5MobileRec => MOBILE_RECOGNITION_MODEL,
            Self::PpOcrv6SmallDetV5EnglishRec => ENGLISH_RECOGNITION_MODEL,
            Self::PpOcrv4MobileZhEn => LEGACY_RECOGNITION_MODEL,
        }
    }

    pub fn dictionary_name(self) -> &'static str {
        match self {
            Self::PpOcrv6SmallDetV5MobileRec => MOBILE_DICTIONARY,
            Self::PpOcrv6SmallDetV5EnglishRec => ENGLISH_DICTIONARY,
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
        } else if [
            directory.join(SMALL_DETECTION_MODEL),
            directory.join(MOBILE_RECOGNITION_MODEL),
            directory.join(MOBILE_DICTIONARY),
        ]
        .iter()
        .all(|path| path.is_file())
        {
            ModelProfile::PpOcrv6SmallDetV5MobileRec
        } else if [
            directory.join(SMALL_DETECTION_MODEL),
            directory.join(ENGLISH_RECOGNITION_MODEL),
            directory.join(ENGLISH_DICTIONARY),
        ]
        .iter()
        .all(|path| path.is_file())
            && requested_language.is_some_and(|language| language.eq_ignore_ascii_case("en"))
        {
            ModelProfile::PpOcrv6SmallDetV5EnglishRec
        } else if [
            directory.join(LEGACY_DETECTION_MODEL),
            directory.join(LEGACY_RECOGNITION_MODEL),
            directory.join(LEGACY_DICTIONARY),
        ]
        .iter()
        .all(|path| path.is_file())
        {
            ModelProfile::PpOcrv4MobileZhEn
        } else {
            return Err(format!(
                "通用 PP-OCRv5 模型未安装：需要 {}、{} 和 {}；英文优化模式请显式选择 english",
                MOBILE_RECOGNITION_MODEL, MOBILE_DICTIONARY, SMALL_DETECTION_MODEL
            ));
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
        let profile_manifest_path = self
            .directory
            .join(format!("model-manifest-{}.json", self.profile.id()));
        let legacy_manifest_path = self.directory.join("model-manifest.json");
        // The original manifest filename predates model profiles and therefore
        // describes only the legacy v4 triplet. A directory may legitimately
        // contain several profiles; do not compare that legacy manifest with a
        // newly selected v5/v6 profile.
        let manifest_path = if profile_manifest_path.is_file() {
            Some(profile_manifest_path)
        } else if self.profile == ModelProfile::PpOcrv4MobileZhEn && legacy_manifest_path.is_file()
        {
            Some(legacy_manifest_path)
        } else {
            None
        };
        if let Some(manifest_path) = manifest_path {
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

    #[test]
    fn auto_prefers_generic_v5_over_english_and_legacy_profiles() {
        let directory = std::env::temp_dir().join(format!(
            "cruciblebox-model-profile-generic-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        for name in [
            SMALL_DETECTION_MODEL,
            MOBILE_RECOGNITION_MODEL,
            MOBILE_DICTIONARY,
            ENGLISH_RECOGNITION_MODEL,
            ENGLISH_DICTIONARY,
        ] {
            fs::write(directory.join(name), b"model").unwrap();
        }
        let paths =
            ModelPaths::resolve(directory.to_str(), None, Some("auto"), Some("mix")).unwrap();
        assert_eq!(paths.profile, ModelProfile::PpOcrv6SmallDetV5MobileRec);
        assert_eq!(
            paths.recognition.file_name().unwrap(),
            MOBILE_RECOGNITION_MODEL
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn legacy_unscoped_manifest_does_not_reject_another_installed_profile() {
        let directory = std::env::temp_dir().join(format!(
            "cruciblebox-model-manifest-profiles-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        for name in [
            SMALL_DETECTION_MODEL,
            MOBILE_RECOGNITION_MODEL,
            MOBILE_DICTIONARY,
        ] {
            fs::write(directory.join(name), b"current-model").unwrap();
        }
        fs::write(
            directory.join("model-manifest.json"),
            r#"{"detectionSha256":"old","recognitionSha256":"old","dictionarySha256":"old"}"#,
        )
        .unwrap();

        let paths =
            ModelPaths::resolve(directory.to_str(), None, Some("auto"), Some("mix")).unwrap();
        assert!(paths.metadata().is_ok());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn profile_scoped_manifest_remains_fail_closed() {
        let directory = std::env::temp_dir().join(format!(
            "cruciblebox-profile-manifest-integrity-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        for name in [
            SMALL_DETECTION_MODEL,
            MOBILE_RECOGNITION_MODEL,
            MOBILE_DICTIONARY,
        ] {
            fs::write(directory.join(name), b"current-model").unwrap();
        }
        fs::write(
            directory.join(format!(
                "model-manifest-{}.json",
                ModelProfile::PpOcrv6SmallDetV5MobileRec.id()
            )),
            format!(
                r#"{{"profile":"{}","detectionSha256":"bad","recognitionSha256":"bad","dictionarySha256":"bad"}}"#,
                ModelProfile::PpOcrv6SmallDetV5MobileRec.id()
            ),
        )
        .unwrap();

        let paths =
            ModelPaths::resolve(directory.to_str(), None, Some("auto"), Some("mix")).unwrap();
        assert!(paths.metadata().unwrap_err().contains("SHA-256"));
        let _ = fs::remove_dir_all(directory);
    }
}
