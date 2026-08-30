use crate::dictionary::Dictionary;
use crate::model::{ModelMetadata, ModelPaths};
use crate::protocol::{
    error, validate_request, ModelInfo, OcrOptions, OcrRequest, OcrResponse, ProgressResponse,
    TextBlock, PROTOCOL_VERSION,
};
use image::{imageops::FilterType, DynamicImage, GenericImageView};
use ort::session::Session;
use ort::value::Tensor;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::sync::OnceLock;

const MAX_IMAGE_BYTES: u64 = 100 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 100_000_000;
const DETECTION_THRESHOLD: f32 = 0.30;
const MIN_COMPONENT_AREA: usize = 3;

struct OcrEngine {
    detection: Session,
    recognition: Session,
    dictionary: Dictionary,
    metadata: ModelMetadata,
    device: &'static str,
}

impl OcrEngine {
    fn load(paths: &ModelPaths, use_gpu: bool) -> Result<Self, String> {
        let mut metadata = paths.metadata()?;
        let dictionary = Dictionary::load(&paths.dictionary)?;
        metadata.dictionary_sha256 = dictionary.sha256.clone();
        let mut detection_builder =
            Session::builder().map_err(|e| format!("detection session creation failed: {e}"))?;
        let mut recognition_builder =
            Session::builder().map_err(|e| format!("recognition session creation failed: {e}"))?;
        if use_gpu {
            #[cfg(windows)]
            {
                let provider = ort::execution_providers::DirectML::default().build();
                detection_builder = detection_builder
                    .with_execution_providers([provider.clone()])
                    .map_err(|e| format!("DirectML detection provider unavailable: {e}"))?;
                recognition_builder = recognition_builder
                    .with_execution_providers([provider])
                    .map_err(|e| format!("DirectML recognition provider unavailable: {e}"))?;
            }
            #[cfg(not(windows))]
            {
                return Err("GPU OCR is only available on Windows DirectML builds".into());
            }
        }
        let detection = detection_builder
            .commit_from_file(&paths.detection)
            .map_err(|e| format!("detection model load failed: {e}"))?;
        let recognition = recognition_builder
            .commit_from_file(&paths.recognition)
            .map_err(|e| format!("recognition model load failed: {e}"))?;
        eprintln!(
            "[ocr-worker] detection input={} {:?} output={:?}",
            detection.inputs().first().map(|v| v.name()).unwrap_or(""),
            detection.inputs().first().map(|v| v.dtype()),
            detection.outputs().first().map(|v| v.dtype())
        );
        eprintln!(
            "[ocr-worker] recognition input={} {:?} output={:?}",
            recognition.inputs().first().map(|v| v.name()).unwrap_or(""),
            recognition.inputs().first().map(|v| v.dtype()),
            recognition.outputs().first().map(|v| v.dtype())
        );
        eprintln!(
            "[ocr-worker] loaded models det={} rec={} dictionary={}",
            metadata.detection_sha256, metadata.recognition_sha256, metadata.dictionary_sha256
        );
        Ok(Self {
            detection,
            recognition,
            dictionary,
            metadata,
            device: if use_gpu { "directml" } else { "cpu" },
        })
    }

    fn recognize(
        &mut self,
        request_id: &str,
        input_path: &str,
        on_progress: &mut dyn FnMut(u8, &'static str, &'static str),
    ) -> Result<OcrResponse, String> {
        let path = Path::new(input_path);
        let file_meta = std::fs::metadata(path).map_err(|e| format!("input unavailable: {e}"))?;
        if !file_meta.is_file() {
            return Err("input is not a regular file".into());
        }
        if file_meta.len() > MAX_IMAGE_BYTES {
            return Err(format!("input exceeds {MAX_IMAGE_BYTES} bytes"));
        }
        let image = image::open(path).map_err(|e| format!("image decode failed: {e}"))?;
        on_progress(10, "decode", "图像已解码");
        let (width, height) = image.dimensions();
        if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
            return Err("image dimensions exceed worker limits".into());
        }

