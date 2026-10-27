use cruciblebox_next2_protocol::{gateway::*, *};
use serde_json::{json, Value};

fn fixtures() -> Value {
    serde_json::from_str(include_str!("../../../../contracts/next2/fixtures.json")).unwrap()
}
fn native(id: &str) -> Manifest {
    let mut manifest = fixtures()["native"].clone();
    manifest["id"] = json!(id);
    validate_manifest(&manifest.to_string()).unwrap()
}
#[test]
fn shared_contract_fixtures() {
    for fixture in fixtures()["cases"].as_array().unwrap() {
        let raw = fixture["raw"].as_str().unwrap();
        let result = match fixture["schema"].as_str().unwrap() {
            "manifest" => validate_manifest(raw).map(|_| ()),
            "resource" => validate_resource_manifest(raw).map(|_| ()),
            "catalog" => validate_resource_catalog(raw).map(|_| ()),
            "request" => validate_request(raw).map(|_| ()),
            "response" => validate_response(raw, "r-1").map(|_| ()),
            "frame" => validate_backend_frame(raw).map(|_| ()),
            _ => panic!("unknown fixture"),
        };
        assert_eq!(
            result.is_ok(),
            fixture["valid"].as_bool().unwrap(),
            "{}: {result:?}",
            fixture["name"]
        );
    }
}
fn request(id: usize, target: Value, method: &str, params: Value) -> String {
    json!({"wireVersion":4,"requestId":format!("r-{id}"),"session":"a".repeat(64),"target":target,"method":method,"params":params}).to_string()
}
fn installed(manifest: Manifest) -> Installed {
    Installed {
        manifest,
        enabled: true,
    }
}
fn session(manifest: Manifest) -> Session {
    Session::new(
        manifest,
        "a".repeat(64),
        "main:frame-1".into(),
        "renderer".into(),
        1000,
    )
}
#[test]
fn owner_is_derived_from_host_and_business_methods_are_opaque() {
    let manifest = native("acme-independent");
    let registry = Registry::new([installed(manifest.clone())]).unwrap();
    let mut session = session(manifest);
    let raw = request(
        1,
        json!({"kind":"backend"}),
        "never-in-host-enum",
        json!({"owner":"sample-alpha"}),
    );
    assert!(session.admit(&registry, &raw, "other-frame", 1).is_err());
    let admitted = session.admit(&registry, &raw, "main:frame-1", 1).unwrap();
    assert_eq!(admitted.owner, "acme-independent");
    assert_eq!(
        admitted.route,
        Route::Backend {
            initiator: "acme-independent".into(),
            provider: "acme-independent".into(),
            service: None
        }
    );
    session.finish("r-1");
    assert!(session.admit(&registry, &raw, "main:frame-1", 1).is_err());
    assert!(session
        .admit(
            &registry,
            &request(2, json!({"kind":"platform"}), "clipboard.read", json!({})),
            "main:frame-1",
            1
        )
        .is_err());
    assert!(session
        .admit(
            &registry,
            &request(3, json!({"kind":"platform"}), "data.directory", json!({})),
            "main:frame-1",
            1
        )
        .is_err());
    assert!(session
        .admit(
            &registry,
            &request(4, json!({"kind":"backend"}), "call", json!({})),
            "main:frame-1",
            1000
        )
        .is_err());
}
#[test]
fn declared_service_version_identity_and_dependency_failures() {
    let mut provider = native("sample-provider");
    provider.exports = vec![ServiceExport {
        id: "transform".into(),
        version: "1.2.0".into(),
        methods: vec!["recognize".into()],
    }];
    let mut caller = native("translate-client");
    caller.dependencies = vec![ServiceDependency {
        id: "transform".into(),
        provider_id: "sample-provider".into(),
        version: VersionRange {
            min: "1.0.0".into(),
            max_exclusive: "2.0.0".into(),
        },
    }];
    let registry = Registry::new([installed(caller.clone()), installed(provider.clone())]).unwrap();
    let route = registry
        .resolve(
            "translate-client",
            &Target::ServiceTarget(ServiceTarget {
                kind: Default::default(),
                id: "transform".into(),
            }),
            "recognize",
        )
        .unwrap();
    assert_eq!(
        route,
        Route::Backend {
            initiator: "translate-client".into(),
            provider: "sample-provider".into(),
            service: Some("transform".into())
        }
    );
    assert_eq!(
        registry.dependents("sample-provider"),
        vec!["translate-client"]
    );
    assert!(Registry::new([installed(caller.clone())]).is_err());
    assert!(Registry::new([
        installed(caller.clone()),
        Installed {
            manifest: provider.clone(),
            enabled: false
        }
    ])
    .is_err());
    provider.exports[0].version = "2.0.0".into();
    assert!(Registry::new([installed(caller.clone()), installed(provider.clone())]).is_err());
    provider.exports[0].version = "1.2.0".into();
    provider.dependencies = vec![ServiceDependency {
        id: "translation".into(),
        provider_id: caller.id.clone(),
        version: VersionRange {
            min: "1.0.0".into(),
            max_exclusive: "2.0.0".into(),
        },
    }];
    caller.exports = vec![ServiceExport {
        id: "translation".into(),
        version: "1.0.0".into(),
        methods: vec!["translate".into()],
    }];
    assert!(Registry::new([installed(caller), installed(provider)]).is_err());
}
#[test]
fn updates_revoke_old_sessions_and_task_providers_cannot_impersonate_initiators() {
    let manifest = native("client");
    let mut active = session(manifest.clone());
    let mut updated = manifest;
    updated.version = "1.0.1".into();
    let registry = Registry::new([installed(updated)]).unwrap();
    assert!(active
        .admit(
            &registry,
            &request(1, json!({"kind":"backend"}), "hello", json!({})),
            "main:frame-1",
            1
        )
        .is_err());
    let task = TaskAuthority::new(
        "client".into(),
        "sample-provider".into(),
        "execution-1".into(),
    );
    assert!(task.may_read_or_cancel("client"));
    assert!(!task.may_read_or_cancel("sample-provider"));
    assert!(!task.may_report("client", "execution-1"));
    assert!(!task.may_report("sample-provider", "old-execution"));
    assert!(task.may_report("sample-provider", "execution-1"));
}
