//! User-owned theme data. Independent of plugin code/resources and retained on uninstall.
use image::ImageReader;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufReader, Cursor, Read},
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{Manager, State, WebviewWindow};

pub const SCHEME: &str = "cruciblebox-theme";
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 32 * 1024 * 1024;

/// Only a file in the caller's already verified package inventory may become a preset asset.
pub fn bundled_path(
    root: &Path,
    manifest: &cruciblebox_next2_protocol::Manifest,
    relative: &str,
) -> Result<PathBuf, String> {
    if !manifest.files.iter().any(|file| file.path == relative) {
        return Err("THEME_BUNDLED_ASSET_UNDECLARED".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err("THEME_BUNDLED_ASSET_OUTSIDE_PACKAGE".into());
    }
    Ok(path)
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Metadata {
    id: String,
    token: String,
    name: String,
    kind: String,
    mime: String,
    bytes: usize,
    license: String,
    notice: String,
    source: String,
    pixels: u64,
}
impl Metadata {
    fn descriptor(&self) -> Value {
        json!({"id":self.id,"name":self.name,"kind":self.kind,"mime":self.mime,
            "bytes":self.bytes,"license":self.license,"source":self.source,
            "url":format!("http://{SCHEME}.localhost/{}/{}",self.token,self.id)})
    }
}

pub struct ThemeAssets {
    root: PathBuf,
    importing: Mutex<()>,
}
fn identifier(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
impl ThemeAssets {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            importing: Mutex::new(()),
        }
    }
    fn metadata(&self, id: &str) -> Result<Metadata, String> {
        if !identifier(id) {
            return Err("THEME_ASSET_ID_INVALID".into());
        }
        let raw = fs::read(self.root.join(id).join("metadata.json")).map_err(|e| e.to_string())?;
        let meta: Metadata = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        if meta.id != id
            || !identifier(&meta.token)
            || meta.bytes == 0
            || meta.bytes > MAX_BYTES
            || meta.pixels > MAX_IMAGE_PIXELS
            || ![
                "image/png",
                "image/jpeg",
                "image/webp",
                "font/ttf",
                "font/otf",
                "font/woff2",
            ]
            .contains(&meta.mime.as_str())
        {
            return Err("THEME_ASSET_METADATA_INVALID".into());
        }
        Ok(meta)
    }
    pub fn get(&self, id: &str) -> Result<Value, String> {
        let meta = self.metadata(id)?;
        let mut result = meta.descriptor();
        result["notice"] = json!(meta.notice);
        Ok(result)
    }
    pub fn describe(&self, ids: &[String]) -> Result<Vec<Value>, String> {
        if ids.len() > 32 {
            return Err("THEME_ASSET_LIMIT".into());
        }
        let mut pixels = 0;
        let mut result = Vec::new();
        for id in ids {
            let meta = self.metadata(id)?;
            pixels += meta.pixels;
            result.push(meta.descriptor());
        }
        if pixels > 64 * 1024 * 1024 {
            return Err("THEME_IMAGE_PIXEL_BUDGET".into());
        }
        Ok(result)
    }
    pub fn validate_theme(&self, theme: &Value) -> Result<(), String> {
        let ids: Vec<_> = theme["design"]["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect();
        self.describe(&ids).map(|_| ())
    }
    pub fn list(&self, after: Option<&str>, limit: usize) -> Result<Value, String> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(json!({"assets":[],"next":null}))
            }
            Err(e) => return Err(e.to_string()),
        };
        let mut ids = Vec::new();
        for entry in entries {
            let name = entry
                .map_err(|e| e.to_string())?
                .file_name()
                .to_string_lossy()
                .into_owned();
            if identifier(&name) && after.is_none_or(|after| name.as_str() > after) {
                ids.push(name);
            }
        }
        ids.sort();
        let page: Vec<_> = ids
            .iter()
            .take(limit.clamp(1, 16))
            .map(|id| self.metadata(id).map(|meta| meta.descriptor()))
            .collect::<Result<_, _>>()?;
        let next = (ids.len() > page.len()).then(|| ids[page.len() - 1].clone());
        Ok(json!({"assets":page,"next":next}))
    }
    pub fn import(&self, path: &Path, params: &Value) -> Result<Value, String> {
        let _guard = self.importing.lock().map_err(|_| "THEME_ASSET_BUSY")?;
        let input = fs::File::open(path).map_err(|e| e.to_string())?;
        if !input.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("FILE_REQUIRED".into());
        }
        let mut bytes = Vec::new();
        input
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.is_empty() || bytes.len() > MAX_BYTES {
            return Err("THEME_ASSET_SIZE_LIMIT".into());
        }
        let (kind, mime, pixels) = inspect(&bytes)?;
        let license = params["license"]
            .as_str()
            .ok_or("THEME_ASSET_LICENSE_REQUIRED")?;
        let notice = params["notice"].as_str().unwrap_or("");
        if notice.len() > 16384 {
            return Err("THEME_LICENSE_NOTICE_SIZE_LIMIT".into());
        }
        if kind == "font"
            && (![
                "OFL-1.1",
                "Apache-2.0",
                "MIT",
                "BSD-2-Clause",
                "BSD-3-Clause",
            ]
            .contains(&license)
                || notice.trim().is_empty())
        {
            return Err("THEME_OPEN_FONT_LICENSE_REQUIRED".into());
        }
        let source = params["source"].as_str().unwrap_or("");
        if !source.is_empty() {
            let url = url::Url::parse(source).map_err(|_| "THEME_ASSET_SOURCE_INVALID")?;
            if !matches!(url.scheme(), "http" | "https")
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err("THEME_ASSET_SOURCE_INVALID".into());
            }
        }
        let id = format!("{:x}", Sha256::digest(&bytes));
        let target = self.root.join(&id);
        match fs::read(target.join("metadata.json")) {
            Ok(_) => {
                let existing = self.metadata(&id)?;
                if existing.license != license {
                    return Err("THEME_ASSET_LICENSE_CONFLICT".into());
                }
                return Ok(existing.descriptor());
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("INVALID_FILE_NAME")?;
        if name.chars().count() > 240 {
            return Err("INVALID_FILE_NAME".into());
        }
        let meta = Metadata {
            id: id.clone(),
            token: crate::rand_token::random_token_hex()?,
            name: name.into(),
            kind: kind.into(),
            mime: mime.into(),
            bytes: bytes.len(),
            license: license.into(),
            notice: notice.into(),
            source: source.into(),
            pixels,
        };
        fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        let stage = tempfile::Builder::new()
            .prefix(".import-")
            .tempdir_in(&self.root)
            .map_err(|e| e.to_string())?;
        fs::write(stage.path().join("data"), bytes).map_err(|e| e.to_string())?;
        fs::write(
            stage.path().join("metadata.json"),
            serde_json::to_vec(&meta).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(stage.path(), target).map_err(|e| e.to_string())?;
        Ok(meta.descriptor())
    }
    pub fn handle(&self, uri: &str) -> tauri::http::Response<Vec<u8>> {
        let result = (|| {
            let url = url::Url::parse(uri).map_err(|_| "invalid URL")?;
            let parts: Vec<_> = url.path().trim_start_matches('/').split('/').collect();
            if parts.len() != 2 || !identifier(parts[0]) || !identifier(parts[1]) {
                return Err("invalid asset reference");
            }
            let meta = self.metadata(parts[1]).map_err(|_| "asset not found")?;
            if meta.token != parts[0] {
                return Err("asset not found");
            }
            let bytes =
                fs::read(self.root.join(&meta.id).join("data")).map_err(|_| "asset not found")?;
            if bytes.len() != meta.bytes {
                return Err("asset damaged");
            }
            Ok((meta.mime, bytes))
        })();
        match result {
            Ok((mime, bytes)) => tauri::http::Response::builder()
                .status(200)
                .header("Content-Type", mime)
                .header("Access-Control-Allow-Origin", "*")
                .header("Cross-Origin-Resource-Policy", "cross-origin")
                .header("X-Content-Type-Options", "nosniff")
                .header("Cache-Control", "private, max-age=31536000, immutable")
                .body(bytes)
                .unwrap(),
            Err(message) => tauri::http::Response::builder()
                .status(404)
                .body(message.as_bytes().to_vec())
                .unwrap(),
        }
    }
}

fn inspect(bytes: &[u8]) -> Result<(&'static str, &'static str, u64), String> {
    let font = if bytes.starts_with(b"\0\x01\0\0") {
        Some("font/ttf")
    } else if bytes.starts_with(b"OTTO") {
        Some("font/otf")
    } else if bytes.starts_with(b"wOF2") {
        let raw = bytes.get(16..20).ok_or("THEME_FONT_INVALID")?;
        if u32::from_be_bytes(raw.try_into().unwrap()) as usize > MAX_BYTES {
            return Err("THEME_FONT_SIZE_LIMIT".into());
        }
        Some("font/woff2")
    } else {
        None
    };
    if let Some(mime) = font {
        if bytes.len() < 12 {
            return Err("THEME_FONT_INVALID".into());
        }
        return Ok(("font", mime, 0));
    }
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("THEME_IMAGE_SIZE_LIMIT".into());
    }
    let format = image::guess_format(bytes).map_err(|_| "THEME_ASSET_FORMAT_UNSUPPORTED")?;
    let mime = match format {
        image::ImageFormat::Png => {
            let png = image::codecs::png::PngDecoder::new(Cursor::new(bytes))
                .map_err(|e| e.to_string())?;
            if png.is_apng().map_err(|e| e.to_string())? {
                return Err("THEME_STATIC_IMAGE_REQUIRED".into());
            }
            "image/png"
        }
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::WebP => {
            let webp = image::codecs::webp::WebPDecoder::new(BufReader::new(Cursor::new(bytes)))
                .map_err(|e| e.to_string())?;
            if webp.has_animation() {
                return Err("THEME_STATIC_IMAGE_REQUIRED".into());
            }
            "image/webp"
        }
        _ => return Err("THEME_ASSET_FORMAT_UNSUPPORTED".into()),
    };
    let (width, height) = ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|e| e.to_string())?;
    let pixels = u64::from(width) * u64::from(height);
    if pixels == 0 || pixels > MAX_IMAGE_PIXELS {
        return Err("THEME_IMAGE_PIXEL_BUDGET".into());
    }
    Ok(("image", mime, pixels))
}

