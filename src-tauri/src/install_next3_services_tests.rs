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
        Self::install_script(tag, permissions, None, customize)
    }
    fn install_script(
        tag: &str,
        permissions: &[&str],
        script: Option<&str>,
        customize: impl FnOnce(&mut Value),
    ) -> Self {
        let (manager, plugins, root) = setup(tag);
        let source = root.join("source");
        write_next2(
            &source,
            "1.0.0",
            Some(script.unwrap_or(
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
            )),
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

    fn restart(self) -> Self {
        let Self { manager, root, .. } = self;
        manager.next_backend.shutdown().unwrap();
        drop(manager);
        let db = Arc::new(std::sync::Mutex::new(
            crate::db::Db::open(&root.join("data/openbox.db")).unwrap(),
        ));
        let backend = crate::backend_process::BackendProcessManager::new(db.clone());
        let next_backend = Arc::new(crate::next_backend::Manager::new(db.clone()));
        let plugins = root.join("plugins");
        let manager = InstallManager::new(plugins.clone(), db, backend, next_backend);
        manager.next_backend.next2.enable(PLUGIN).unwrap();
        let session = next2_session(&manager, &plugins);
        Self {
            manager,
            session,
            root,
            sequence: Cell::new(0),
        }
    }
}

#[test]
fn extended_files_use_host_tasks_streams_permissions_and_os_notifications() {
    let fixture = Fixture::install_script(
        "extended-files",
        &["files:read", "files:write"],
        Some(
            r#"
module.exports.activate = context => {
  let sequence = 0;
  const platform = async (method, params = {}) => {
    const response = await context.exchange({wireVersion:context.wireVersion,requestId:'test-'+(++sequence),session:context.session,target:{kind:'platform'},method,params});
    if (!response.ok) throw Object.assign(Error(response.error.message), {code:response.error.code});
    return response.result;
  };
  return {exercise: async () => {
    const root = await platform('files.temporary',{kind:'directory',name:'files'});
    const watch = await platform('files.watch',{ref:root.ref,recursive:true});
    const writer = await platform('files.write.open',{directory:root.ref,name:'中文.txt',bytes:6});
    await platform('files.write.append',{streamId:writer.streamId,data:'5Lit5paH'});
    const written = await platform('files.write.commit',writer);
    const read = await platform('files.read',{ref:written.ref,offset:0,length:24});
    let traversal;
    try { await platform('files.child',{ref:root.ref,path:'../escape'}); } catch(error) { traversal=error.code; }
    const copied = await platform('files.copy',{ref:written.ref,directory:root.ref,name:'copied.txt'});
    const moved = await platform('files.move',{ref:copied.ref,directory:root.ref,name:'moved.txt'});
    await platform('files.remove',{ref:moved.ref});
    const abandoned = await platform('files.write.open',{directory:root.ref,name:'aborted.txt',bytes:6});
    await platform('files.write.append',{streamId:abandoned.streamId,data:'5Lit5paH'});
    await platform('files.write.abort',abandoned);
    let events=[];
    for(let index=0;index<100&&events.length===0;index++) events=(await platform('files.watch.poll',watch)).events;
    await platform('files.watch.close',watch);
    const page=await platform('files.directory.walk',{ref:root.ref,recursive:true});
    return {root,written,read,traversal,events,paths:page.items.map(item=>item.path)};
  }};
};"#,
        ),
        |manifest| {
            manifest["minHostVersion"] = json!("2.2.0-beta.7");
        },
    );
    let builtin = fixture.call("capabilities.describe", json!({"id":"host-files"}));
    assert_eq!(builtin["result"]["kind"], "host", "{builtin}");
    assert_eq!(builtin["result"]["availability"]["state"], "ready");
    assert!(builtin["result"]["methodContracts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|method| method["name"] == "files.write.open"));
    let result = fixture.finish(&fixture.start("exercise", json!({})));
    assert_eq!(result["status"], "succeeded", "{result}");
    assert_eq!(result["result"]["read"]["data"], "5Lit5paH");
    assert_eq!(result["result"]["traversal"], "INVALID_FILE_NAME");
    assert!(
        !result["result"]["events"].as_array().unwrap().is_empty(),
        "OS watcher must report actual writes"
    );
    assert_eq!(result["result"]["paths"], json!(["中文.txt"]));
    fixture.manager.next_backend.next2.stop(PLUGIN).unwrap();
}

#[test]
fn builtin_task_target_preserves_inner_permissions_and_host_identity() {
    let fixture = Fixture::install("builtin-task", &["files:read", "files:write"], |manifest| {
        manifest["minHostVersion"] = json!("2.2.0-beta.7");
        manifest.as_object_mut().unwrap().remove("backend");
        manifest["contributes"]["commands"] = json!([]);
    });
    let source = fixture.call("files.temporary", json!({"kind":"file","name":"source"}));
    let root = fixture.call(
        "files.temporary",
        json!({"kind":"directory","name":"target"}),
    );
    let response = fixture.call("tasks.start", json!({"target":{"kind":"platform"},"method":"files.copy","params":{"ref":source["result"]["ref"],"directory":root["result"]["ref"],"name":"copied"}}));
    let id = response["result"]["taskId"]
        .as_str()
        .unwrap_or_else(|| panic!("{response}"));
    let result = fixture.finish(id);
    assert_eq!(result["status"], "succeeded", "{result}");
    assert_eq!(result["providerId"], "cruciblebox-host");
    assert_eq!(result["initiatorId"], PLUGIN);
    let denied = fixture.call("tasks.start", json!({"target":{"kind":"platform"},"method":"network.download","params":{"url":"https://example.com/","name":"file","bytes":1,"sha256":"a".repeat(64)}}));
    assert_eq!(denied["error"]["code"], "PERMISSION_DENIED", "{denied}");
    let denied = fixture.call(
        "tasks.start",
        json!({"target":{"kind":"platform"},"method":"storage.clear","params":{}}),
    );
    assert_eq!(denied["ok"], false, "{denied}");
    fixture.manager.next_backend.shutdown().unwrap();
}

#[test]
fn provider_catalog_reports_real_activation_failure() {
    let fixture = Fixture::install("provider-activation-failure", &[], |_| {});
    let source = fixture.root.join("failed-provider");
    write_next2(
        &source,
        "1.0.0",
        Some("module.exports.activate=()=>{throw Error('fixture activation failure')};"),
    );
    let path = source.join("plugin.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["id"] = json!("acme-broken");
    manifest["exports"] = json!([{"id":"failed-provider","version":"1.0.0","interchangeable":true,"methods":["run"]}]);
    fs::write(path, manifest.to_string()).unwrap();
    fixture.manager.remember_trusted_path(source.clone());
    let preview = fixture.manager.preview(directory(&source)).unwrap();
    fixture
        .manager
        .commit_approved(
            preview["installToken"].as_str().unwrap().into(),
            Some(next2_approval(&preview)),
        )
        .unwrap();
    // Restore the saved enabled preference before attempting startup, as on host reopen.
    lock(&fixture.manager.db)
        .set_plugin_enabled("acme-broken", true)
        .unwrap();
    let registered = fixture.call("capabilities.providers", json!({"id":"failed-provider"}));
    assert_eq!(
        registered["result"]["items"][0]["state"], "registered",
        "{registered}"
    );
    let activation_error = fixture
        .manager
        .next_backend
        .next2
        .enable("acme-broken")
        .unwrap_err();
    let failed = fixture.call("capabilities.providers", json!({"id":"failed-provider"}));
    assert_eq!(failed["result"]["items"][0]["state"], "failed", "{failed}");
    assert!(
        failed["result"]["items"][0]["error"]["message"]
            .as_str()
            .unwrap()
            .contains(&activation_error),
        "{failed}"
    );
    fixture.manager.next_backend.shutdown().unwrap();
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

fn read_http(stream: &mut std::net::TcpStream) -> (String, Vec<u8>) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut header = Vec::new();
    let mut byte = [0];
    while !header.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).unwrap();
        header.push(byte[0]);
        assert!(header.len() < 16384);
    }
    let header = String::from_utf8(header).unwrap();
    let length = header
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|value| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    (header, body)
}

#[test]
fn binary_http_and_file_upload_use_actual_bytes_and_host_task() {
    let fixture = Fixture::install("binary-upload", &["network:fetch", "files:read"], |_| {});
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/binary", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        for (method, expected) in [
            ("POST", vec![0, 255, 128, 10]),
            ("PUT", vec![0, 1, 255, 0, 128]),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            let (header, body) = read_http(&mut stream);
            assert!(header.starts_with(method), "{header}");
            assert_eq!(body, expected);
            stream.write_all(b"HTTP/1.1 418 Test\r\nContent-Length: 4\r\nConnection: close\r\n\r\n\x00\xff\x80\n").unwrap();
        }
    });
    let response = fixture.call(
        "network.request",
        json!({"url":url,"method":"POST","body":STANDARD.encode([0,255,128,10])}),
    );
    assert_eq!(response["result"]["status"], 418, "{response}");
    assert_eq!(
        response["result"]["body"],
        STANDARD.encode([0, 255, 128, 10])
    );
    let source = fixture.root.join("upload.bin");
    fs::write(&source, [0, 1, 255, 0, 128]).unwrap();
    let reference = fixture
        .manager
        .next_backend
        .next2
        .grant_fixture_file(PLUGIN, &source);
    let started = fixture.call("tasks.start", json!({"target":{"kind":"platform"},"method":"network.upload","params":{"url":url,"method":"PUT","ref":reference.r#ref}}));
    let result = fixture.finish(
        started["result"]["taskId"]
            .as_str()
            .unwrap_or_else(|| panic!("{started}")),
    );
    assert_eq!(result["status"], "succeeded", "{result}");
    assert_eq!(result["result"]["status"], 418);
    let denied = fixture.call(
        "network.request",
        json!({"url":url,"method":"GET","headers":{"Proxy-Authorization":"secret"}}),
    );
    assert_eq!(denied["error"]["code"], "HTTP_HEADER_RESERVED", "{denied}");
    server.join().unwrap();
    fixture.manager.next_backend.shutdown().unwrap();
}

#[test]
fn interrupted_download_resumes_exact_range_and_publishes_only_verified_file() {
    let fixture = Fixture::install(
        "resumable-download",
        &["network:download", "files:read"],
        |_| {},
    );
    let data = vec![37u8; 131072];
    let digest = format!("{:x}", Sha256::digest(&data));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/resume", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let (header, _) = read_http(&mut stream);
        assert!(!header.to_ascii_lowercase().contains("range:"));
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"fixture-v1\"\r\nConnection: close\r\n\r\n", data.len()).unwrap();
        stream.write_all(&data[..65536]).unwrap();
        drop(stream);
        let (mut stream, _) = listener.accept().unwrap();
        let (header, _) = read_http(&mut stream);
        let header = header.to_ascii_lowercase();
        assert!(header.contains("range: bytes=65536-"), "{header}");
        assert!(header.contains("if-range: \"fixture-v1\""), "{header}");
        write!(stream, "HTTP/1.1 206 Partial Content\r\nContent-Length: 65536\r\nContent-Range: bytes 65536-131071/131072\r\nETag: \"fixture-v1\"\r\nConnection: close\r\n\r\n").unwrap();
        stream.write_all(&data[65536..]).unwrap();
    });
    let prepared = fixture.call(
        "network.download.prepare",
        json!({"url":url,"name":"download.bin","bytes":131072,"sha256":digest}),
    );
    assert_eq!(prepared["ok"], true, "{prepared}");
    let params = json!({"downloadId":prepared["result"]["downloadId"]});
    let start = |fixture: &Fixture| {
        fixture.call("tasks.start", json!({"target":{"kind":"platform"},"method":"network.download.resume","params":params}))
    };
    let started = start(&fixture);
    let failed = fixture.finish(
        started["result"]["taskId"]
            .as_str()
            .unwrap_or_else(|| panic!("{started}")),
    );
    assert_eq!(failed["status"], "failed", "{failed}");
    assert!(
        failed["resultRefs"].as_array().unwrap().is_empty(),
        "partial file was published"
    );
    assert_eq!(
        fixture.call("network.download.state", params.clone())["result"]["received"],
        65536
    );
    let fixture = fixture.restart();
    assert_eq!(
        fixture.call("network.download.state", params.clone())["result"]["received"],
        65536
    );
    let started = start(&fixture);
    let result = fixture.finish(started["result"]["taskId"].as_str().unwrap());
    assert_eq!(result["status"], "succeeded", "{result}");
    let content = fixture.call(
        "files.read",
        json!({"ref":result["result"]["ref"],"offset":0,"length":100}),
    );
    assert_eq!(content["result"]["data"], STANDARD.encode(vec![37u8; 100]));
    assert_eq!(
        fixture.call("network.download.discard", params)["result"],
        true
    );
    server.join().unwrap();
    fixture.manager.next_backend.shutdown().unwrap();
}

