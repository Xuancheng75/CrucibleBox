use super::{Execution, Manager};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

pub(super) fn agent(url: &str) -> Result<ureq::Agent, String> {
    let target = url::Url::parse(url).map_err(|_| "NETWORK_URL_INVALID")?;
    if !matches!(target.scheme(), "http" | "https")
        || !target.username().is_empty()
        || target.password().is_some()
    {
        return Err("NETWORK_URL_DENIED".into());
    }
    crate::network_policy::current()
        .agent_for_url(url, 10, 5)
        .map(|(agent, _)| agent)
        .map_err(|_| "NETWORK_ROUTE_UNAVAILABLE".into())
}

pub(super) fn network_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(status, _) => format!("HTTP_STATUS: {status}"),
        ureq::Error::Transport(error) => format!("NETWORK_TRANSPORT: {:?}", error.kind()),
    }
}

fn request(params: &Value) -> Result<ureq::Request, String> {
    let url = params["url"].as_str().unwrap();
    let mut request = agent(url)?.request(params["method"].as_str().unwrap(), url);
    if let Some(headers) = params["headers"].as_object() {
        if headers.len() > 32 {
            return Err("HTTP_HEADER_LIMIT".into());
        }
        for (name, value) in headers {
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "host"
                    | "content-length"
                    | "transfer-encoding"
                    | "proxy-authorization"
                    | "connection"
                    | "upgrade"
            ) {
                return Err("HTTP_HEADER_RESERVED".into());
            }
            request = request.set(name, value.as_str().unwrap());
        }
    }
    Ok(request)
}

fn binary_response(
    response: Result<ureq::Response, ureq::Error>,
    execution: Execution<'_>,
) -> Result<Value, String> {
    let response = match response {
        Ok(response) | Err(ureq::Error::Status(_, response)) => response,
        Err(error) => {
            execution.check()?;
            return Err(network_error(error));
        }
    };
    let status = response.status();
    let mut headers = serde_json::Map::new();
    let mut header_bytes = 0;
    for name in response.headers_names() {
        if let Some(value) = response.header(&name) {
            header_bytes += name.len() + value.len();
            if headers.len() >= 32 || header_bytes > 4096 {
                return Err("HTTP_RESPONSE_HEADERS_TOO_LARGE".into());
            }
            headers.insert(name, value.into());
        }
    }
    let mut reader = response.into_reader();
    let mut body = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        execution.check()?;
        let count = reader
            .read(&mut buffer)
            .map_err(|_| "NETWORK_READ_FAILED")?;
        if count == 0 {
            break;
        }
        if body.len() + count > 16384 {
            return Err("HTTP_RESPONSE_TOO_LARGE".into());
        }
        body.extend_from_slice(&buffer[..count]);
    }
    execution.check()?;
    Ok(json!({"status":status,"headers":headers,"body":STANDARD.encode(body)}))
}

pub(super) fn binary(params: &Value, execution: Execution<'_>) -> Result<Value, String> {
    execution.check()?;
    let body = STANDARD
        .decode(params["body"].as_str().unwrap_or_default())
        .map_err(|_| "INVALID_HTTP_BODY")?;
    if body.len() > 16384 {
        return Err("HTTP_REQUEST_TOO_LARGE".into());
    }
    binary_response(request(params)?.send_bytes(&body), execution)
}