#[tauri::command(async)]
pub fn theme_assets_describe(
    window: WebviewWindow,
    caller: tauri::ipc::Request<'_>,
    assets: State<'_, ThemeAssets>,
    ids: Vec<String>,
) -> Result<Vec<Value>, String> {
    let main_url = window.url().map_err(|_| "SESSION_DENIED")?;
    if !crate::next_renderer::trusted_origin(
        window.label(),
        &main_url,
        caller.headers().get("Origin").and_then(|v| v.to_str().ok()),
    ) {
        return Err("SESSION_DENIED".into());
    }
    assets.describe(&ids)
}

pub fn from_app(app: &tauri::AppHandle) -> &ThemeAssets {
    app.state::<ThemeAssets>().inner()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_assets_require_the_callers_declared_file_and_contained_path() {
        let root = tempfile::tempdir().unwrap();
        let code = root.path().join("code");
        fs::create_dir(&code).unwrap();
        fs::write(code.join("font.ttf"), b"OTTO0123456789").unwrap();
        fs::write(root.path().join("outside.ttf"), b"OTTO0123456789").unwrap();
        let mut manifest: cruciblebox_next2_protocol::Manifest = serde_json::from_value(
            serde_json::from_str::<Value>(include_str!("../../contracts/next2/fixtures.json"))
                .unwrap()["native"]
                .clone(),
        )
        .unwrap();
        manifest.files =
            serde_json::from_value(json!([{"path":"font.ttf","bytes":14,"sha256":"0".repeat(64)}]))
                .unwrap();
        assert_eq!(
            bundled_path(&code, &manifest, "font.ttf").unwrap(),
            code.join("font.ttf").canonicalize().unwrap()
        );
        assert_eq!(
            bundled_path(&code, &manifest, "outside.ttf").unwrap_err(),
            "THEME_BUNDLED_ASSET_UNDECLARED"
        );
        manifest.files[0].path = "../outside.ttf".into();
        assert_eq!(
            bundled_path(&code, &manifest, "../outside.ttf").unwrap_err(),
            "THEME_BUNDLED_ASSET_OUTSIDE_PACKAGE"
        );
    }
    #[test]
    fn managed_assets_deduplicate_and_only_serve_their_capability_url() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("user-image.png");
        let image = image::DynamicImage::new_rgba8(2, 2);
        image
            .save_with_format(&source, image::ImageFormat::Png)
            .unwrap();
        let original = fs::read(&source).unwrap();
        let assets = ThemeAssets::new(root.path().join("user-themes"));
        let options = json!({"license":"User-created"});
        let first = assets.import(&source, &options).unwrap();
        assert_eq!(assets.import(&source, &options).unwrap(), first);
        assert_eq!(fs::read(&source).unwrap(), original);
        assert_eq!(
            assets.list(None, 16).unwrap()["assets"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let uri = first["url"].as_str().unwrap();
        assert_eq!(assets.handle(uri).body(), &original);
        assert_eq!(
            assets
                .handle(&uri.replacen(uri.split('/').nth(3).unwrap(), &"0".repeat(64), 1))
                .status(),
            404
        );
        assert_eq!(
            assets
                .handle("http://cruciblebox-theme.localhost/../../private")
                .status(),
            404
        );
        assert!(assets
            .import(&source, &json!({"license":"different"}))
            .unwrap_err()
            .contains("LICENSE_CONFLICT"));
    }
    #[test]
    fn executable_and_unlicensed_font_inputs_are_refused() {
        let root = tempfile::tempdir().unwrap();
        let assets = ThemeAssets::new(root.path().join("themes"));
        let file = root.path().join("input.bin");
        fs::write(&file, b"MZ executable is not a theme asset").unwrap();
        assert!(assets
            .import(&file, &json!({"license":"User-created"}))
            .is_err());
        fs::write(&file, b"OTTO0123456789").unwrap();
        assert!(assets
            .import(&file, &json!({"license":"proprietary"}))
            .unwrap_err()
            .contains("OPEN_FONT_LICENSE_REQUIRED"));
        assert!(assets
            .import(&file, &json!({"license":"OFL-1.1"}))
            .unwrap_err()
            .contains("OPEN_FONT_LICENSE_REQUIRED"));
        assert_eq!(assets.list(None, 16).unwrap()["assets"], json!([]));
    }
}