#[test]
fn websocket_exchanges_real_text_binary_and_releases_session_socket() {
    let fixture = Fixture::install("websocket-session", &["network:fetch"], |_| {});
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}/echo", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        assert_eq!(
            socket.read().unwrap(),
            tungstenite::Message::Text("中文".into())
        );
        socket
            .send(tungstenite::Message::Text("中文".into()))
            .unwrap();
        assert_eq!(
            socket.read().unwrap(),
            tungstenite::Message::Binary(vec![0, 255, 128].into())
        );
        socket
            .send(tungstenite::Message::Binary(vec![0, 255, 128].into()))
            .unwrap();
        assert!(
            socket.read().is_err(),
            "disposed session must close TCP connection"
        );
    });
    let opened = fixture.call("network.websocket.open", json!({"url":url}));
    assert_eq!(opened["ok"], true, "{opened}");
    let id = &opened["result"]["socketId"];
    assert_eq!(
        fixture.call(
            "network.websocket.send",
            json!({"socketId":id,"text":"中文"})
        )["result"],
        true
    );
    let read = || {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let response = fixture.call("network.websocket.poll", json!({"socketId":id}));
            assert_eq!(response["ok"], true, "{response}");
            if !response["result"]["messages"]
                .as_array()
                .unwrap()
                .is_empty()
            {
                break response["result"]["messages"][0].clone();
            }
            assert!(Instant::now() < deadline, "{response}");
            thread::sleep(Duration::from_millis(10));
        }
    };
    assert_eq!(
        read(),
        json!({"kind":"text","data":STANDARD.encode("中文")})
    );
    assert_eq!(
        fixture.call(
            "network.websocket.send",
            json!({"socketId":id,"data":STANDARD.encode([0,255,128])})
        )["result"],
        true
    );
    assert_eq!(
        read(),
        json!({"kind":"binary","data":STANDARD.encode([0,255,128])})
    );
    fixture
        .manager
        .next_backend
        .next2
        .dispose_renderer_session(&fixture.session.token)
        .unwrap();
    server.join().unwrap();
    fixture.manager.next_backend.shutdown().unwrap();
}

