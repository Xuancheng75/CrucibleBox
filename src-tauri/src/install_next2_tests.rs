use super::{lock, tests::*, InstallSource};
use base64::Engine;
use cruciblebox_next2_runtime::package::Approval;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};

fn directory(path: &Path) -> InstallSource {
    InstallSource {
        source_type: "directory".into(),
        path: path.to_string_lossy().into(),
    }
}

#[test]
fn next3_ipc_events_streaming_and_request_cancellation_cross_process() {
    let (manager, plugins, root) = setup("next3-ipc-integration");
    let provider = root.join("provider");
    write_next2(
        &provider,
        "1.0.0",
        Some(
            r#"
let request = 0;
module.exports.activate = ctx => {
  let stream;
  const call = async (method, params) => {
    const reply = await ctx.exchange({wireVersion:ctx.wireVersion,requestId:'ipc-'+(++request),session:ctx.session,target:{kind:'platform'},method,params});
    if (!reply.ok) throw Object.assign(Error(reply.error.message), {code:reply.error.code});
    return reply.result;
  };
  const emit = payload => call('events.publish',{capabilityId:'sample-ipc',event:'progress',payload});
  return {
    open: async ({ref}) => {stream = await call('files.stream.open',{ref});return stream;},
    chunk: ({offset,length}) => call('files.stream.read',{streamId:stream.streamId,offset,length}),
    close: () => call('files.stream.close',{streamId:stream.streamId}),
    emit: async ({count}) => {for(let index=1;index<=count;index++) await emit({index});return count;},
    invalid: () => emit({index:'invalid'}),
    hang: async () => {await call('storage.set',{key:'rpc-started',value:true});while(true){}},
    ping: () => 'alive'
  };
};"#,
        ),
    );
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(provider.join("plugin.json")).unwrap()).unwrap();
    manifest["id"] = json!("ipc-provider");
    manifest["permissions"]
        .as_array_mut()
        .unwrap()
        .extend([json!("files:read"), json!("events:publish")]);
    manifest["exports"] = json!([{"id":"sample-ipc","version":"1.0.0","methods":["open","chunk","close","emit","invalid","hang","ping"],"events":[{
        "name":"progress","payload":{"type":"object","properties":{"index":{"type":"integer"}},"required":["index"],"additionalProperties":false}
    }]}]);
    fs::write(provider.join("plugin.json"), manifest.to_string()).unwrap();
    let client = root.join("client");
    write_next2(&client, "1.0.0", None);
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(client.join("plugin.json")).unwrap()).unwrap();
    manifest["permissions"]
        .as_array_mut()
        .unwrap()
        .extend([json!("files:read"), json!("events:subscribe")]);
    manifest["dependencies"] = json!([{"id":"sample-ipc","providerId":"ipc-provider","version":{"min":"1.0.0","maxExclusive":"2.0.0"}}]);
    fs::write(client.join("plugin.json"), manifest.to_string()).unwrap();
    for (source, id) in [(provider, "ipc-provider"), (client, "acme-independent")] {
        manager.remember_trusted_path(source.clone());
        let preview = manager.preview(directory(&source)).unwrap();
        manager
            .commit_approved(
                preview["installToken"].as_str().unwrap().into(),
                Some(next2_approval(&preview)),
            )
            .unwrap();
        manager.next_backend.next2.enable(id).unwrap();
    }

    let host = &manager.next_backend.next2;
    let session = next2_session(&manager, &plugins);
    let foreign = next2_session(&manager, &plugins);
    let request = |session: &crate::plugin_session::RendererSession,
                   index,
                   target,
                   method: &str,
                   params| {
        json!({"wireVersion":cruciblebox_next2_protocol::WIRE_VERSION,"requestId":format!("renderer-{index}"),"session":session.token,"target":target,"method":method,"params":params}).to_string()
    };
    let sequence = std::cell::Cell::new(0u64);
    let next = || {
        sequence.set(sequence.get() + 1);
        sequence.get()
    };
    let platform = |method: &str, params| {
        host.renderer(
            &request(&session, next(), json!({"kind":"platform"}), method, params),
            &session,
        )
        .unwrap()
    };
    let service = |method: &str, params| {
        host.renderer(
            &request(
                &session,
                next(),
                json!({"kind":"service","id":"sample-ipc"}),
                method,
                params,
            ),
            &session,
        )
        .unwrap()
    };

    let subscription = platform(
        "events.subscribe",
        json!({"capabilityId":"sample-ipc","event":"progress"}),
    );
    let subscription_id = subscription["result"]["subscriptionId"].as_str().unwrap();
    let topic = json!({"subscriptionId":subscription_id});
    let event_count = cruciblebox_next2_protocol::BUDGET_EVENT_QUEUE + 2;
    assert_eq!(
        service("emit", json!({"count":event_count}))["result"],
        event_count
    );
    let rejected = host
        .renderer(
            &request(
                &foreign,
                1,
                json!({"kind":"platform"}),
                "events.poll",
                topic.clone(),
            ),
            &foreign,
        )
        .unwrap();
    assert_eq!(rejected["error"]["code"], "EVENT_SUBSCRIPTION_UNAVAILABLE");
    let event = platform("events.poll", topic.clone());
    assert_eq!(
        event["result"],
        json!({"event":{"sequence":3,"publisherId":"ipc-provider","capabilityId":"sample-ipc","event":"progress","payload":{"index":3}},"dropped":2})
    );
    assert_eq!(
        service("invalid", json!({}))["error"]["code"],
        "INVALID_EVENT_PAYLOAD"
    );
    assert_eq!(
        platform("events.unsubscribe", topic.clone())["result"],
        true
    );
    assert_eq!(
        platform("events.poll", topic)["error"]["code"],
        "EVENT_SUBSCRIPTION_UNAVAILABLE"
    );

    let path = root.join("payload.bin");
    let original: Vec<_> = (0..180_000).map(|index| (index % 251) as u8).collect();
    fs::write(&path, &original).unwrap();
    let grant = host.grant_fixture_file("acme-independent", &path);
    let opened = service("open", json!({"ref":grant.r#ref}));
    assert_eq!(opened["result"]["bytes"], original.len(), "{opened}");
    let stream_id = opened["result"]["streamId"].as_str().unwrap();
    let rejected = platform(
        "files.stream.read",
        json!({"streamId":stream_id,"offset":0,"length":1}),
    );
    assert_eq!(rejected["error"]["code"], "FILE_STREAM_UNAVAILABLE");
    let mut received = Vec::new();
    while received.len() < original.len() {
        let result = service(
            "chunk",
            json!({"offset":received.len(),"length":cruciblebox_next2_protocol::BUDGET_FILE_STREAM_CHUNK_BYTES}),
        );
        let chunk = &result["result"];
        let data = base64::engine::general_purpose::STANDARD
            .decode(chunk["data"].as_str().unwrap())
            .unwrap();
        assert!(!data.is_empty());
        received.extend(data);
        assert_eq!(chunk["nextOffset"], received.len());
        assert_eq!(chunk["eof"], received.len() == original.len());
    }
    assert_eq!(received, original);
    assert_eq!(service("close", json!({}))["result"], true);
    assert_eq!(service("open", json!({"ref":grant.r#ref}))["ok"], true);
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(1)
        .unwrap();
    assert_eq!(
        service("chunk", json!({"offset":0,"length":1}))["error"]["code"],
        "FILE_STREAM_CHANGED"
    );
    assert_eq!(service("close", json!({}))["result"], false);
    fs::remove_file(&path).unwrap();
    let outside = root.join("outside-grant.bin");
    fs::write(&outside, b"unauthorized").unwrap();
    std::os::windows::fs::symlink_file(&outside, &path).unwrap();
    assert_eq!(
        service("open", json!({"ref":grant.r#ref}))["error"]["code"],
        "FILE_AUTHORITY_DENIED"
    );

    let request_index = next();
    let raw = request(
        &session,
        request_index,
        json!({"kind":"service","id":"sample-ipc"}),
        "hang",
        json!({}),
    );
    std::thread::scope(|scope| {
        let pending = scope.spawn(|| host.renderer(&raw, &session).unwrap());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while lock(&manager.db)
            .storage_get("ipc-provider", "rpc-started")
            .unwrap()
            .is_none()
        {
            assert!(
                std::time::Instant::now() < deadline,
                "provider never started request"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let target = json!({"requestId":format!("renderer-{request_index}")});
        let refused = host
            .renderer(
                &request(
                    &foreign,
                    2,
                    json!({"kind":"platform"}),
                    "rpc.cancel",
                    target.clone(),
                ),
                &foreign,
            )
            .unwrap();
        assert_eq!(refused["result"], false);
        assert_eq!(platform("rpc.cancel", target)["result"], true);
        assert_eq!(pending.join().unwrap()["error"]["code"], "CANCELLED");
    });
    assert_eq!(service("ping", json!({}))["result"], "alive");
    let started = platform(
        "tasks.start",
        json!({"target":{"kind":"service","id":"sample-ipc"},"method":"hang","params":{},"timeoutMs":250}),
    );
    let task_id = started["result"]["taskId"].as_str().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let task = platform("tasks.get", json!({"taskId":task_id}));
        if task["result"]["status"] == "failed" {
            assert!(
                task["result"]["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("TASK_TIMEOUT"),
                "{task}"
            );
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "task did not time out: {task}"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    manager.next_backend.shutdown().unwrap();
}

#[test]
fn next2_failed_start_keeps_disabled_and_successful_start_survives_shutdown_preference() {
    let (manager, _, root) = setup("next2-enable-lifecycle");
    let source = root.join("source");
    write_next2(
        &source,
        "1.0.0",
        Some("throw new Error('fixture activation failed')"),
    );
    manager.remember_trusted_path(source.clone());
    let preview = manager.preview(directory(&source)).unwrap();
    manager
        .commit_approved(
            preview["installToken"].as_str().unwrap().into(),
            Some(next2_approval(&preview)),
        )
        .unwrap();
    let owner = "acme-independent";
    assert!(manager.next_backend.next2.enable(owner).is_err());
    assert!(
        !lock(&manager.db)
            .plugin_find_by_id(owner)
            .unwrap()
            .unwrap()
            .enabled
    );

    write_next2(
        &source,
        "1.1.0",
        Some("module.exports.activate = async () => ({ping: () => 'pong'});"),
    );
    let preview = manager.preview(directory(&source)).unwrap();
    manager
        .commit_approved(
            preview["installToken"].as_str().unwrap().into(),
            Some(next2_approval(&preview)),
        )
        .unwrap();
    manager.next_backend.next2.enable(owner).unwrap();
    assert!(
        lock(&manager.db)
            .plugin_find_by_id(owner)
            .unwrap()
            .unwrap()
            .enabled
    );
    manager.next_backend.shutdown().unwrap();
    assert!(
        lock(&manager.db)
            .plugin_find_by_id(owner)
            .unwrap()
            .unwrap()
            .enabled
    );
}

#[test]
fn background_services_restore_without_renderer_and_isolate_failed_startup() {
    let (manager, _, root) = setup("next2-background-restore");
    let install = |id: &str, background: bool, fails: bool, dependencies: Value| {
        let source = root.join(id);
        let backend = if fails {
            "throw Error('fixture startup failure')"
        } else if id == "healthy" {
            r#"let sequence = 0;
module.exports.activate = () => ({ ping: () => 'restored' });
module.exports.background = async ctx => {
  const call = async (method, params) => {
    const requestId = 'heartbeat-' + (++sequence);
    const reply = await ctx.exchange({wireVersion:ctx.wireVersion,requestId,session:ctx.session,target:{kind:'platform'},method,params});
    if (!reply.ok) throw Error(reply.error.code);
    return reply.result;
  };
  if (await call('storage.get', {key:'fail-tick'})) throw Error('intentional hook failure');
  await call('storage.set', {key:'heartbeat',value:1+(await call('storage.get', {key:'heartbeat'}) || 0)});
};"#
        } else {
            "module.exports.activate = () => ({ ping: () => 'restored' });"
        };
        write_next2(&source, "1.0.0", Some(backend));
        let mut manifest: Value =
            serde_json::from_slice(&fs::read(source.join("plugin.json")).unwrap()).unwrap();
        manifest.as_object_mut().unwrap().remove("renderer");
        manifest["contributes"]
            .as_object_mut()
            .unwrap()
            .remove("navigation");
        manifest["files"]
            .as_array_mut()
            .unwrap()
            .retain(|file| file["path"] != "dist/renderer.js");
        fs::remove_file(source.join("dist/renderer.js")).unwrap();
        manifest["id"] = json!(id);
        manifest["background"] = json!(background);
        if id == "healthy" {
            manifest["backgroundIntervalMs"] = json!(1000);
        }
        manifest["dependencies"] = dependencies;
        manifest["exports"] = json!([{"id":id,"version":"1.0.0","methods":["ping"]}]);
        manifest["contributes"]["commands"] = json!([{"id":"ping","title":"Ping","method":"ping"}]);
        fs::write(source.join("plugin.json"), manifest.to_string()).unwrap();
        manager.remember_trusted_path(source.clone());
        let preview = manager.preview(directory(&source)).unwrap();
        manager
            .commit_approved(
                preview["installToken"].as_str().unwrap().into(),
                Some(next2_approval(&preview)),
            )
            .unwrap();
    };
    install("a-broken", true, true, json!([]));
    install("on-demand", false, false, json!([]));
    install("z-dependency", false, false, json!([]));
    install(
        "healthy",
        true,
        false,
        json!([{"id":"z-dependency","providerId":"z-dependency","version":{"min":"1.0.0","maxExclusive":"2.0.0"}}]),
    );
    for id in ["on-demand", "z-dependency", "healthy"] {
        manager.next_backend.next2.enable(id).unwrap();
    }
    // An enabled service can fail on a later startup even with a valid package.
    lock(&manager.db)
        .set_plugin_enabled("a-broken", true)
        .unwrap();
    manager.next_backend.shutdown().unwrap();
    assert!(!manager.next_backend.next2.has_backend_slot("healthy"));

    let db = crate::db::Db::open(&root.join("data/openbox.db")).unwrap();
    let restarted =
        crate::next_backend::Manager::new(std::sync::Arc::new(std::sync::Mutex::new(db.clone())));
    restarted.next2.restore_background().unwrap();
    assert!(!restarted.next2.has_backend_slot("a-broken"));
    assert!(!restarted.next2.has_backend_slot("on-demand"));
    assert!(restarted.next2.has_backend_slot("z-dependency"));
    assert!(restarted.next2.has_backend_slot("healthy"));
    assert_eq!(
        restarted.next2.execute_command("healthy", "ping").unwrap(),
        json!("restored")
    );
    let logs = db.plugin_logs(Some("a-broken"), Some("error"), 1).unwrap();
    assert!(logs[0].message.contains("Background activation failed"));
    let wait = |predicate: &dyn Fn() -> bool| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while !predicate() {
            assert!(
                std::time::Instant::now() < deadline,
                "supervisor state timeout"
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    };
    wait(&|| db.storage_get("healthy", "heartbeat").unwrap().is_some());
    db.storage_set("healthy", "fail-tick", "true").unwrap();
    wait(&|| {
        !restarted.next2.has_backend_slot("healthy")
            && !db
                .plugin_logs(Some("healthy"), Some("error"), 1)
                .unwrap()
                .is_empty()
    });
    assert!(db.plugin_find_by_id("healthy").unwrap().unwrap().enabled);
    assert!(restarted.next2.has_backend_slot("z-dependency"));
    db.storage_set("healthy", "fail-tick", "false").unwrap();
    assert_eq!(
        restarted.next2.execute_command("healthy", "ping").unwrap(),
        json!("restored")
    );
    restarted.shutdown().unwrap();
    assert!(!restarted.next2.has_backend_slot("healthy"));
    assert!(!restarted.next2.has_backend_slot("z-dependency"));
}

#[test]
fn background_service_survives_two_headless_host_process_starts() {
    const PARENT: &str = "CRUCIBLEBOX_P2_RESTART_FIXTURE_PARENT";
    if let Ok(parent) = std::env::var(PARENT) {
        let parent: u32 = parent.parse().unwrap();
        let path = std::env::temp_dir().join(format!(
            "cruciblebox-install-test-next2-host-process-restart-{parent}/data/openbox.db"
        ));
        let db = crate::db::Db::open(&path).unwrap();
        let backend =
            crate::next_backend::Manager::new(std::sync::Arc::new(std::sync::Mutex::new(db)));
        backend.next2.restore_background().unwrap();
        assert!(backend.next2.has_backend_slot("acme-independent"));
        backend
            .next2
            .execute_command("acme-independent", "increment")
            .unwrap();
        backend.shutdown().unwrap();
        assert!(!backend.next2.has_backend_slot("acme-independent"));
        return;
    }
    let (manager, _, root) = setup("next2-host-process-restart");
    let source = root.join("source");
    write_next2(
        &source,
        "1.0.0",
        Some(
            r#"module.exports.activate = ctx => ({ increment: async () => {
  const call = async (requestId, method, params) => {
    const reply = await ctx.exchange({wireVersion:ctx.wireVersion,requestId,session:ctx.session,target:{kind:'platform'},method,params});
    if (!reply.ok) throw Error(reply.error.code);
    return reply.result;
  };
  const count = (await call('r1','storage.get',{key:'boots'})) || 0;
  await call('r2','storage.set',{key:'boots',value:count+1});
  return count+1;
} });"#,
        ),
    );
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(source.join("plugin.json")).unwrap()).unwrap();
    manifest["background"] = json!(true);
    manifest["contributes"]["commands"] =
        json!([{"id":"increment","title":"Increment","method":"increment"}]);
    fs::write(source.join("plugin.json"), manifest.to_string()).unwrap();
    manager.remember_trusted_path(source.clone());
    let preview = manager.preview(directory(&source)).unwrap();
    manager
        .commit_approved(
            preview["installToken"].as_str().unwrap().into(),
            Some(next2_approval(&preview)),
        )
        .unwrap();
    manager
        .next_backend
        .next2
        .enable("acme-independent")
        .unwrap();
    manager.next_backend.shutdown().unwrap();
    drop(manager);
    let path = root.join("data/openbox.db");
    for _ in 0..2 {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "install::next2_tests::background_service_survives_two_headless_host_process_starts", "--nocapture"])
            .env(PARENT, std::process::id().to_string()).output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    }
    let db = crate::db::Db::open(&path).unwrap();
    assert_eq!(
        db.storage_get("acme-independent", "boots")
            .unwrap()
            .as_deref(),
        Some("2")
    );
}

#[test]
fn next2_uninstall_removes_all_code_and_resources_but_restores_config_and_kv_by_manifest_identity()
{
    let (manager, plugins, root) = setup("next2-uninstall-managed-code");
    let source = root.join("source");
    write_next2(&source, "1.0.0", None);
    manager.remember_trusted_path(source.clone());
    let preview = manager.preview(directory(&source)).unwrap();
    manager
        .commit_approved(
            preview["installToken"].as_str().unwrap().into(),
            Some(next2_approval(&preview)),
        )
        .unwrap();
    write_next2(&source, "1.1.0", None);
    let preview = manager.preview(directory(&source)).unwrap();
    manager
        .commit_approved(
            preview["installToken"].as_str().unwrap().into(),
            Some(next2_approval(&preview)),
        )
        .unwrap();
    let owner = "acme-independent";
    let config = r#"{"setting":"preserved"}"#;
    lock(&manager.db)
        .plugin_update_config(owner, config)
        .unwrap();
    // Historical records can have a different installation ID from the manifest ID.
    lock(&manager.db)
        .conn()
        .lock()
        .unwrap()
        .execute(
            "UPDATE plugins SET id = ?1 WHERE id = ?2",
            ("retained-record-id", owner),
        )
        .unwrap();
    lock(&manager.db)
        .storage_set("retained-record-id", "sentinel", "原始值")
        .unwrap();
    let target = plugins.join(owner);
    fs::write(target.join("user-note.txt"), "笔记").unwrap();
    let resources = root.join("plugin-resources").join(owner);
    fs::create_dir_all(&resources).unwrap();
    fs::write(resources.join("model.bin"), "exclusive resource").unwrap();
    let code = target.join(cruciblebox_next2_protocol::CODE_DIRECTORY);
    fs::create_dir_all(code.join("old-version")).unwrap();
    fs::write(code.join("old-version/old.js"), "retired code").unwrap();
    let result = manager.uninstall("retained-record-id").unwrap();
    assert!(!target.exists());
    assert!(!resources.exists());
    let retained = Path::new(result["data"]["retainedFiles"][0].as_str().unwrap());
    assert_eq!(
        fs::read_to_string(retained.join("user-note.txt")).unwrap(),
        "笔记"
    );
    assert!(!retained
        .join(cruciblebox_next2_protocol::CODE_DIRECTORY)
        .exists());
    assert!(!retained.join("plugin.json").exists());
    for entry in fs::read_dir(&plugins).unwrap() {
        assert!(!entry
            .unwrap()
            .path()
            .join(cruciblebox_next2_protocol::CODE_DIRECTORY)
            .exists());
    }
    let preview = manager.preview(directory(&source)).unwrap();
    manager
        .commit_approved(
            preview["installToken"].as_str().unwrap().into(),
            Some(next2_approval(&preview)),
        )
        .unwrap();
    let row = lock(&manager.db)
        .plugin_find_by_name(owner)
        .unwrap()
        .unwrap();
    assert_eq!(row.config_data, config);
    assert_eq!(
        lock(&manager.db)
            .storage_get(&row.id, "sentinel")
            .unwrap()
            .as_deref(),
        Some("原始值")
    );
}

#[test]
fn next2_uninstall_retries_cleanup_from_committed_journal_after_record_deletion() {
    use crate::{
        journal::{write_journal, Journal, JOURNAL_VERSION},
        transaction::RemovalTransaction,
    };
    let (manager, plugins, root) = setup("next2-uninstall-recovery");
    let source = root.join("source");
    write_next2(&source, "1.0.0", None);
    manager.remember_trusted_path(source.clone());
    let preview = manager.preview(directory(&source)).unwrap();
    manager
        .commit_approved(
            preview["installToken"].as_str().unwrap().into(),
            Some(next2_approval(&preview)),
        )
        .unwrap();
    let owner = "acme-independent";
    let row = lock(&manager.db).plugin_find_by_id(owner).unwrap().unwrap();
    let mut metadata = super::plugin_row_to_metadata(&row);
    metadata["retainPreviousFiles"] = json!(true);
    metadata["purgeManagedCode"] = json!(true);
    let journal = Journal {
        version: JOURNAL_VERSION,
        operation: "uninstall".into(),
        phase: "committed".into(),
        plugin_name: owner.into(),
        transaction_id: "cleanup-retry".into(),
        previous_metadata: Some(metadata),
        created_at: super::now_iso(),
    };
    let mut transaction = RemovalTransaction::new(
        plugins.clone(),
        owner.into(),
        journal.transaction_id.clone(),
    )
    .unwrap();
    transaction.quarantine().unwrap();
    write_journal(transaction.quarantine_dir(), &journal).unwrap();
    lock(&manager.db).plugin_delete(owner).unwrap();
    let retained = transaction.commit_retaining_files().unwrap();
    // Cleanup is interrupted after the DB commit; retry must still find this exact installation.
    assert!(retained
        .join(cruciblebox_next2_protocol::CODE_DIRECTORY)
        .exists());
    assert_eq!(
        manager.uninstall(owner).unwrap()["data"]["cleanupResumed"],
        true
    );
    assert!(!manager.is_blocked(owner));
    assert!(!retained.exists());
}

#[test]
fn next3_upgrades_a_retained_manifest6_record_and_preserves_data() {
    let (manager, plugins, root) = setup("next3-compatibility-upgrade");
    let target = plugins.join("acme-independent");
    let old_code = target
        .join(cruciblebox_next2_protocol::CODE_DIRECTORY)
        .join("1.0.0");
    write_next2(&old_code, "1.0.0", None);
    let mut old: Value =
        serde_json::from_slice(&fs::read(old_code.join("plugin.json")).unwrap()).unwrap();
    old["manifestVersion"] = json!(6);
    old["sdkApiVersion"] = json!(6);
    old["sdk"] = json!({"min":6,"maxExclusive":7});
    old["minHostVersion"] = json!("2.1.1");
    fs::write(old_code.join("plugin.json"), old.to_string()).unwrap();
    fs::write(target.join("plugin.json"), old.to_string()).unwrap();
    let id = "retained-record-id";
    lock(&manager.db).conn().lock().unwrap().execute(
        "INSERT INTO plugins(id,name,version,display_name,entry_main,entry_renderer,installed_path,enabled,config_data) VALUES (?1,'acme-independent','1.0.0','Previous plugin','','dist/renderer.js',?2,0,?3)",
        (id, old_code.to_string_lossy().as_ref(), r#"{"retained":7}"#),
    ).unwrap();
    lock(&manager.db)
        .storage_set(id, "preserved", "原始数据")
        .unwrap();
    let data = root.join("plugin-data/acme-independent");
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join("user-note.txt"), "保留文件").unwrap();
    manager.remember_trusted_path(old_code.clone());
    let rejection = manager.preview(directory(&old_code)).unwrap_err();
    assert!(rejection.contains("LEGACY_RUNTIME_RETIRED"), "{rejection}");

    let source = root.join("source");
    write_next2(&source, "1.1.0", None);
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(source.join("plugin.json")).unwrap()).unwrap();
    let migration = json!({"fromPluginVersion":{"min":"1.0.0","maxExclusive":"1.1.0"},"summary":"Preserve existing settings and files."});
    manifest["dataMigration"] = migration.clone();
    fs::write(source.join("plugin.json"), manifest.to_string()).unwrap();
    manager.remember_trusted_path(source.clone());
    let preview = manager.preview(directory(&source)).unwrap();
    assert_eq!(preview["data"]["previousVersion"], "1.0.0");
    assert_eq!(preview["data"]["next2"]["dataMigration"], migration);
    manager
        .commit_approved(
            preview["installToken"].as_str().unwrap().into(),
            Some(next2_approval(&preview)),
        )
        .unwrap();
    let db = lock(&manager.db);
    let upgraded = db.plugin_find_by_id(id).unwrap().unwrap();
    assert_eq!(upgraded.version, "1.1.0");
    assert_eq!(upgraded.config_data, r#"{"retained":7}"#);
    assert_eq!(
        db.storage_get(id, "preserved").unwrap().as_deref(),
        Some("原始数据")
    );
    assert_eq!(
        fs::read_to_string(data.join("user-note.txt")).unwrap(),
        "保留文件"
    );
    assert!(crate::next2_install::load(&db, Path::new(&upgraded.installed_path)).is_ok());
}

#[test]
fn next2_versioned_code_power_cut_restores_paired_metadata_and_data() {
    use super::{manifest_to_version_fields, now_iso, plugin_row_to_metadata};
    use crate::journal::{write_journal, Journal, JOURNAL_VERSION};
    let (manager, plugins, root) = setup("next2-versioned-recovery");
    let source = root.join("source");
    write_next2(&source, "1.0.0", None);
    manager.remember_trusted_path(source.clone());
    let preview = manager.preview(directory(&source)).unwrap();
    manager
        .commit_approved(
            preview["installToken"].as_str().unwrap().into(),
            Some(next2_approval(&preview)),
        )
        .unwrap();
    lock(&manager.db)
        .storage_set("acme-independent", "preserved", "原始数据")
        .unwrap();
    let old = lock(&manager.db)
        .plugin_find_by_name("acme-independent")
        .unwrap()
        .unwrap();
    write_next2(&source, "1.1.0", None);
    let preview = manager.preview(directory(&source)).unwrap();
    let mut pending = lock(&manager.prepared)
        .remove(preview["installToken"].as_str().unwrap())
        .unwrap();
    crate::next2_install::save(
        &lock(&manager.db),
        pending.transaction.stage_dir(),
        next2_approval(&preview),
    )
    .unwrap();
    crate::next2_install::stage_layout(pending.transaction.stage_dir(), &pending.manifest).unwrap();
    let mut journal = Journal {
        version: JOURNAL_VERSION,
        operation: "upgrade".into(),
        phase: "prepared".into(),
        plugin_name: "acme-independent".into(),
        transaction_id: pending.transaction_id.clone(),
        previous_metadata: Some(plugin_row_to_metadata(&old)),
        created_at: now_iso(),
    };
    write_journal(pending.transaction.stage_dir(), &journal).unwrap();
    pending.transaction.swap().unwrap();
    let target = plugins.join("acme-independent");
    journal.phase = "applied".into();
    write_journal(&target, &journal).unwrap();
    let code = crate::next2_install::code_directory(&target, &pending.manifest);
    lock(&manager.db)
        .plugin_update_version(
            "acme-independent",
            &manifest_to_version_fields(&pending.manifest, &code),
        )
        .unwrap();
    // Simulate a stopped process after directory/metadata publication, before commit.
    drop(pending);
    manager.run_recovery_pass();
    assert!(!manager.is_blocked("acme-independent"));
    let restored = lock(&manager.db)
        .plugin_find_by_name("acme-independent")
        .unwrap()
        .unwrap();
    assert_eq!(restored.version, "1.0.0");
    assert_eq!(restored.installed_path, old.installed_path);
    assert!(
        crate::next2_install::load(&lock(&manager.db), Path::new(&restored.installed_path)).is_ok()
    );
    assert_eq!(
        lock(&manager.db)
            .storage_get("acme-independent", "preserved")
            .unwrap()
            .as_deref(),
        Some("原始数据")
    );
}

#[test]
#[ignore = "requires CRUCIBLEBOX_LIVE_PROXY and official HTTPS release assets"]
fn next2_user_selected_resource_download_verifies_failure_and_cancellation() {
    let (manager, _, root) = setup("next2-resource-download");
    let proxy = std::env::var("CRUCIBLEBOX_LIVE_PROXY").expect("explicit network route required");
    lock(&manager.db)
        .download_proxy_set("auto", &proxy)
        .unwrap();
    crate::network_policy::reload(&manager.db);
    let source = root.join("source");
    write_next2(&source, "1.0.0", None);
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(source.join("plugin.json")).unwrap()).unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("../../contracts/next2/fixtures.json")).unwrap();
    let mut resource = fixture["resource"].clone();
    resource["kind"] = json!("runtime");
    resource["license"] = json!("Transport acceptance fixture; not an inference model");
    resource["files"] = json!([{"path":"fixtures/official-theme.zip","bytes":71344,"sha256":"6fc13c60bda789d74b721c4574adf34c826ddefdf60de70f37061210f7db340f"}]);
    let url = "https://github.com/Xuancheng75/CrucibleBox/releases/download/tauri-v2.1.0-beta.4/sample-zeta-0.4.0-alpha.1.zip";
    let mut releases = Vec::new();
    for (id, version) in [
        ("valid", "1.0.0"),
        ("damaged", "1.0.1"),
        ("cancelled", "1.0.2"),
    ] {
        let mut resource = resource.clone();
        resource["version"] = json!(version);
        if id == "damaged" {
            resource["files"][0]["sha256"] = json!("0".repeat(64));
        }
        releases.push(json!({"id":id,"title":id,"description":"Actual HTTPS transfer acceptance, no model inference","manifest":resource,"downloads":[{"path":"fixtures/official-theme.zip","url":url}]}));
    }
    let catalog = json!({"resourceCatalogVersion":1,"releases":releases}).to_string();
    fs::create_dir(source.join("resources")).unwrap();
    fs::write(source.join("resources/catalog.json"), &catalog).unwrap();
    manifest["files"].as_array_mut().unwrap().push(json!({"path":"resources/catalog.json","bytes":catalog.len(),"sha256":format!("{:x}",Sha256::digest(catalog.as_bytes()))}));
    manifest["resources"] = json!([{"id":"translation-model","version":{"min":"1.0.0","maxExclusive":"2.0.0"},"optional":true,"catalog":"resources/catalog.json"}]);
    fs::write(source.join("plugin.json"), manifest.to_string()).unwrap();
    manager.remember_trusted_path(source.clone());
    let preview = manager.preview(directory(&source)).unwrap();
    manager
        .commit_approved(
            preview["installToken"].as_str().unwrap().into(),
            Some(next2_approval(&preview)),
        )
        .unwrap();
    let options = manager.resources.list("acme-independent").unwrap()[0]["available"]
        .as_array()
        .unwrap()
        .clone();
    let runtime = manager.task_runtime();
    for id in ["valid", "damaged", "cancelled"] {
        let release = options
            .iter()
            .find(|option| option["release"]["id"] == id)
            .unwrap();
        let approval = Approval {
            manifest_sha256: serde_json::from_value(release["manifestSha256"].clone()).unwrap(),
            accept_native_code: false,
            accept_unsigned_local: true,
            accept_permissions: true,
        };
        let task = crate::next2_resources::Manager::start_download(
            manager.clone(),
            "acme-independent".into(),
            "translation-model".into(),
            id.into(),
            approval,
        )
        .unwrap();
        if id == "cancelled" {
            assert!(runtime.cancel("plugin-install", &task));
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let result = loop {
            let result = runtime.get("plugin-install", &task).unwrap();
            if matches!(
                result["status"].as_str(),
                Some("succeeded" | "failed" | "cancelled")
            ) {
                break result;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "resource task timeout: {result}"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert_eq!(
            result["status"],
            if id == "valid" {
                "succeeded"
            } else if id == "damaged" {
                "failed"
            } else {
                "cancelled"
            },
            "{result}"
        );
        if id == "damaged" {
            assert!(result["error"]["message"]
                .as_str()
                .unwrap()
                .contains("RESOURCE_INTEGRITY_MISMATCH"));
        }
    }
    let installed = manager.resources.list("acme-independent").unwrap()[0]["installed"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0]["version"], "1.0.0");
    fs::write(Path::new(env!("CARGO_MANIFEST_DIR")).join("../artifacts/next2/p2/resource-download-evidence.json"), json!({"liveSource":url,"bytes":71344,"download":"passed","wrongDigest":"rejected; old version retained","cancel":"confirmed","modelInferenceTested":false}).to_string()).unwrap();
}

#[test]
fn next2_service_binds_caller_storage_results_and_rejects_breaking_upgrade() {
    let (manager, plugins, root) = setup("next2-service-graph");
    let provider = root.join("provider");
    write_next2(
        &provider,
        "1.0.0",
        Some(
            r#"let requests = 0;
module.exports.activate = ctx => ({ identity: async (params, execution) => {
  if (params.badResult) return 'not the declared object';
  let response;
  for (let i = 0; i < (params.burst ? 4104 : 1); i++) {
    const requestId = 'provider-value-' + (++requests);
    response = await ctx.exchange({wireVersion:ctx.wireVersion,requestId,session:ctx.session,target:{kind:'platform'},method:'storage.get',params:{key:'identity'}});
    if (!response.ok || response.requestId !== requestId) throw Error('Capability or correlation failed');
  }
  const logged = await ctx.exchange({wireVersion:ctx.wireVersion,requestId:'provider-log-' + (++requests),session:ctx.session,target:{kind:'platform'},method:'logging.write',params:{level:'info',message:'Sample capability executed'}});
  if (!logged.ok || logged.result !== true) throw Error('Logging failed');
  return {caller:execution.callerId, value:response.result};
} });"#,
        ),
    );
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(provider.join("plugin.json")).unwrap()).unwrap();
    manifest["id"] = json!("service-provider");
    manifest.as_object_mut().unwrap().remove("renderer");
    manifest["contributes"]
        .as_object_mut()
        .unwrap()
        .remove("navigation");
    manifest["files"]
        .as_array_mut()
        .unwrap()
        .retain(|file| file["path"] != "dist/renderer.js");
    fs::remove_file(provider.join("dist/renderer.js")).unwrap();
    manifest["permissions"]
        .as_array_mut()
        .unwrap()
        .push(json!("logging:write"));
    manifest["exports"] = json!([{"id":"sample-service","version":"1.0.0","interchangeable":true,"methods":[{
        "name":"identity", "contexts":["renderer"],
        "params":{"type":"object","properties":{"owner":{"type":"string"},"burst":{"type":"boolean"},"badResult":{"type":"boolean"}},"additionalProperties":false},
        "result":{"type":"object","properties":{"caller":{"type":"string"},"value":{"type":"string"}},"required":["caller","value"],"additionalProperties":false}
    }]}]);
    manifest["contributes"]["commands"] =
        json!([{"id":"show-value","title":"Show value","method":"identity"}]);
    fs::write(provider.join("plugin.json"), manifest.to_string()).unwrap();
    manager.remember_trusted_path(provider.clone());
    let install = |path: &Path| {
        let preview = manager.preview(directory(path)).unwrap();
        manager.commit_approved(
            preview["installToken"].as_str().unwrap().into(),
            Some(next2_approval(&preview)),
        )
    };
    install(&provider).unwrap();
    let provider_id = "retained-provider-guid";
    lock(&manager.db)
        .conn()
        .lock()
        .unwrap()
        .execute(
            "UPDATE plugins SET id=?1 WHERE name='service-provider'",
            [provider_id],
        )
        .unwrap();
    lock(&manager.db)
        .set_plugin_enabled(provider_id, true)
        .unwrap();
    lock(&manager.db)
        .storage_set(provider_id, "identity", "\"provider data\"")
        .unwrap();
    manager.next_backend.activate(provider_id).unwrap();
    assert_eq!(
        manager
            .next_backend
            .next2
            .execute_command(provider_id, "show-value")
            .unwrap(),
        json!({"caller":"service-provider","value":"provider data"})
    );
    assert_eq!(
        manager
            .next_backend
            .next2
            .execute_command(provider_id, "missing")
            .unwrap_err(),
        "COMMAND_NOT_DECLARED"
    );
    let client = root.join("client");
    write_next2(&client, "1.0.0", None);
    let mut caller: Value =
        serde_json::from_slice(&fs::read(client.join("plugin.json")).unwrap()).unwrap();
    caller["dependencies"] =
        json!([{"id":"sample-service","version":{"min":"1.0.0","maxExclusive":"2.0.0"}}]);
    fs::write(client.join("plugin.json"), caller.to_string()).unwrap();
    manager.remember_trusted_path(client.clone());
    install(&client).unwrap();
    lock(&manager.db)
        .set_plugin_enabled("acme-independent", true)
        .unwrap();
    lock(&manager.db)
        .storage_set("acme-independent", "identity", "\"caller secret\"")
        .unwrap();
    manager.next_backend.activate("acme-independent").unwrap();
    let unrelated = root.join("unrelated");
    write_next2(&unrelated, "1.0.0", None);
    let mut unrelated_manifest: Value =
        serde_json::from_slice(&fs::read(unrelated.join("plugin.json")).unwrap()).unwrap();
    unrelated_manifest["id"] = json!("unrelated-plugin");
    fs::write(
        unrelated.join("plugin.json"),
        unrelated_manifest.to_string(),
    )
    .unwrap();
    manager.remember_trusted_path(unrelated.clone());
    install(&unrelated).unwrap();
    manager
        .next_backend
        .next2
        .enable("unrelated-plugin")
        .unwrap();
    let unrelated_path = lock(&manager.db)
        .plugin_find_by_name("unrelated-plugin")
        .unwrap()
        .unwrap()
        .installed_path;
    fs::write(
        Path::new(&unrelated_path).join("plugin.json"),
        "broken manifest",
    )
    .unwrap();

    let session = next2_session(&manager, &plugins);
    let discovery = json!({"wireVersion":cruciblebox_next2_protocol::WIRE_VERSION,"requestId":"renderer-1","session":session.token,"target":{"kind":"platform"},"method":"capabilities.resolve","params":{"id":"sample-service"}});
    let binding = manager
        .next_backend
        .next2
        .renderer(&discovery.to_string(), &session)
        .unwrap();
    assert_eq!(
        binding["result"],
        json!({"providerId":"service-provider","providerVersion":"1.0.0","lifecycle":"on-demand","export":manifest["exports"][0]})
    );
    let request = json!({"wireVersion":cruciblebox_next2_protocol::WIRE_VERSION,"requestId":"renderer-2","session":session.token,"target":{"kind":"service","id":"sample-service"},"method":"identity","params":{"owner":"forged"}});
    let result = manager
        .next_backend
        .next2
        .renderer(&request.to_string(), &session)
        .unwrap();
    assert_eq!(
        result["result"],
        json!({"caller":"acme-independent","value":"provider data"})
    );
    let start = json!({"wireVersion":cruciblebox_next2_protocol::WIRE_VERSION,"requestId":"renderer-3","session":session.token,"target":{"kind":"platform"},"method":"tasks.start","params":{"target":{"kind":"service","id":"sample-service"},"method":"identity","params":{"burst":true}}});
    let started = manager
        .next_backend
        .next2
        .renderer(&start.to_string(), &session)
        .unwrap();
    let id = started["result"]["taskId"].as_str().unwrap();
    let runtime = manager.task_runtime();
    let wait_for_task = |id: &str| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let result = runtime.get("acme-independent", id).unwrap();
            if matches!(result["status"].as_str(), Some("succeeded" | "failed")) {
                return result;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "task timeout: {result}"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    };
    let result = wait_for_task(id);
    assert_eq!(result["status"], "succeeded", "{result}");
    assert_eq!(
        result["result"],
        json!({"caller":"acme-independent","value":"provider data"})
    );
    assert!(runtime.get(provider_id, id).is_none());
    let db = lock(&manager.db);
    let logs = db.plugin_logs(Some(provider_id), Some("info"), 10).unwrap();
    assert_eq!(logs.len(), 3);
    let log: Value = serde_json::from_str(&logs[0].message).unwrap();
    assert_eq!(log["taskId"], id);
    assert_eq!(log["runtime"], "backend");
    assert!(db
        .plugin_logs(Some("acme-independent"), None, 10)
        .unwrap()
        .is_empty());
    drop(db);
    let mut invalid = request.clone();
    invalid["requestId"] = json!("renderer-4");
    invalid["params"] = json!({"owner":123});
    let rejected = manager
        .next_backend
        .next2
        .renderer(&invalid.to_string(), &session)
        .unwrap();
    assert_eq!(
        rejected["error"]["code"], "INVALID_CAPABILITY_PARAMS",
        "{rejected}"
    );
    let mut invalid_task = start.clone();
    invalid_task["requestId"] = json!("renderer-5");
    invalid_task["params"]["params"] = json!({"burst":123});
    let rejected = manager
        .next_backend
        .next2
        .renderer(&invalid_task.to_string(), &session)
        .unwrap();
    assert_eq!(
        rejected["error"]["code"], "INVALID_CAPABILITY_PARAMS",
        "{rejected}"
    );
    invalid["requestId"] = json!("renderer-6");
    invalid["params"] = json!({"badResult":true});
    let rejected = manager
        .next_backend
        .next2
        .renderer(&invalid.to_string(), &session)
        .unwrap();
    assert_eq!(
        rejected["error"]["code"], "INVALID_CAPABILITY_RESULT",
        "{rejected}"
    );
    invalid_task["requestId"] = json!("renderer-7");
    invalid_task["params"]["params"] = json!({"badResult":true});
    let started = manager
        .next_backend
        .next2
        .renderer(&invalid_task.to_string(), &session)
        .unwrap();
    let failed = wait_for_task(started["result"]["taskId"].as_str().unwrap());
    assert_eq!(failed["status"], "failed", "{failed}");
    assert!(failed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("INVALID_CAPABILITY_RESULT"));
    let provider_path = lock(&manager.db)
        .plugin_find_by_id(provider_id)
        .unwrap()
        .unwrap()
        .installed_path;
    let executable = Path::new(&provider_path).join("dist/main.js");
    let approved_code = fs::read(&executable).unwrap();
    let mut modified_code = approved_code.clone();
    modified_code[0] ^= 1;
    fs::write(&executable, modified_code).unwrap();
    let mut tampered = request;
    tampered["requestId"] = json!("renderer-8");
    let rejected = manager
        .next_backend
        .next2
        .renderer(&tampered.to_string(), &session)
        .unwrap();
    assert_eq!(rejected["ok"], false, "{rejected}");
    assert!(
        rejected["error"]["message"]
            .as_str()
            .unwrap()
            .contains("PACKAGE_DIGEST_MISMATCH"),
        "{rejected}"
    );
    fs::write(executable, approved_code).unwrap();
    assert!(manager
        .next_backend
        .next2
        .assert_can_remove(provider_id)
        .unwrap_err()
        .contains("acme-independent"));
    assert!(manager.uninstall(provider_id).is_err());
    manifest["version"] = json!("1.1.0");
    manifest["exports"][0]["version"] = json!("2.0.0");
    fs::write(provider.join("plugin.json"), manifest.to_string()).unwrap();
    assert!(install(&provider)
        .unwrap_err()
        .contains("DEPENDENCY_INCOMPATIBLE"));
    assert_eq!(
        lock(&manager.db)
            .plugin_find_by_name("service-provider")
            .unwrap()
            .unwrap()
            .version,
        "1.0.0"
    );
    manager.uninstall("acme-independent").unwrap();
    manager.uninstall(provider_id).unwrap();
    manager.next_backend.shutdown().unwrap();
}

#[test]
#[ignore = "requires the independently built CRUCIBLEBOX_NEXT2_TEST_BACKEND executable"]
fn next2_native_installed_backend_runs_cancels_crashes_and_upgrades() {
    let backend = std::env::var("CRUCIBLEBOX_NEXT2_TEST_BACKEND").expect("native fixture required");
    let (manager, plugins, root) = setup("next2-native-installed");
    let source = root.join("source");
    write_next2(&source, "1.0.0", None);
    fs::create_dir(source.join("bin")).unwrap();
    fs::copy(backend, source.join("bin/backend.exe")).unwrap();
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(source.join("plugin.json")).unwrap()).unwrap();
    manifest["backend"] = json!({"kind":"native","entry":"bin/backend.exe","os":"windows","arch":"x86_64","trust":"trusted-local-code"});
    let bytes = fs::read(source.join("bin/backend.exe")).unwrap();
    manifest["files"].as_array_mut().unwrap().push(json!({"path":"bin/backend.exe","bytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(bytes))}));
    fs::write(source.join("plugin.json"), manifest.to_string()).unwrap();
    manager.remember_trusted_path(source.clone());
    let preview = manager.preview(directory(&source)).unwrap();
    let mut approval = next2_approval(&preview);
    approval.accept_native_code = true;
    manager
        .commit_approved(
            preview["installToken"].as_str().unwrap().into(),
            Some(approval),
        )
        .unwrap();
    lock(&manager.db)
        .set_plugin_enabled("acme-independent", true)
        .unwrap();
    manager.next_backend.activate("acme-independent").unwrap();
    let session = next2_session(&manager, &plugins);
    let sequence = std::cell::Cell::new(0u64);
    let call = |id: &str, target, method, params| {
        sequence.set(sequence.get() + 1);
        let mut response=manager.next_backend.next2.renderer(&json!({"wireVersion":cruciblebox_next2_protocol::WIRE_VERSION,"requestId":format!("renderer-{}",sequence.get()),"session":session.token,"target":target,"method":method,"params":params}).to_string(), &session).unwrap();
        response["requestId"] = json!(id);
        response
    };
    assert_eq!(
        call(
            "sum",
            json!({"kind":"backend"}),
            "sum",
            json!({"numbers":[11,7,-3]})
        )["result"],
        15
    );
    sequence.set(sequence.get() + 1);
    let request_id = format!("renderer-{}", sequence.get());
    let raw = json!({
        "wireVersion":cruciblebox_next2_protocol::WIRE_VERSION, "requestId":request_id, "session":session.token,
        "target":{"kind":"backend"}, "method":"wait", "params":{}
    })
    .to_string();
    std::thread::scope(|scope| {
        let host = &manager.next_backend.next2;
        let pending = scope.spawn(|| host.renderer(&raw, &session).unwrap());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while lock(&manager.db)
            .storage_get("acme-independent", "native-rpc-started")
            .unwrap()
            .is_none()
        {
            assert!(
                std::time::Instant::now() < deadline,
                "native request never started"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            call(
                "abort",
                json!({"kind":"platform"}),
                "rpc.cancel",
                json!({"requestId":request_id})
            )["result"],
            true
        );
        assert_eq!(pending.join().unwrap()["error"]["code"], "CANCELLED");
    });
    let burst = call(
        "burst",
        json!({"kind":"platform"}),
        "tasks.start",
        json!({"target":{"kind":"backend"},"method":"burst","params":{}}),
    );
    let burst_id = burst["result"]["taskId"].as_str().unwrap();
    let runtime = manager.task_runtime();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
    while std::time::Instant::now() < deadline {
        if matches!(
            runtime.get("acme-independent", burst_id).unwrap()["status"].as_str(),
            Some("succeeded" | "failed")
        ) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let burst_result = runtime.get("acme-independent", burst_id).unwrap();
    assert_eq!(burst_result["status"], "succeeded", "{burst_result}");
    assert_eq!(burst_result["result"], 4104);
    let task = call(
        "start",
        json!({"kind":"platform"}),
        "tasks.start",
        json!({"target":{"kind":"backend"},"method":"wait-cancel","params":{}}),
    );
    let task_id = task["result"]["taskId"].as_str().unwrap();
    let runtime = manager.task_runtime();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while std::time::Instant::now() < deadline {
        if runtime.get("acme-independent", task_id).unwrap()["progress"]["percent"] == 25 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let waiting = runtime.get("acme-independent", task_id).unwrap();
    assert_eq!(waiting["progress"]["percent"], 25, "{waiting}");
    call(
        "cancel",
        json!({"kind":"platform"}),
        "tasks.cancel",
        json!({"taskId":task_id}),
    );
    for _ in 0..700 {
        if runtime.get("acme-independent", task_id).unwrap()["status"] == "cancelled" {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        runtime.get("acme-independent", task_id).unwrap()["status"],
        "cancelled"
    );
    assert_eq!(
        call("crash", json!({"kind":"backend"}), "crash", json!({}))["ok"],
        false
    );
    assert_eq!(
        call(
            "alive",
            json!({"kind":"platform"}),
            "storage.get",
            json!({"key":"none"})
        )["ok"],
        true
    );
    manifest["version"] = json!("1.1.0");
    fs::write(source.join("plugin.json"), manifest.to_string()).unwrap();
    let preview = manager.preview(directory(&source)).unwrap();
    let mut approval = next2_approval(&preview);
    approval.accept_native_code = true;
    manager
        .commit_approved(
            preview["installToken"].as_str().unwrap().into(),
            Some(approval),
        )
        .unwrap();
    assert_eq!(
        call(
            "old-session-after-upgrade",
            json!({"kind":"platform"}),
            "storage.get",
            json!({"key":"none"})
        )["error"]["code"],
        "SESSION_REPLACED"
    );
    manager.uninstall("acme-independent").unwrap();
    manager.next_backend.shutdown().unwrap();
}

#[test]
fn next2_resource_zip_review_import_and_remove_preserves_code_and_data() {
    let (manager, _, root) = setup("next2-resource-install");
    let source = root.join("source");
    write_next2(&source, "1.0.0", None);
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(source.join("plugin.json")).unwrap()).unwrap();
    manifest["resources"] = json!([{"id":"translation-model","version":{"min":"1.0.0","maxExclusive":"2.0.0"},"optional":true}]);
    fs::write(source.join("plugin.json"), manifest.to_string()).unwrap();
    manager.remember_trusted_path(source.clone());
    let preview = manager.preview(directory(&source)).unwrap();
    manager
        .commit_approved(
            preview["installToken"].as_str().unwrap().into(),
            Some(next2_approval(&preview)),
        )
        .unwrap();
    let row = lock(&manager.db)
        .plugin_find_by_id("acme-independent")
        .unwrap()
        .unwrap();
    assert!(row.installed_path.contains(".cruciblebox-code"));
    let before = fs::read(Path::new(&row.installed_path).join("plugin.json")).unwrap();
    lock(&manager.db)
        .storage_set("acme-independent", "preserve", "原值")
        .unwrap();
    let data = b"resource ZIP integrity fixture, not an inference model";
    let fixture: Value =
        serde_json::from_str(include_str!("../../contracts/next2/fixtures.json")).unwrap();
    let mut resource = fixture["resource"].clone();
    resource["files"] = json!([{"path":"model/model.bin","bytes":data.len(),"sha256":format!("{:x}",Sha256::digest(data))}]);
    let zip_path = root.join("resource.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
    for (name, bytes) in [
        ("resource.json", resource.to_string().into_bytes()),
        ("model/model.bin", data.to_vec()),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
    manager.remember_trusted_path(zip_path.clone());
    let preview = manager
        .resources
        .preview(
            &manager,
            "acme-independent",
            InstallSource {
                source_type: "zip".into(),
                path: zip_path.to_string_lossy().into(),
            },
        )
        .unwrap();
    let mut approval = Approval {
        manifest_sha256: serde_json::from_value(preview["manifestSha256"].clone()).unwrap(),
        accept_native_code: false,
        accept_unsigned_local: false,
        accept_permissions: true,
    };
    assert!(manager
        .resources
        .commit(preview["token"].as_str().unwrap(), approval.clone())
        .is_err());
    let preview = manager
        .resources
        .preview(
            &manager,
            "acme-independent",
            InstallSource {
                source_type: "zip".into(),
                path: zip_path.to_string_lossy().into(),
            },
        )
        .unwrap();
    approval.accept_unsigned_local = true;
    let mounted = manager
        .resources
        .commit(preview["token"].as_str().unwrap(), approval)
        .unwrap();
    assert_eq!(
        fs::read(Path::new(mounted["directory"].as_str().unwrap()).join("model/model.bin"))
            .unwrap(),
        data
    );
    assert_eq!(
        manager.resources.list("acme-independent").unwrap()[0]["installed"][0]["version"],
        "1.0.0"
    );
    manager
        .resources
        .remove("acme-independent", "translation-model", "1.0.0")
        .unwrap();
    assert_eq!(
        manager.resources.list("acme-independent").unwrap()[0]["installed"],
        json!([])
    );
    assert_eq!(
        fs::read(Path::new(&row.installed_path).join("plugin.json")).unwrap(),
        before
    );
    assert_eq!(
        lock(&manager.db)
            .storage_get("acme-independent", "preserve")
            .unwrap()
            .as_deref(),
        Some("原值")
    );
}