        let boxes = detect_text(&mut self.detection, &image)?;
        let box_count = boxes.len();
        on_progress(35, "detect", "文本区域检测完成");
        let mut blocks = Vec::with_capacity(boxes.len());
        for (index, polygon) in boxes.into_iter().enumerate() {
            let crop = crop_text_region(&image, &polygon);
            let (text, confidence) =
                recognize_text(&mut self.recognition, &self.dictionary, &crop)?;
            let percent = if box_count == 0 {
                95
            } else {
                35u16
                    .saturating_add((((index + 1) * 60) / box_count) as u16)
                    .min(95) as u8
            };
            on_progress(percent, "recognize", "正在识别文本区域");
            if !text.trim().is_empty() {
                blocks.push(TextBlock {
                    text,
                    bbox: polygon_bbox(&polygon),
                    polygon,
                    confidence,
                });
            }
        }
        blocks.sort_by_key(|block| (block.bbox[1], block.bbox[0]));
        let text = blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        Ok(OcrResponse {
            protocol_version: PROTOCOL_VERSION,
            request_id: request_id.to_string(),
            r#type: "result",
            text,
            blocks,
            model: ModelInfo {
                detection_sha256: self.metadata.detection_sha256.clone(),
                recognition_sha256: self.metadata.recognition_sha256.clone(),
                dictionary_sha256: self.metadata.dictionary_sha256.clone(),
                device: self.device,
            },
        })
    }
}

struct WorkerState {
    loaded_key: Option<String>,
    engine: Option<OcrEngine>,
}

impl WorkerState {
    fn new() -> Self {
        Self {
            loaded_key: None,
            engine: None,
        }
    }

    fn engine_for(&mut self, options: Option<&OcrOptions>) -> Result<&mut OcrEngine, String> {
        let options = options.cloned().unwrap_or(OcrOptions {
            language: None,
            device: None,
            model_directory: None,
            dictionary_path: None,
        });
        let requested_device = options.device.as_deref().unwrap_or("auto");
        if !matches!(requested_device, "auto" | "cpu" | "gpu") {
            return Err(format!(
                "device '{requested_device}' is not supported (expected auto, cpu or gpu)"
            ));
        }
        if let Some(language) = options.language.as_deref() {
            if !matches!(language, "ch" | "zh" | "en" | "mix" | "auto") {
                return Err(format!("unsupported language: {language}"));
            }
        }
        let paths = ModelPaths::resolve(
            options.model_directory.as_deref(),
            options.dictionary_path.as_deref(),
        )?;
        let use_gpu = match requested_device {
            "gpu" => true,
            "auto" => gpu_available(),
            _ => false,
        };
        let key = format!(
            "{}|{}|{}|{}",
            paths.detection.display(),
            paths.recognition.display(),
            paths.dictionary.display(),
            if use_gpu { "gpu" } else { "cpu" }
        );
        if self.loaded_key.as_deref() != Some(key.as_str()) {
            let engine = match OcrEngine::load(&paths, use_gpu) {
                Ok(engine) => engine,
                Err(error) if use_gpu && requested_device == "auto" => {
                    eprintln!("[ocr-worker] DirectML unavailable, falling back to CPU: {error}");
                    OcrEngine::load(&paths, false)?
                }
                Err(error) => return Err(error),
            };
            self.engine = Some(engine);
            self.loaded_key = Some(key);
        }
        self.engine
            .as_mut()
            .ok_or_else(|| "OCR engine is not initialized".into())
    }
}

fn gpu_available() -> bool {
    static GPU_AVAILABLE: OnceLock<bool> = OnceLock::new();
    *GPU_AVAILABLE.get_or_init(|| gpu_available_uncached())
}

fn gpu_available_uncached() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};

        let mut command = Command::new("nvidia-smi");
        command
            .args(["-L"])
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        command
            .output()
            .map(|output| output.status.success() && !output.stdout.is_empty())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub fn run() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    let mut state = WorkerState::new();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                eprintln!("[ocr-worker] stdin read failed: {error}");
                break;
            }
        };
        let response = process_request(&line, &mut state, &mut stdout);
        write_json(&mut stdout, &response);
    }
}

fn write_json(stdout: &mut impl Write, value: &Value) {
    match serde_json::to_string(value) {
        Ok(json) => {
            let _ = writeln!(stdout, "{json}");
            let _ = stdout.flush();
        }
        Err(error) => eprintln!("[ocr-worker] response serialization failed: {error}"),
    }
}

