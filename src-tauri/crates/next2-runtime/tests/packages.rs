use base64::Engine;
use cruciblebox_next2_runtime::{package::*, resource::*};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn manifest(root: &Path) -> Value {
    fs::create_dir_all(root.join("dist")).unwrap();
    fs::write(root.join("dist/renderer.js"), b"export {};").unwrap();
    let data = fs::read(root.join("dist/renderer.js")).unwrap();
    let source: Value =
        serde_json::from_str(include_str!("../../../../contracts/next2/fixtures.json")).unwrap();
    let mut manifest = source["manifest"].clone();
    manifest["files"] = json!([{"path":"dist/renderer.js","bytes":data.len(),"sha256":format!("{:x}",Sha256::digest(data))}]);
    manifest["resources"] =
        json!([{"id":"translation-model","version":{"min":"1.0.0","maxExclusive":"2.0.0"}}]);
    fs::write(root.join("plugin.json"), manifest.to_string()).unwrap();
    manifest
}
fn decision(digest: [u8; 32]) -> Approval {
    Approval {
        manifest_sha256: digest,
        accept_native_code: false,
        accept_unsigned_local: true,
        accept_permissions: true,
    }
}
#[test]
fn detached_signature_is_bound_to_real_key_publisher_and_raw_manifest() {
    let root = tempfile::tempdir().unwrap();
    manifest(root.path());
    let key = Ed25519KeyPair::from_seed_unchecked(&[7u8; 32]).unwrap();
    let raw = fs::read(root.path().join("plugin.json")).unwrap();
    let signature = base64::engine::general_purpose::STANDARD.encode(key.sign(&raw));
    fs::write(
        root.path().join("signature.json"),
        json!({"signatureVersion":1,"publisherId":"acme","keyId":"test-key","signature":signature})
            .to_string(),
    )
    .unwrap();
    let keys = [PublisherKey {
        official: false,
        publisher_id: "acme".into(),
        key_id: "test-key".into(),
        public_key: key.public_key().as_ref().try_into().unwrap(),
    }];
    let inspected = inspect_plugin(root.path(), &keys, "2.2.0-alpha.1").unwrap();
    assert!(matches!(
        inspected.provenance(),
        Provenance::Verified { .. }
    ));
    assert!(inspect_plugin(root.path(), &[], "2.2.0-alpha.1").is_err());
    let mut changed: Value = serde_json::from_slice(&raw).unwrap();
    changed["description"] = json!("changed after signing");
    fs::write(root.path().join("plugin.json"), changed.to_string()).unwrap();
    assert!(inspect_plugin(root.path(), &keys, "2.2.0-alpha.1")
        .err()
        .unwrap()
        .contains("SIGNATURE_INVALID"));
    // A bad signature is never downgraded to unsigned local trust.
    let result = inspect_plugin(root.path(), &keys, "2.2.0-alpha.1");
    assert!(result.is_err());
}
fn resource(root: &Path, version: &str) -> Value {
    fs::create_dir_all(root.join("model")).unwrap();
    let data = b"resource integrity fixture; not an inference model";
    fs::write(root.join("model/model.bin"), data).unwrap();
    let source: Value =
        serde_json::from_str(include_str!("../../../../contracts/next2/fixtures.json")).unwrap();
    let mut manifest = source["resource"].clone();
    manifest["version"] = json!(version);
    manifest["files"] = json!([{"path":"model/model.bin","bytes":data.len(),"sha256":format!("{:x}",Sha256::digest(data))}]);
    fs::write(root.join("resource.json"), manifest.to_string()).unwrap();
    manifest
}
#[test]
fn offline_versions_are_published_atomically_and_failed_updates_preserve_previous() {
    let code = tempfile::tempdir().unwrap();
    manifest(code.path());
    let inspected = inspect_plugin(code.path(), &[], "2.2.0-alpha.1").unwrap();
    let digest = inspected.manifest_sha256();
    let approved = inspected.approve(decision(digest)).unwrap();
    let source = tempfile::tempdir().unwrap();
    resource(source.path(), "1.0.0");
    let store = tempfile::tempdir().unwrap();
    let inspected = inspect_resource(source.path(), &[]).unwrap();
    let digest = inspected.manifest_sha256();
    let first =
        import_directory(inspected, store.path(), &approved, decision(digest), &[]).unwrap();
    let first_approval = decision(digest);
    assert!(resolve(store.path(), &approved, &[], &[])
        .unwrap_err()
        .contains("RESOURCE_APPROVAL_REQUIRED"));
    assert_eq!(
        resolve(
            store.path(),
            &approved,
            &[],
            std::slice::from_ref(&first_approval)
        )
        .unwrap()[0]
            .version,
        "1.0.0"
    );
    let original = fs::read(Path::new(&first.directory).join("model/model.bin")).unwrap();
    resource(source.path(), "1.0.1");
    let inspected = inspect_resource(source.path(), &[]).unwrap();
    let digest = inspected.manifest_sha256();
    let second =
        import_directory(inspected, store.path(), &approved, decision(digest), &[]).unwrap();
    let second_approval = decision(digest);
    let approvals = [first_approval.clone(), second_approval];
    assert_eq!(
        resolve(store.path(), &approved, &[], &approvals).unwrap()[0].version,
        "1.0.1"
    );
    // Installing an unapproved newer version never silently selects the older version.
    assert!(resolve(
        store.path(),
        &approved,
        &[],
        std::slice::from_ref(&first_approval)
    )
    .is_err());
    assert_ne!(first.directory, second.directory);
    resource(source.path(), "1.0.2");
    let inspected = inspect_resource(source.path(), &[]).unwrap();
    let digest = inspected.manifest_sha256();
    fs::write(
        source.path().join("model/model.bin"),
        b"modified after inspection",
    )
    .unwrap();
    assert!(import_directory(inspected, store.path(), &approved, decision(digest), &[]).is_err());
    assert_eq!(
        fs::read(Path::new(&first.directory).join("model/model.bin")).unwrap(),
        original
    );
    assert!(!store
        .path()
        .join("acme-independent/translation-model/1.0.2")
        .exists());
    assert!(Path::new(&second.directory).exists());
    fs::write(
        Path::new(&second.directory).join("model/model.bin"),
        b"corrupt selected version",
    )
    .unwrap();
    assert!(resolve(store.path(), &approved, &[], &approvals).is_err());
    assert!(remove(store.path(), &approved, "translation-model", "../outside").is_err());
    remove(store.path(), &approved, "translation-model", "1.0.1").unwrap();
    assert_eq!(
        resolve(store.path(), &approved, &[], &[first_approval]).unwrap()[0].version,
        "1.0.0"
    );
}

