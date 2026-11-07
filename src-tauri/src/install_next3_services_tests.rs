use super::{lock, tests::*, InstallManager, InstallSource};
use base64::{engine::general_purpose::STANDARD, Engine};
use cruciblebox_next2_protocol::WIRE_VERSION;
use cruciblebox_next2_runtime::package::Approval;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

const PLUGIN: &str = "acme-independent";

fn directory(path: &Path) -> InstallSource {
    InstallSource {
        source_type: "directory".into(),
        path: path.to_string_lossy().into(),
    }
}

struct Fixture {
    manager: Arc<InstallManager>,
    session: crate::plugin_session::RendererSession,
    root: PathBuf,
    sequence: Cell<u64>,
}

impl Fixture {
    fn install(tag: &str, permissions: &[&str], customize: impl FnOnce(&mut Value)) -> Self {
        let (manager, plugins, root) = setup(tag);
        let source = root.join("source");
        write_next2(
            &source,
            "1.0.0",
            Some(
                r#"let id = 0;
module.exports.activate = context => Object.fromEntries(
  ['files.writeResult', 'network.download', 'process.run', 'logging.write'].map(method => [
    method, async params => {
      const result = await context.exchange({
        wireVersion: context.wireVersion, requestId: 'test-' + (++id), session: context.session,
        target: { kind: 'platform' }, method, params
      });
      if (!result.ok) throw Object.assign(Error(result.error.message), {code:result.error.code});
      return result.result;
    }
  ])
);"#,
            ),
        );
        let path = source.join("plugin.json");
        let mut manifest: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        manifest["permissions"]
            .as_array_mut()
            .unwrap()
            .extend(permissions.iter().map(|value| json!(value)));
        customize(&mut manifest);
        fs::write(path, manifest.to_string()).unwrap();
        manager.remember_trusted_path(source.clone());
        let preview = manager.preview(directory(&source)).unwrap();
        manager
            .commit_approved(
                preview["installToken"].as_str().unwrap().into(),
                Some(next2_approval(&preview)),
            )
            .unwrap();
        manager.next_backend.next2.enable(PLUGIN).unwrap();
        let session = next2_session(&manager, &plugins);
        Self {
            manager,
            session,
            root,
            sequence: Cell::new(0),
        }
    }

    fn call(&self, method: &str, params: Value) -> Value {
        let next = self.sequence.get() + 1;
        self.sequence.set(next);
        let raw = json!({"wireVersion":WIRE_VERSION,"requestId":format!("renderer-{next}"),
            "session":self.session.token,"target":{"kind":"platform"},"method":method,"params":params});
        self.manager
            .next_backend
            .next2
            .renderer(&raw.to_string(), &self.session)
            .unwrap()
    }

    fn start(&self, method: &str, params: Value) -> String {
        let response = self.call(
            "tasks.start",
            json!({"target":{"kind":"backend"},"method":method,"params":params}),
        );
        response["result"]["taskId"]
            .as_str()
            .unwrap_or_else(|| panic!("{response}"))
            .into()
    }

    fn wait(&self, task: &str, ready: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            let response = self.call("tasks.get", json!({"taskId":task}));
            if ready(&response["result"]) {
                return response["result"].clone();
            }
            assert!(Instant::now() < deadline, "{response}");
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn finish(&self, task: &str) -> Value {
        self.wait(task, |value| {
            matches!(
                value["status"].as_str(),
                Some("succeeded" | "failed" | "cancelled")
            )
        })
    }
}

fn http_fixture(
    body: Vec<u8>,
    resume: Option<mpsc::Receiver<()>>,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/result", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(12);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "download did not reach the loopback server"
                    );
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("{error}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = [0; 4096];
        assert!(stream.read(&mut request).unwrap() > 0);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        if let Some(resume) = resume {
            stream.write_all(&body[..65536]).unwrap();
            resume.recv_timeout(Duration::from_secs(12)).unwrap();
            let _ = stream.write_all(&body[65536..]);
        } else {
            stream.write_all(&body).unwrap();
        }
    });
    (url, server)
}