fn process_request(line: &str, state: &mut WorkerState, stdout: &mut impl Write) -> Value {
    if line.len() > crate::protocol::MAX_REQUEST_LINE_BYTES {
        return error("unknown", "request-too-large", "request line exceeds limit");
    }
    let request: OcrRequest = match serde_json::from_str(line) {
        Ok(request) => request,
        Err(parse_error) => {
            return error(
                "unknown",
                "invalid-request",
                format!("invalid JSON: {parse_error}"),
            )
        }
    };
    if let Err(response) = validate_request(&request) {
        return response;
    }
    let request_id = request.request_id().to_string();
    let cold_start = state.engine.is_none();
    if cold_start {
        emit_progress(stdout, &request_id, "loading", 5, "正在加载 OCR 模型");
    } else {
        emit_progress(stdout, &request_id, "ready", 5, "OCR 模型已就绪");
    }
    let engine = match state.engine_for(request.options.as_ref()) {
        Ok(engine) => engine,
        Err(message) => return error(request_id, "model-unavailable", message),
    };
    let mut progress = |percent, stage, message| {
        emit_progress(stdout, &request_id, stage, percent, message);
    };
    match engine.recognize(&request_id, &request.input, &mut progress) {
        Ok(response) => serde_json::to_value(response).unwrap_or_else(|e| {
            error(
                request_id,
                "serialization-failed",
                format!("failed to encode result: {e}"),
            )
        }),
        Err(message) => error(request_id, "ocr-failed", message),
    }
}

fn emit_progress(
    stdout: &mut impl Write,
    request_id: &str,
    stage: &'static str,
    percent: u8,
    message: &'static str,
) {
    let response = ProgressResponse {
        protocol_version: PROTOCOL_VERSION,
        request_id,
        r#type: "progress",
        stage,
        percent,
        message,
    };
    if let Ok(value) = serde_json::to_value(response) {
        write_json(stdout, &value);
    }
}

type Polygon = [[i32; 2]; 4];

fn detect_text(session: &mut Session, image: &DynamicImage) -> Result<Vec<Polygon>, String> {
    let (original_width, original_height) = image.dimensions();
    let max_size = 960.0f32;
    let scale = (max_size / original_width.max(original_height) as f32).min(1.0);
    let new_width = ((((original_width as f32 * scale) as u32).max(32)) / 32) * 32;
    let new_height = ((((original_height as f32 * scale) as u32).max(32)) / 32) * 32;
    let width = new_width.max(32);
    let height = new_height.max(32);
    let resized = image.resize_exact(width, height, FilterType::Lanczos3);
    let rgb = resized.to_rgb8();
    let plane = (width * height) as usize;
    let mut input_data = vec![0.0f32; 3 * plane];
    let mean = [0.485f32, 0.456, 0.406];
    let std = [0.229f32, 0.224, 0.225];
    for y in 0..height {
        for x in 0..width {
            let pixel = rgb.get_pixel(x, y);
            let index = (y * width + x) as usize;
            input_data[index] = (pixel[0] as f32 / 255.0 - mean[0]) / std[0];
            input_data[plane + index] = (pixel[1] as f32 / 255.0 - mean[1]) / std[1];
            input_data[2 * plane + index] = (pixel[2] as f32 / 255.0 - mean[2]) / std[2];
        }
    }
    let input = Tensor::from_array(([1usize, 3, height as usize, width as usize], input_data))
        .map_err(|e| format!("detection tensor creation failed: {e}"))?;
    let input_name = session.inputs()[0].name().to_string();
    let mut inputs: HashMap<&str, ort::value::Value> = HashMap::new();
    inputs.insert(input_name.as_str(), input.into());
    let outputs = session
        .run(inputs)
        .map_err(|e| format!("detection failed: {e}"))?;
    let output = outputs
        .iter()
        .next()
        .ok_or("detection returned no output")?;
    let (shape, data) = output
        .1
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("detection output extraction failed: {e}"))?;
    if shape.len() < 2 {
        return Err("detection output has fewer than two dimensions".into());
    }
    let h = shape[shape.len() - 2] as usize;
    let w = shape[shape.len() - 1] as usize;
    if h == 0 || w == 0 || h.saturating_mul(w) > data.len() {
        return Err("detection output shape is invalid".into());
    }
    let sx = original_width as f32 / width as f32;
    let sy = original_height as f32 / height as f32;
    Ok(connected_components(data, h, w, DETECTION_THRESHOLD)
        .into_iter()
        .filter(|component| component.area >= MIN_COMPONENT_AREA)
        .map(|component| {
            let x1 = (component.min_x as f32 * sx) as i32;
            let y1 = (component.min_y as f32 * sy) as i32;
            let x2 = (((component.max_x + 1) as f32) * sx) as i32;
            let y2 = (((component.max_y + 1) as f32) * sy) as i32;
            [[x1, y1], [x2, y1], [x2, y2], [x1, y2]]
        })
        .collect())
}