struct Upload<'a> {
    file: File,
    execution: Execution<'a>,
    sent: u64,
    total: u64,
}
impl Read for Upload<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        self.execution.check().map_err(std::io::Error::other)?;
        let length = output.len().min(65536);
        let count = self.file.read(&mut output[..length])?;
        self.sent += count as u64;
        if let Some((context, _)) = self.execution.task {
            context.update_progress(
                "upload",
                (self.sent * 95 / self.total.max(1)) as u32,
                "上传文件",
                Some(json!({"sent":self.sent,"total":self.total})),
            );
        }
        Ok(count)
    }
}
impl Manager {
    pub(super) fn upload_file(
        &self,
        owner: &str,
        params: &Value,
        execution: Execution<'_>,
    ) -> Result<Value, String> {
        let (_, provider) = execution.task.ok_or("TASK_EXECUTION_REQUIRED")?;
        if provider != owner {
            return Err("TASK_AUTHORITY_DENIED".into());
        }
        let path = self.resolve_file(owner, execution.caller, params["ref"].as_str().unwrap())?;
        let file = crate::next2_files::open_readonly(&path)?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        if !metadata.is_file() || metadata.len() > 1073741824 {
            return Err("UPLOAD_SIZE_LIMIT".into());
        }
        let mut input = Upload {
            file,
            execution,
            sent: 0,
            total: metadata.len(),
        };
        let response = request(params)?
            .set("Content-Length", &metadata.len().to_string())
            .send(&mut input);
        execution.check()?;
        let result = binary_response(response, execution)?;
        if input.sent != input.total {
            return Err("UPLOAD_SOURCE_CHANGED".into());
        }
        Ok(result)
    }
}

pub(super) fn fetch(params: &Value) -> Result<Value, String> {
    let url = params["url"].as_str().unwrap();
    let method = params["method"].as_str().unwrap();
    let mut request = agent(url)?.request(method, url);
    if let Some(headers) = params["headers"].as_object() {
        for (name, value) in headers {
            request = request.set(
                name,
                value.as_str().ok_or("HTTP_HEADER_VALUE_MUST_BE_STRING")?,
            );
        }
    }
    let response = if let Some(body) = params["body"].as_str() {
        request.send_string(body)
    } else {
        request.call()
    };
    let response = match response {
        Ok(response) | Err(ureq::Error::Status(_, response)) => response,
        Err(error) => return Err(network_error(error)),
    };
    let status = response.status();
    let mut headers = serde_json::Map::new();
    for name in response.headers_names() {
        if let Some(value) = response.header(&name) {
            headers.insert(name, value.into());
        }
    }
    let mut body = Vec::new();
    response
        .into_reader()
        .take(32769)
        .read_to_end(&mut body)
        .map_err(|_| "NETWORK_READ_FAILED")?;
    if body.len() > 32768 {
        return Err("HTTP_RESPONSE_TOO_LARGE: use network.download for a verified file".into());
    }
    let body = String::from_utf8(body).map_err(|_| "HTTP_RESPONSE_REQUIRES_UTF8")?;
    Ok(json!({"status":status,"headers":headers,"body":body}))
}

pub(super) fn download(
    stage: &Path,
    params: &Value,
    execution: Execution<'_>,
    context: &crate::task_runtime::Context,
) -> Result<(), String> {
    let url = params["url"].as_str().unwrap();
    let expected = params["bytes"].as_u64().unwrap();
    let digest = params["sha256"].as_str().unwrap();
    execution.check()?;
    let response = agent(url)?.get(url).call().map_err(network_error)?;
    if response.status() != 200 {
        return Err(format!("HTTP_STATUS: {}", response.status()));
    }
    let mut input = response.into_reader().take(expected + 1);
    let mut output = File::create(stage).map_err(|error| error.to_string())?;
    let mut buffer = [0; 65536];
    let mut hash = Sha256::new();
    let mut received = 0;
    let mut percent = 0;
    context.update_progress("download", 0, "下载文件", None);
    loop {
        execution.check()?;
        let count = input.read(&mut buffer).map_err(|_| "NETWORK_READ_FAILED")?;
        if count == 0 {
            break;
        }
        received += count as u64;
        if received > expected {
            return Err("DOWNLOAD_SIZE_MISMATCH".into());
        }
        output
            .write_all(&buffer[..count])
            .map_err(|error| error.to_string())?;
        hash.update(&buffer[..count]);
        let next = (received * 95 / expected.max(1)) as u32;
        if next != percent {
            percent = next;
            context.update_progress(
                "download",
                percent,
                "下载文件",
                Some(json!({"received":received,"total":expected})),
            );
        }
    }
    if received != expected {
        return Err("DOWNLOAD_SIZE_MISMATCH".into());
    }
    if format!("{:x}", hash.finalize()) != digest {
        return Err("DOWNLOAD_INTEGRITY_MISMATCH".into());
    }
    output.sync_all().map_err(|error| error.to_string())
}