#[test]
fn public_files_downloads_and_logs_use_the_worker_and_task_boundary() {
    let fixture = Fixture::install(
        "next3-file-download",
        &[
            "files:read",
            "files:publish",
            "network:download",
            "logging:write",
        ],
        |_| {},
    );
    let input = fixture.root.join("input");
    fs::create_dir(&input).unwrap();
    fs::write(input.join("item.txt"), b"hello").unwrap();
    std::os::windows::fs::symlink_file(input.join("item.txt"), input.join("link.txt")).unwrap();
    let host = &fixture.manager.next_backend.next2;
    let grant = host.grant_fixture_file(PLUGIN, &input);
    assert_eq!(
        fixture.call("files.info", json!({"ref":grant.r#ref}))["result"]["kind"],
        "directory"
    );
    assert_eq!(
        fixture.call("files.list", json!({"ref":grant.r#ref}))["result"],
        json!([{"name":"item.txt","kind":"file","bytes":5}])
    );

    let write = fixture.start(
        "files.writeResult",
        json!({"name":"note.txt","data":"aGVsbG8="}),
    );
    let written = fixture.finish(&write);
    assert_eq!(written["status"], "succeeded", "{written}");
    let read = fixture.call(
        "files.read",
        json!({"ref":written["result"]["ref"],"offset":0,"length":24}),
    );
    assert_eq!(read["result"]["data"], "aGVsbG8=");
    let log = fixture.start("logging.write", json!({"level":"warn","message":"fixture","code":"TEST_WARNING","context":{"stage":"verify"}}));
    assert_eq!(fixture.finish(&log)["status"], "succeeded");
    let logs = lock(&fixture.manager.db)
        .plugin_logs(Some(PLUGIN), Some("warn"), 10)
        .unwrap();
    let entry: Value = serde_json::from_str(&logs[0].message).unwrap();
    assert_eq!(entry["taskId"], log);
    assert_eq!(entry["runtime"], "quickjs");
    assert_eq!(entry["code"], "TEST_WARNING");
    assert_eq!(entry["context"]["stage"], "verify");

    let body = vec![41; 192 * 1024];
    let digest = format!("{:x}", Sha256::digest(&body));
    let bytes = body.len();
    let params = |url: String, sha256: &str| json!({"url":url,"bytes":bytes,"sha256":sha256,"name":"output.bin"});
    let (url, server) = http_fixture(body.clone(), None);
    let task = fixture.start("network.download", params(url, &digest));
    let downloaded = fixture.finish(&task);
    server.join().unwrap();
    assert_eq!(downloaded["status"], "succeeded", "{downloaded}");
    assert_eq!(downloaded["result"]["bytes"], body.len());
    let path = fixture
        .root
        .join("plugin-data")
        .join(PLUGIN)
        .join("results")
        .join(task)
        .join("output.bin");
    assert_eq!(fs::read(path).unwrap(), body);

    let (url, server) = http_fixture(body.clone(), None);
    let wrong = fixture.start("network.download", params(url, &"0".repeat(64)));
    let failed = fixture.finish(&wrong);
    server.join().unwrap();
    assert_eq!(failed["status"], "failed", "{failed}");
    assert!(failed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("DOWNLOAD_INTEGRITY_MISMATCH"));

    let (resume, wait) = mpsc::channel();
    let (url, server) = http_fixture(body, Some(wait));
    let task = fixture.start("network.download", params(url, &digest));
    fixture.wait(&task, |task| {
        task["progress"]["percent"]
            .as_u64()
            .is_some_and(|percent| percent > 0)
    });
    assert_eq!(
        fixture.call("tasks.cancel", json!({"taskId":task}))["result"]["accepted"],
        true
    );
    resume.send(()).unwrap();
    server.join().unwrap();
    assert_eq!(fixture.finish(&task)["status"], "cancelled");
    for task in [wrong, task] {
        let directory = fixture
            .root
            .join("plugin-data")
            .join(PLUGIN)
            .join("results")
            .join(task);
        assert_eq!(fs::read_dir(directory).unwrap().count(), 0);
    }
    fixture.manager.next_backend.shutdown().unwrap();
}

#[test]
#[ignore = "requires a prepared, isolated CPython runtime resource"]
fn declared_python_runtime_requires_consent_and_runs_bounded_processes() {
    let asset = PathBuf::from(
        std::env::var_os("CRUCIBLEBOX_NEXT3_PYTHON_RESOURCE").expect("Python fixture required"),
    );
    let fixture = Fixture::install("next3-python-tool", &["process:run"], |manifest| {
        manifest["resources"] = json!([{"id":"python-runtime","version":{"min":"3.14.0","maxExclusive":"3.15.0"},"optional":true}]);
        manifest["tools"] =
            json!([{"id":"python","resourceId":"python-runtime","entry":"python.exe"}]);
    });
    assert_eq!(
        fixture.call("runtime.list", json!({}))["result"][0]["state"],
        "missing"
    );
    fixture.manager.remember_trusted_path(asset.clone());
    let preview = || {
        fixture
            .manager
            .resources
            .preview(&fixture.manager, PLUGIN, directory(&asset))
            .unwrap()
    };
    let inspect = preview();
    let mut approval = Approval {
        manifest_sha256: serde_json::from_value(inspect["manifestSha256"].clone()).unwrap(),
        accept_native_code: false,
        accept_unsigned_local: true,
        accept_permissions: true,
    };
    assert_eq!(
        fixture
            .manager
            .resources
            .commit(inspect["token"].as_str().unwrap(), approval.clone())
            .unwrap_err(),
        "NATIVE_TRUST_REQUIRED"
    );
    approval.accept_native_code = true;
    fixture
        .manager
        .resources
        .commit(preview()["token"].as_str().unwrap(), approval)
        .unwrap();
    assert_eq!(
        fixture.call("runtime.list", json!({}))["result"][0]["state"],
        "ready"
    );
    assert_eq!(
        fixture.call("process.run", json!({"toolId":"python","args":[]}))["ok"],
        false,
        "renderer must not execute a process directly"
    );

    let task = fixture.start("process.run", json!({"toolId":"python","args":["-I","-S","-B","-c","import sys; print(sys.version_info.major); print(sys.stdin.read()); sys.stderr.write('diagnostic'); sys.exit(7)"],"stdin":"aGVsbG8="}));
    let result = fixture.finish(&task);
    assert_eq!(result["status"], "succeeded", "{result}");
    assert_eq!(result["result"]["exitCode"], 7);
    let stdout = STANDARD
        .decode(result["result"]["stdout"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        String::from_utf8(stdout).unwrap().replace("\r\n", "\n"),
        "3\nhello\n"
    );
    assert_eq!(
        STANDARD
            .decode(result["result"]["stderr"].as_str().unwrap())
            .unwrap(),
        b"diagnostic"
    );

    let task = fixture.start("process.run", json!({"toolId":"python","args":["-I","-S","-B","-c","import sys; sys.stdout.write('x'*8193)"]}));
    let result = fixture.finish(&task);
    assert_eq!(result["status"], "failed", "{result}");
    assert!(result["error"]["message"]
        .as_str()
        .unwrap()
        .contains("PROCESS_OUTPUT_LIMIT"));

    let task = fixture.start("process.run", json!({"toolId":"python","args":["-I","-S","-B","-c","import time; time.sleep(30)"],"timeoutMs":300}));
    let result = fixture.finish(&task);
    assert_eq!(result["status"], "failed", "{result}");
    assert!(result["error"]["message"]
        .as_str()
        .unwrap()
        .contains("PROCESS_TIMEOUT"));
    let task = fixture.start("process.run", json!({"toolId":"python","args":["-I","-S","-B","-c","import time; open('started.txt','w').close(); time.sleep(30)"]}));
    let data = fixture.root.join("plugin-data").join(PLUGIN);
    let deadline = Instant::now() + Duration::from_secs(10);
    let workspace = loop {
        if let Some(path) = fs::read_dir(&data)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.join("started.txt").is_file())
        {
            break path;
        }
        assert!(Instant::now() < deadline, "Python process did not start");
        thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(
        fixture.call("tasks.cancel", json!({"taskId":task}))["result"]["accepted"],
        true
    );
    assert_eq!(fixture.finish(&task)["status"], "cancelled");
    assert!(!workspace.exists(), "Python workspace was not cleaned");
    let denied = fixture.start("process.run", json!({"toolId":"foreign-tool","args":[]}));
    assert!(fixture.finish(&denied)["error"]["message"]
        .as_str()
        .unwrap()
        .contains("PROCESS_TOOL_UNDECLARED"));
    assert!(!fs::read_dir(data).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("process-")));
    fixture.manager.next_backend.shutdown().unwrap();
}

#[test]
#[ignore = "requires ZIPs built using the independent CLI templates"]
fn independent_sdk_templates_install_and_execute_through_the_public_contract() {
    let archive_root = PathBuf::from(
        std::env::var_os("CRUCIBLEBOX_NEXT3_SDK_SAMPLES").expect("SDK fixture required"),
    );
    let (manager, plugins, root) = setup("next3-sdk-templates");
    for (kind, native) in [
        ("frontend", false),
        ("provider", false),
        ("js", false),
        ("native", true),
    ] {
        let id = format!("sample-{kind}");
        let archive = archive_root.join(format!("{id}.zip"));
        manager.remember_trusted_path(archive.clone());
        let preview = manager
            .preview(InstallSource {
                source_type: "zip".into(),
                path: archive.to_string_lossy().into(),
            })
            .unwrap();
        let mut approval = next2_approval(&preview);
        approval.accept_native_code = native;
        manager
            .commit_approved(
                preview["installToken"].as_str().unwrap().into(),
                Some(approval),
            )
            .unwrap();
        manager.next_backend.next2.enable(&id).unwrap();
        if matches!(kind, "js" | "native") {
            assert_eq!(
                manager
                    .next_backend
                    .next2
                    .execute_command(&id, "ping")
                    .unwrap(),
                true
            );
        }
    }
    let source = root.join("consumer");
    write_next2(&source, "1.0.0", None);
    let path = source.join("plugin.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["dependencies"] = json!([{"id":"sample-provider","providerId":"sample-provider","version":{"min":"1.0.0","maxExclusive":"2.0.0"}}]);
    fs::write(path, manifest.to_string()).unwrap();
    manager.remember_trusted_path(source.clone());
    let preview = manager.preview(directory(&source)).unwrap();
    manager
        .commit_approved(
            preview["installToken"].as_str().unwrap().into(),
            Some(next2_approval(&preview)),
        )
        .unwrap();
    manager.next_backend.next2.enable(PLUGIN).unwrap();
    let session = next2_session(&manager, &plugins);
    let raw = json!({"wireVersion":WIRE_VERSION,"requestId":"renderer-1","session":session.token,
        "target":{"kind":"service","id":"sample-provider"},"method":"sum","params":{"numbers":[7,19]}});
    let response = manager
        .next_backend
        .next2
        .renderer(&raw.to_string(), &session)
        .unwrap();
    assert_eq!(response["result"], 26, "{response}");
    manager.next_backend.shutdown().unwrap();
}