#[derive(Debug)]
struct Component {
    min_x: usize,
    min_y: usize,
    max_x: usize,
    max_y: usize,
    area: usize,
}

fn connected_components(
    data: &[f32],
    height: usize,
    width: usize,
    threshold: f32,
) -> Vec<Component> {
    let mut seen = vec![false; height * width];
    let mut components = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            if seen[index] || data[index] <= threshold {
                continue;
            }
            seen[index] = true;
            let mut queue = vec![(x, y)];
            let mut component = Component {
                min_x: x,
                min_y: y,
                max_x: x,
                max_y: y,
                area: 0,
            };
            while let Some((cx, cy)) = queue.pop() {
                component.area += 1;
                component.min_x = component.min_x.min(cx);
                component.min_y = component.min_y.min(cy);
                component.max_x = component.max_x.max(cx);
                component.max_y = component.max_y.max(cy);
                let y_start = cy.saturating_sub(1);
                let y_end = (cy + 1).min(height - 1);
                let x_start = cx.saturating_sub(1);
                let x_end = (cx + 1).min(width - 1);
                for ny in y_start..=y_end {
                    for nx in x_start..=x_end {
                        let neighbor = ny * width + nx;
                        if !seen[neighbor] && data[neighbor] > threshold {
                            seen[neighbor] = true;
                            queue.push((nx, ny));
                        }
                    }
                }
            }
            components.push(component);
        }
    }
    components
}

fn crop_text_region(image: &DynamicImage, polygon: &Polygon) -> DynamicImage {
    let [raw_x1, raw_y1, raw_x2, raw_y2] = polygon_bbox(polygon);
    // DB maps are often only a few pixels high on small source images. Give
    // the recognizer a little context around each component; the recognizer
    // will normalize the crop to its 48-pixel text-line canvas.
    let horizontal_pad = ((raw_x2 - raw_x1).max(1) as f32 * 0.08).ceil() as i32;
    let vertical_pad = ((raw_y2 - raw_y1).max(1) as f32 * 0.60).ceil() as i32;
    let x1 = raw_x1 - horizontal_pad;
    let y1 = raw_y1 - vertical_pad;
    let x2 = raw_x2 + horizontal_pad;
    let y2 = raw_y2 + vertical_pad;
    let (width, height) = image.dimensions();
    let x = x1.max(0).min(width.saturating_sub(1) as i32) as u32;
    let y = y1.max(0).min(height.saturating_sub(1) as i32) as u32;
    let right = x2.max(x1 + 1).min(width as i32) as u32;
    let bottom = y2.max(y1 + 1).min(height as i32) as u32;
    image.crop_imm(
        x,
        y,
        right.saturating_sub(x).max(1),
        bottom.saturating_sub(y).max(1),
    )
}

fn polygon_bbox(polygon: &Polygon) -> [i32; 4] {
    let mut min_x = polygon[0][0];
    let mut min_y = polygon[0][1];
    let mut max_x = polygon[0][0];
    let mut max_y = polygon[0][1];
    for point in polygon.iter().skip(1) {
        min_x = min_x.min(point[0]);
        min_y = min_y.min(point[1]);
        max_x = max_x.max(point[0]);
        max_y = max_y.max(point[1]);
    }
    [min_x, min_y, max_x, max_y]
}

