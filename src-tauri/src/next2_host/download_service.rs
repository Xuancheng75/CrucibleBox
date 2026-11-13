//! Explicit resumable downloads. Partial files are private checkpoints, never task results.
use super::{
    managed_child,
    network_service::{agent, network_error},
    Execution, Manager,
};
use cruciblebox_next2_protocol::{self as protocol, Request};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Default)]
pub(super) struct Downloads(Mutex<HashSet<PathBuf>>);
struct Active<'a> {
    downloads: &'a Downloads,
    directory: PathBuf,
}
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.downloads
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.directory);
    }
}
fn checkpoint(directory: &Path, id: &str, total: u64) -> Result<Value, String> {
    let payload = crate::next2_files::open_readonly(&directory.join("payload.part"))?;
    let received = payload.metadata().map_err(|error| error.to_string())?.len();
    if received > total {
        return Err("DOWNLOAD_CHECKPOINT_INVALID".into());
    }
    Ok(json!({"downloadId":id,"received":received,"total":total}))
}
impl Manager {
    pub(super) fn download_operation(
        &self,
        owner: &str,
        row: &crate::db::PluginRow,
        request: &Request,
        execution: Execution<'_>,
    ) -> Result<Value, String> {
        let params = &request.params;
        let root = managed_child(&Self::writable_data(row)?, "downloads")?
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let recipient = execution.caller.unwrap_or(owner);
        if request.method == "network.download.prepare" {
            agent(params["url"].as_str().unwrap())?;
            super::file_operations::name(params["name"].as_str().unwrap())?;
            let _preparing = self
                .downloads
                .0
                .lock()
                .map_err(|_| "DOWNLOAD_STATE_UNAVAILABLE")?;
            if fs::read_dir(&root)
                .map_err(|error| error.to_string())?
                .take(16)
                .count()
                >= 16
            {
                return Err("DOWNLOAD_CHECKPOINT_LIMIT".into());
            }
            let id = crate::rand_token::random_token_hex()?;
            let directory = root.join(&id);
            fs::create_dir(&directory).map_err(|error| error.to_string())?;
            let prepare = || -> Result<Value, String> {
                fs::write(
                    directory.join("checkpoint.json"),
                    json!({"owner":owner,"recipient":recipient,"params":params}).to_string(),
                )
                .map_err(|error| error.to_string())?;
                File::create(directory.join("payload.part")).map_err(|error| error.to_string())?;
                checkpoint(&directory, &id, params["bytes"].as_u64().unwrap())
            };
            return match prepare() {
                Ok(value) => Ok(value),
                Err(error) => {
                    fs::remove_dir_all(&directory)
                        .map_err(|cleanup| format!("{error}; checkpoint cleanup: {cleanup}"))?;
                    Err(error)
                }
            };
        }
        let id = params["downloadId"].as_str().unwrap();
        let directory = root.join(id);
        crate::transaction::assert_plain_directory(&directory, "Download checkpoint")?;
        let _directory_handle = crate::next2_files::open_authorized(&directory)?;
        let record_file = crate::next2_files::open_readonly(&directory.join("checkpoint.json"))?;
        let record: Value = serde_json::from_reader(record_file.take(16385))
            .map_err(|_| "DOWNLOAD_CHECKPOINT_INVALID")?;
        if record["owner"] != owner || record["recipient"] != recipient {
            return Err("DOWNLOAD_AUTHORITY_DENIED".into());
        }
        // Persisted local metadata re-enters through the public contract after a restart.
        let mut check = request.clone();
        check.method = "network.download.prepare".into();
        check.params = record["params"].clone();
        check.target = serde_json::from_value(json!({"kind":"platform"})).unwrap();
        protocol::validate_request(
            &serde_json::to_string(&check).map_err(|error| error.to_string())?,
        )?;
        let params = &record["params"];
        let total = params["bytes"].as_u64().unwrap();
        if request.method == "network.download.state" {
            return checkpoint(&directory, id, total);
        }
        let mut active = self
            .downloads
            .0
            .lock()
            .map_err(|_| "DOWNLOAD_STATE_UNAVAILABLE")?;
        if !active.insert(directory.clone()) {
            return Err("DOWNLOAD_BUSY".into());
        }
        drop(active);
        let _active = Active {
            downloads: &self.downloads,
            directory: directory.clone(),
        };
        match request.method.as_str() {
            "network.download.discard" => {
                drop(_directory_handle);
                fs::remove_dir_all(&directory).map_err(|error| error.to_string())?;
                Ok(json!(true))
            }
            "network.download.resume" => {
                let (context, provider) = execution.task.ok_or("TASK_EXECUTION_REQUIRED")?;
                if provider != owner {
                    return Err("TASK_AUTHORITY_DENIED".into());
                }
                resume(&directory, params, execution, context)?;
                let result = self.write_output(
                    owner,
                    params["name"].as_str().unwrap(),
                    execution,
                    |stage, _| {
                        let mut input =
                            crate::next2_files::open_readonly(&directory.join("payload.part"))?;
                        let mut output = File::create(stage).map_err(|error| error.to_string())?;
                        let mut buffer = [0; 65536];
                        loop {
                            execution.check()?;
                            let count =
                                input.read(&mut buffer).map_err(|error| error.to_string())?;
                            if count == 0 {
                                break;
                            }
                            output
                                .write_all(&buffer[..count])
                                .map_err(|error| error.to_string())?;
                        }
                        output.sync_all().map_err(|error| error.to_string())
                    },
                )?;
                // Keep the verified checkpoint until explicit discard: it can be queried or
                // published again after a failed result acknowledgement without re-downloading.
                Ok(result)
            }
            _ => Err("PLATFORM_METHOD_NOT_CONNECTED".into()),
        }
    }
}
fn resume(
    directory: &Path,
    params: &Value,
    execution: Execution<'_>,
    context: &crate::task_runtime::Context,
) -> Result<(), String> {
    let path = directory.join("payload.part");
    let mut received = crate::next2_files::open_readonly(&path)?
        .metadata()
        .map_err(|error| error.to_string())?
        .len();
    let total = params["bytes"].as_u64().unwrap();
    if received > total {
        return Err("DOWNLOAD_CHECKPOINT_INVALID".into());
    }
    execution.check()?;
    if received < total {
        let url = params["url"].as_str().unwrap();
        let mut request = agent(url)?.get(url).set("Accept-Encoding", "identity");
        if received != 0 {
            request = request.set("Range", &format!("bytes={received}-"));
            match fs::read_to_string(directory.join("etag")) {
                Ok(etag) => {
                    request = request.set("If-Range", &etag);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        let response = request.call().map_err(network_error)?;
        execution.check()?;
        if response
            .header("Content-Encoding")
            .is_some_and(|value| value != "identity")
        {
            return Err("DOWNLOAD_ENCODING_UNSUPPORTED".into());
        }
        if received == 0 {
            if response.status() != 200 {
                return Err(format!("HTTP_STATUS: {}", response.status()));
            }
            if let Some(etag) = response.header("ETag").filter(|value| {
                value.starts_with('"') && value.ends_with('"') && value.len() <= 1024
            }) {
                fs::write(directory.join("etag"), etag).map_err(|error| error.to_string())?;
            }
        } else if response.status() != 206
            || response.header("Content-Range")
                != Some(format!("bytes {received}-{}/{total}", total - 1).as_str())
        {
            return Err(
                "DOWNLOAD_RANGE_MISMATCH: checkpoint retained; discard explicitly to restart"
                    .into(),
            );
        }
        if let Some(length) = response.header("Content-Length") {
            if length.parse::<u64>().ok() != Some(total - received) {
                return Err("DOWNLOAD_SIZE_MISMATCH".into());
            }
        }
        let mut input = response.into_reader();
        let mut output = File::options()
            .append(true)
            .open(&path)
            .map_err(|error| error.to_string())?;
        let mut buffer = [0; 65536];
        loop {
            execution.check()?;
            let count = input
                .read(&mut buffer)
                .map_err(|_| "NETWORK_READ_FAILED: checkpoint retained")?;
            if count == 0 {
                break;
            }
            if received + count as u64 > total {
                return Err("DOWNLOAD_SIZE_MISMATCH".into());
            }
            output
                .write_all(&buffer[..count])
                .map_err(|error| error.to_string())?;
            received += count as u64;
            context.update_progress(
                "download",
                (received * 95 / total.max(1)) as u32,
                "续传文件",
                Some(json!({"received":received,"total":total})),
            );
        }
        output.sync_all().map_err(|error| error.to_string())?;
        if received != total {
            return Err("DOWNLOAD_INCOMPLETE: checkpoint retained".into());
        }
    }
    let mut input = crate::next2_files::open_readonly(&path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        execution.check()?;
        let count = input.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    if format!("{:x}", hash.finalize()) != params["sha256"].as_str().unwrap() {
        return Err("DOWNLOAD_INTEGRITY_MISMATCH".into());
    }
    Ok(())
}
