//! PaddleOCR CTC dictionary loading and decoding.

use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Dictionary {
    chars: Vec<String>,
    pub sha256: String,
}

impl Dictionary {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|e| format!("dictionary read failed: {e}"))?;
        let text =
            std::str::from_utf8(&bytes).map_err(|e| format!("dictionary is not UTF-8: {e}"))?;
        let mut chars = Vec::new();
        let mut seen = HashSet::new();
        for (line_no, raw) in text.lines().enumerate() {
            let ch = raw.trim_end_matches('\r');
            if ch.is_empty() {
                continue;
            }
            if !seen.insert(ch.to_string()) {
                return Err(format!(
                    "dictionary contains duplicate entry at line {}",
                    line_no + 1
                ));
            }
            chars.push(ch.to_string());
        }
        if chars.is_empty() {
            return Err("dictionary is empty".into());
        }
        let mut h = Sha256::new();
        h.update(&bytes);
        Ok(Self {
            chars,
            sha256: format!("{:x}", h.finalize()),
        })
    }

    pub fn len(&self) -> usize {
        self.chars.len()
    }

    /// Decode greedy CTC output. Class 0 is the blank class; dictionary index n
    /// is the class n+1. Repeated classes are collapsed before blank removal.
    pub fn decode(&self, classes: &[usize], probabilities: &[f32]) -> (String, f32) {
        let mut text = String::new();
        let mut confidence_sum = 0.0f32;
        let mut confidence_count = 0usize;
        let mut previous = 0usize;
        for (time, &class) in classes.iter().enumerate() {
            if class != 0 && class != previous {
                let ch = if class == self.chars.len() + 1 {
                    Some(" ")
                } else {
                    self.chars.get(class - 1).map(String::as_str)
                };
                if let Some(ch) = ch {
                    text.push_str(ch);
                    confidence_sum += probabilities.get(time).copied().unwrap_or(0.0);
                    confidence_count += 1;
                }
            }
            previous = class;
        }
        let confidence = if confidence_count == 0 {
            0.0
        } else {
            confidence_sum / confidence_count as f32
        };
        (text, confidence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_file(contents: &str) -> std::path::PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ocr-dict-{id}.txt"));
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn loads_and_decodes_ctc_dictionary() {
        let dict = Dictionary::load(&temp_file("你\n好\na\n")).unwrap();
        let (text, confidence) = dict.decode(&[1, 1, 0, 2, 3, 3], &[0.9, 0.8, 0.0, 0.7, 0.6, 0.5]);
        assert_eq!(text, "你好a");
        assert!(confidence > 0.7 && confidence < 0.8);
    }

    #[test]
    fn rejects_duplicate_dictionary_entries() {
        assert!(Dictionary::load(&temp_file("a\na\n")).is_err());
    }
}