#[test]
fn resumable_download_rejects_wrong_ranges_and_hash_without_publishing() {
    let fixture = Fixture::install("resume-rejections", &["network:download"], |_| {});
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/resume", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        for attempt in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            let (header, _) = read_http(&mut stream);
            if attempt == 0 {
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: 131072\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
                stream.write_all(&vec![42; 65536]).unwrap();
            } else if attempt == 1 {
                assert!(header.to_ascii_lowercase().contains("range: bytes=65536-"));
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 131072\r\nConnection: close\r\n\r\n",
                    )
                    .unwrap();
            } else {
                stream.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Length: 65536\r\nContent-Range: bytes 65536-131071/131072\r\nConnection: close\r\n\r\n").unwrap();
                stream.write_all(&vec![42; 65536]).unwrap();
            }
        }
    });
    let prepared = fixture.call(
        "network.download.prepare",
        json!({"url":url,"name":"invalid.bin","bytes":131072,"sha256":"0".repeat(64)}),
    );
    let params = json!({"downloadId":prepared["result"]["downloadId"]});
    for (attempt, expected) in [
        "NETWORK_READ_FAILED",
        "DOWNLOAD_RANGE_MISMATCH",
        "DOWNLOAD_INTEGRITY_MISMATCH",
    ]
    .iter()
    .enumerate()
    {
        let started = fixture.call("tasks.start",json!({"target":{"kind":"platform"},"method":"network.download.resume","params":params}));
        let failed = fixture.finish(started["result"]["taskId"].as_str().unwrap());
        assert_eq!(failed["status"], "failed", "{failed}");
        assert!(
            failed["resultRefs"].as_array().unwrap().is_empty(),
            "{failed}"
        );
        assert!(
            failed["error"]["message"]
                .as_str()
                .unwrap()
                .contains(expected),
            "{failed}"
        );
        assert_eq!(
            fixture.call("network.download.state", params.clone())["result"]["received"],
            if attempt == 2 { 131072 } else { 65536 }
        );
    }
    assert_eq!(
        fixture.call("network.download.discard", params)["result"],
        true
    );
    server.join().unwrap();
    fixture.manager.next_backend.shutdown().unwrap();
}