fn recognize_text(
    session: &mut Session,
    dictionary: &Dictionary,
    image: &DynamicImage,
) -> Result<(String, f32), String> {
    let target_height = 48u32;
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return Ok((String::new(), 0.0));
    }
    // PP-OCRv4 rec models are trained with a 320-pixel canvas. Preserve the
    // aspect ratio and pad the normalized tensor instead of feeding a shorter
    // dynamic width, which changes the CTC time scale at inference time.
    const MODEL_WIDTH: u32 = 320;
    let resized_width = ((width as f32 * target_height as f32 / height as f32).ceil() as u32)
        .clamp(32, MODEL_WIDTH);
    let resized = image.resize_exact(resized_width, target_height, FilterType::Lanczos3);
    let rgb = resized.to_rgb8();
    let plane = (MODEL_WIDTH * target_height) as usize;
    let target_width = MODEL_WIDTH;
    let mut input_data = vec![0.0f32; 3 * plane];
    for y in 0..target_height {
        for x in 0..resized_width {
            let pixel = rgb.get_pixel(x, y);
            let index = (y * target_width + x) as usize;
            input_data[index] = pixel[0] as f32 / 127.5 - 1.0;
            input_data[plane + index] = pixel[1] as f32 / 127.5 - 1.0;
            input_data[2 * plane + index] = pixel[2] as f32 / 127.5 - 1.0;
        }
    }
    let input = Tensor::from_array((
        [1usize, 3, target_height as usize, target_width as usize],
        input_data,
    ))
    .map_err(|e| format!("recognition tensor creation failed: {e}"))?;
    let input_name = session.inputs()[0].name().to_string();
    let mut inputs: HashMap<&str, ort::value::Value> = HashMap::new();
    inputs.insert(input_name.as_str(), input.into());
    let outputs = session
        .run(inputs)
        .map_err(|e| format!("recognition failed: {e}"))?;
    let output = outputs
        .iter()
        .next()
        .ok_or("recognition returned no output")?;
    let (shape, data) = output
        .1
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("recognition output extraction failed: {e}"))?;
    let (sequence_length, class_count) = match shape.len() {
        3 => (shape[1] as usize, shape[2] as usize),
        2 => (shape[0] as usize, shape[1] as usize),
        _ => return Err("recognition output must be [1,T,C] or [T,C]".into()),
    };
    if class_count != dictionary.len() + 1 && class_count != dictionary.len() + 2
        || sequence_length.saturating_mul(class_count) > data.len()
    {
        return Err(format!(
            "recognition output shape is incompatible with dictionary size {} (classes={class_count})",
            dictionary.len()
        ));
    }
    let mut classes = Vec::with_capacity(sequence_length);
    let mut probabilities = Vec::with_capacity(sequence_length);
    for time in 0..sequence_length {
        let row = &data[time * class_count..(time + 1) * class_count];
        let row_sum: f32 = row.iter().sum();
        let already_probabilities =
            row.iter().all(|value| (0.0..=1.0).contains(value)) && (0.99..=1.01).contains(&row_sum);
        let max_logit = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let exp_sum: f32 = row.iter().map(|value| (*value - max_logit).exp()).sum();
        let mut best_class = 0usize;
        let mut best_probability = 0.0f32;
        for (class, value) in row.iter().enumerate() {
            let probability = if already_probabilities {
                *value
            } else {
                (*value - max_logit).exp() / exp_sum.max(f32::EPSILON)
            };
            if probability > best_probability {
                best_probability = probability;
                best_class = class;
            }
        }
        classes.push(best_class);
        probabilities.push(best_probability);
    }
    Ok(dictionary.decode(&classes, &probabilities))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connected_components_returns_separate_regions() {
        let data = [0.0, 0.9, 0.9, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.8, 0.8];
        let components = connected_components(&data, 3, 4, 0.3);
        assert_eq!(components.len(), 2);
        assert_eq!(components[0].area, 2);
        assert_eq!(components[1].area, 2);
    }

    #[test]
    fn polygon_bbox_is_stable() {
        let polygon = [[10, 30], [50, 20], [60, 80], [5, 90]];
        assert_eq!(polygon_bbox(&polygon), [5, 20, 60, 90]);
    }
}
