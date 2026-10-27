use super::{lock, tests::*, InstallSource};
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
            r#"let requests = 0; module.exports.activate = ctx => ({ identity: async (_, execution) => { const response = await ctx.exchange({wireVersion:4,requestId:'provider-value-'+(++requests),session:ctx.session,target:{kind:'platform'},method:'storage.get',params:{key:'identity'}}); return {caller:execution.callerId, value:response.result}; } });"#,
        ),
    );
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(provider.join("plugin.json")).unwrap()).unwrap();
    manifest["id"] = json!("service-provider");
    manifest["exports"] = json!([{"id":"sample-service","version":"1.0.0","methods":["identity"]}]);
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
    caller["dependencies"] = json!([{"id":"sample-service","providerId":"service-provider","version":{"min":"1.0.0","maxExclusive":"2.0.0"}}]);
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
    let session = next2_session(&manager, &plugins);
    let request = json!({"wireVersion":4,"requestId":"service-call","session":session.token,"target":{"kind":"service","id":"sample-service"},"method":"identity","params":{"owner":"forged"}});
    let result = manager
        .next_backend
        .next2
        .renderer(&request.to_string(), &session)
        .unwrap();
    assert_eq!(
        result["result"],
        json!({"caller":"acme-independent","value":"provider data"})
    );
    let start = json!({"wireVersion":4,"requestId":"service-task","session":session.token,"target":{"kind":"platform"},"method":"tasks.start","params":{"target":{"kind":"service","id":"sample-service"},"method":"identity","params":{}}});
    let started = manager
        .next_backend
        .next2
        .renderer(&start.to_string(), &session)
        .unwrap();
    let id = started["result"]["taskId"].as_str().unwrap();
    let runtime = manager.task_runtime();
    for _ in 0..500 {
        if runtime.get("acme-independent", id).unwrap()["status"] == "succeeded" {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let result = runtime.get("acme-independent", id).unwrap();
    assert_eq!(result["status"], "succeeded");
    assert_eq!(
        result["result"],
        json!({"caller":"acme-independent","value":"provider data"})
    );
    assert!(runtime.get(provider_id, id).is_none());
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
    let call = |id: &str, target, method, params| {
        manager.next_backend.next2.renderer(&json!({"wireVersion":4,"requestId":id,"session":session.token,"target":target,"method":method,"params":params}).to_string(), &session).unwrap()
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
    let task = call(
        "start",
        json!({"kind":"platform"}),
        "tasks.start",
        json!({"target":{"kind":"backend"},"method":"wait-cancel","params":{}}),
    );
    let task_id = task["result"]["taskId"].as_str().unwrap();
    let runtime = manager.task_runtime();
    for _ in 0..300 {
        if runtime.get("acme-independent", task_id).unwrap()["progress"]["percent"] == 25 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        runtime.get("acme-independent", task_id).unwrap()["progress"]["percent"],
        25
    );
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