#[test]
fn file_writes_release_abandoned_streams_and_preserve_recursive_replace_rules() {
    let fixture = Fixture::install_script(
        "file-boundaries",
        &["files:read", "files:write"],
        Some(
            r#"
module.exports.activate = context => {
  let sequence=0;
  const call=async(method,params={})=>{const r=await context.exchange({wireVersion:context.wireVersion,requestId:'file-'+(++sequence),session:context.session,target:{kind:'platform'},method,params});if(!r.ok)throw Object.assign(Error(r.error.message),{code:r.error.code});return r.result;};
  return {
    abandon: async()=>{const root=await call('files.temporary',{kind:'directory',name:'abandon'});return {root,writer:await call('files.write.open',{directory:root.ref,name:'unfinished',bytes:1})};},
    reuse: params=>call('files.write.append',{streamId:params.streamId,data:'YQ=='}),
    release: params=>call('files.temporary.release',params),
    recursive: async()=>{
      const root=await call('files.temporary',{kind:'directory',name:'tree'});
      const source=await call('files.directory.create',{ref:root.ref,name:'source'});
      const child=await call('files.directory.create',{ref:source.ref,name:'nested'});
      const writer=await call('files.write.open',{directory:child.ref,name:'file',bytes:1});
      await call('files.write.append',{streamId:writer.streamId,data:'YQ=='});await call('files.write.commit',writer);
      let nonrecursive,overwrite,overlap;
      try{await call('files.copy',{ref:source.ref,directory:root.ref,name:'copy'});}catch(e){nonrecursive=e.code;}
      const copied=await call('files.copy',{ref:source.ref,directory:root.ref,name:'copy',recursive:true});
      try{await call('files.copy',{ref:source.ref,directory:root.ref,name:'copy',recursive:true});}catch(e){overwrite=e.code;}
      try{await call('files.move',{ref:source.ref,directory:child.ref,name:'inside',recursive:true});}catch(e){overlap=e.code;}
      await call('files.copy',{ref:source.ref,directory:root.ref,name:'copy',recursive:true,replace:true});
      const paths=(await call('files.directory.walk',{ref:copied.ref,recursive:true})).items.map(x=>x.path);
      await call('files.remove',{ref:copied.ref,recursive:true});
      return {nonrecursive,overwrite,overlap,paths,root};
    }
  };
};"#,
        ),
        |_| {},
    );
    for _ in 0..10 {
        let result = fixture.finish(&fixture.start("abandon", json!({})));
        assert_eq!(result["status"], "succeeded", "{result}");
        let root = &result["result"]["root"]["ref"];
        let walked = fixture.call("files.directory.walk", json!({"ref":root}));
        assert_eq!(
            walked["result"]["items"],
            json!([]),
            "abandoned stage remains: {walked}"
        );
        let denied = fixture.finish(&fixture.start("reuse", result["result"]["writer"].clone()));
        assert_eq!(denied["status"], "failed", "{denied}");
        assert!(denied["error"]["message"]
            .as_str()
            .unwrap()
            .contains("FILE_STREAM_UNAVAILABLE"));
        // Temporary resources are session-scoped and explicitly releasable by their owner.
        let released = fixture.finish(&fixture.start("release", json!({"ref":root})));
        assert_eq!(released["status"], "succeeded", "{released}");
    }
    let result = fixture.finish(&fixture.start("recursive", json!({})));
    assert_eq!(result["status"], "succeeded", "{result}");
    assert_eq!(
        result["result"]["nonrecursive"],
        "RECURSIVE_OPERATION_REQUIRED"
    );
    assert_eq!(result["result"]["overwrite"], "FILE_ALREADY_EXISTS");
    assert_eq!(
        result["result"]["overlap"],
        "FILE_DESTINATION_OVERLAPS_SOURCE"
    );
    assert_eq!(result["result"]["paths"], json!(["nested", "nested/file"]));
    fixture.manager.next_backend.shutdown().unwrap();
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
fn runtime_provider_shares_one_approved_python_between_independent_consumers() {
    let asset = PathBuf::from(
        std::env::var_os("CRUCIBLEBOX_NEXT3_PYTHON_RESOURCE").expect("Python fixture required"),
    );
    let fixture = Fixture::install_script(
        "shared-runtime",
        &["process:run"],
        Some(
            r#"
module.exports.activate = context => {
 let sequence=0;
 return {run: async params => {
  const response = await context.exchange({wireVersion:context.wireVersion,requestId:'runtime-'+(++sequence),session:context.session,target:{kind:'platform'},method:'process.run',params:{...params,toolId:'python'}});
  if(!response.ok) throw Object.assign(Error(response.error.message),{code:response.error.code});
  return response.result;
 }};
};"#,
        ),
        |manifest| {
            manifest["resources"] = json!([{"id":"python-runtime","version":{"min":"1.0.0","maxExclusive":"2.0.0"},"optional":true}]);
            manifest["tools"] =
                json!([{"id":"python","resourceId":"python-runtime","entry":"python.exe"}]);
            manifest["exports"] = json!([{"id":"shared-python","version":"1.0.0","methods":["run"],"interchangeable":true}]);
            manifest["runtimeCapabilities"] =
                json!([{"capabilityId":"shared-python","toolId":"python","engine":"python"}]);
        },
    );
    let mut consumers = Vec::new();
    for id in ["runtime-consumer-alpha", "runtime-consumer-beta"] {
        let source = fixture.root.join(id);
        write_next2(&source, "1.0.0", None);
        let path = source.join("plugin.json");
        let mut manifest: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        manifest["id"] = json!(id);
        manifest["permissions"].as_array_mut().unwrap().extend([
            json!("runtime:execute"),
            json!("files:read"),
            json!("files:write"),
        ]);
        manifest["contributes"]["commands"] = json!([]);
        manifest["dependencies"] = json!([{"id":"shared-python","providerId":PLUGIN,"version":{"min":"1.0.0","maxExclusive":"2.0.0"},"optional":true}]);
        manifest["runtimeRequirements"] = json!([{"capabilityId":"shared-python","engine":"python","version":{"min":"3.14.0","maxExclusive":"3.15.0"}}]);
        assert!(manifest["resources"].as_array().unwrap().is_empty());
        fs::write(path, manifest.to_string()).unwrap();
        fixture.manager.remember_trusted_path(source.clone());
        let preview = fixture.manager.preview(directory(&source)).unwrap();
        fixture
            .manager
            .commit_approved(
                preview["installToken"].as_str().unwrap().into(),
                Some(next2_approval(&preview)),
            )
            .unwrap();
        fixture.manager.next_backend.next2.enable(id).unwrap();
        let session = next2_session_for(&fixture.manager, &fixture.root.join("plugins"), id);
        consumers.push(Fixture {
            manager: fixture.manager.clone(),
            session,
            root: fixture.root.clone(),
            sequence: Cell::new(0),
        });
    }
    let missing = consumers[0].call("runtime.resolve", json!({"id":"shared-python"}));
    assert!(missing["result"].get("binding").is_none(), "{missing}");
    assert_eq!(missing["result"]["availability"]["state"], "unavailable");
    let prepared = fixture.root.join("python-resource");
    fs::create_dir(&prepared).unwrap();
    let mut resource: Value =
        serde_json::from_slice(&fs::read(asset.join("resource.json")).unwrap()).unwrap();
    cruciblebox_next2_protocol::validate_resource_manifest(&resource.to_string()).unwrap();
    for entry in resource["files"].as_array().unwrap() {
        let relative = entry["path"].as_str().unwrap();
        let target = prepared.join(relative);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(asset.join(relative), target).unwrap();
    }
    let version = std::process::Command::new(prepared.join("python.exe"))
        .args([
            "-I",
            "-S",
            "-B",
            "-c",
            "import sys; print('.'.join(map(str,sys.version_info[:3])))",
        ])
        .current_dir(&fixture.root)
        .output()
        .unwrap();
    assert!(version.status.success());
    let engine_version = String::from_utf8(version.stdout).unwrap().trim().to_owned();
    assert!(engine_version.starts_with("3.14."), "{engine_version}");
    resource["version"] = json!("1.4.0");
    resource["runtimeIdentity"] = json!({"engine":"python","version":engine_version});
    fs::write(prepared.join("resource.json"), resource.to_string()).unwrap();
    fixture.manager.remember_trusted_path(prepared.clone());
    let preview = fixture
        .manager
        .resources
        .preview(&fixture.manager, PLUGIN, directory(&prepared))
        .unwrap();
    fixture
        .manager
        .resources
        .commit(
            preview["token"].as_str().unwrap(),
            Approval {
                manifest_sha256: serde_json::from_value(preview["manifestSha256"].clone()).unwrap(),
                accept_native_code: true,
                accept_unsigned_local: true,
                accept_permissions: true,
            },
        )
        .unwrap();
    let mut executables = Vec::new();
    for consumer in &consumers {
        let resolved = consumer.call("runtime.resolve", json!({"id":"shared-python"}));
        assert_eq!(
            resolved["result"]["binding"]["providerId"], PLUGIN,
            "{resolved}"
        );
        assert_eq!(resolved["result"]["binding"]["export"]["version"], "1.0.0");
        assert_eq!(resolved["result"]["environment"]["version"], engine_version);
        assert_eq!(
            resolved["result"]["environment"]["resourceVersion"],
            "1.4.0"
        );
        let cwd = consumer.call("files.temporary", json!({"kind":"directory","name":"work"}));
        // Runtime execution is delegated with a consumer-owned cwd and explicit environment.
        let started=consumer.call("tasks.start",json!({"target":{"kind":"service","id":"shared-python"},"method":"run","params":{"args":["-I","-S","-B","-c","import os,sys,json; print(json.dumps([sys.executable,os.path.basename(os.getcwd()),os.environ['CB_TEST'],os.environ['TEMP']]))"],"workingDirectory":cwd["result"]["ref"],"environment":{"CB_TEST":"中文"}}}));
        let result = consumer.finish(
            started["result"]["taskId"]
                .as_str()
                .unwrap_or_else(|| panic!("{started}")),
        );
        assert_eq!(result["status"], "succeeded", "{result}");
        assert_eq!(result["providerId"], PLUGIN);
        let stdout = STANDARD
            .decode(result["result"]["stdout"].as_str().unwrap())
            .unwrap();
        let printed: Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(printed[1], "work");
        assert_eq!(printed[2], "中文");
        assert!(Path::new(printed[3].as_str().unwrap()).starts_with(&fixture.root));
        assert!(
            !Path::new(printed[3].as_str().unwrap()).exists(),
            "scratch must be released"
        );
        executables.push(printed[0].clone());
    }
    assert_eq!(
        executables[0], executables[1],
        "both consumers must execute the same installed runtime"
    );
    let started=consumers[0].call("tasks.start",json!({"target":{"kind":"service","id":"shared-python"},"method":"run","params":{"args":["-I","-S","-B","-c","import time; time.sleep(30)"]}}));
    let task = started["result"]["taskId"].as_str().unwrap();
    consumers[0].wait(task, |value| value["status"] == "running");
    assert_eq!(
        consumers[0].call("tasks.cancel", json!({"taskId":task}))["result"]["accepted"],
        true
    );
    assert_eq!(consumers[0].finish(task)["status"], "cancelled");
    let providers = consumers[0].call("capabilities.providers", json!({"id":"shared-python"}));
    assert_eq!(
        providers["result"]["items"][0]["providerId"], PLUGIN,
        "{providers}"
    );
    fixture.manager.next_backend.next2.stop(PLUGIN).unwrap();
    let row = lock(&fixture.manager.db)
        .plugin_find_by_name(PLUGIN)
        .unwrap()
        .unwrap();
    lock(&fixture.manager.db)
        .set_plugin_enabled(&row.id, false)
        .unwrap();
    let providers = consumers[0].call("capabilities.providers", json!({"id":"shared-python"}));
    assert_eq!(
        providers["result"]["items"][0]["state"], "stopped",
        "{providers}"
    );
    assert!(
        consumers[0].call("runtime.resolve", json!({"id":"shared-python"}))["result"]
            .get("binding")
            .is_none()
    );
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