#[test]
fn optional_resources_do_not_block_a_lightweight_plugin_before_user_download() {
    let code = tempfile::tempdir().unwrap();
    let mut value = manifest(code.path());
    value["resources"][0]["optional"] = json!(true);
    fs::write(code.path().join("plugin.json"), value.to_string()).unwrap();
    let inspected = inspect_plugin(code.path(), &[], "2.2.0-alpha.1").unwrap();
    let digest = inspected.manifest_sha256();
    let approved = inspected.approve(decision(digest)).unwrap();
    assert!(resolve(
        &code.path().join("absent-resource-store"),
        &approved,
        &[],
        &[]
    )
    .unwrap()
    .is_empty());
}
#[test]
fn code_change_after_approval_and_stale_approval_are_rejected() {
    let code = tempfile::tempdir().unwrap();
    manifest(code.path());
    let inspected = inspect_plugin(code.path(), &[], "2.2.0-alpha.1").unwrap();
    assert!(inspected.approve(decision([0u8; 32])).is_err());
    let inspected = inspect_plugin(code.path(), &[], "2.2.0-alpha.1").unwrap();
    let digest = inspected.manifest_sha256();
    let approved = inspected.approve(decision(digest)).unwrap();
    fs::write(code.path().join("dist/renderer.js"), b"changed").unwrap();
    assert!(approved.revalidate().is_err());
}
