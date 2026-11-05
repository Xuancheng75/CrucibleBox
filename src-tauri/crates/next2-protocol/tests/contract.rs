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
fn ordered_renderer_requests_exceed_opaque_budget_without_forgetting_replay_or_expiry() {
    let manifest = native("acme-independent");
    let registry = Registry::new([installed(manifest.clone())]).unwrap();
    let mut session = Session::renderer(manifest, "a".repeat(64), "main:frame-1".into(), 1000);
    let call = |index| {
        json!({"wireVersion":4,"requestId":format!("renderer-{index}"),"session":"a".repeat(64),"target":{"kind":"backend"},"method":"call","params":{}}).to_string()
    };
    for index in 1..=BUDGET_SESSION_REQUESTS + 4 {
        let admitted = session
            .admit(&registry, &call(index), "main:frame-1", 1)
            .unwrap();
        session.finish(&admitted.request.request_id);
    }
    session.renew(2000);
    assert_eq!(
        session
            .admit(&registry, &call(1), "main:frame-1", 1500)
            .err()
            .unwrap(),
        "REPLAY_DENIED"
    );
    assert_eq!(
        session
            .admit(&registry, &call(BUDGET_SESSION_REQUESTS + 5), "other", 1500)
            .err()
            .unwrap(),
        "SESSION_DENIED"
    );
    assert_eq!(
        session
            .admit(
                &registry,
                &call(BUDGET_SESSION_REQUESTS + 5),
                "main:frame-1",
                2000
            )
            .err()
            .unwrap(),
        "SESSION_EXPIRED"
    );
    session.renew(4000);
    assert!(session
        .admit(
            &registry,
            &call(BUDGET_SESSION_REQUESTS + 5),
            "main:frame-1",
            3000
        )
        .is_ok());
}
#[test]
fn ordered_backend_capabilities_outlive_the_opaque_budget_and_keep_replay_protection() {
    let manifest = native("acme-independent");
    let registry = Registry::new([installed(manifest.clone())]).unwrap();
    let mut session = Session::new(
        manifest,
        "a".repeat(64),
        "native".into(),
        "backend".into(),
        1000,
    );
    let call = |index| {
        json!({"wireVersion":4,"requestId":format!("backend-seq-{index}"),"session":"a".repeat(64),"target":{"kind":"platform"},"method":"storage.get","params":{"key":"identity"}}).to_string()
    };
    let completed = BUDGET_SESSION_REQUESTS + 8;
    for index in 1..=completed {
        let admitted = session.admit(&registry, &call(index), "native", 1).unwrap();
        session.finish(&admitted.request.request_id);
    }
    session.renew(2000);
    assert_eq!(
        session
            .admit(&registry, &call(1), "native", 1500)
            .err()
            .unwrap(),
        "REPLAY_DENIED"
    );
    assert_eq!(
        session
            .admit(&registry, &call(completed + 1), "other", 1500)
            .err()
            .unwrap(),
        "SESSION_DENIED"
    );
    assert_eq!(
        session
            .admit(&registry, &call(completed + 1), "native", 2000)
            .err()
            .unwrap(),
        "SESSION_EXPIRED"
    );
    for index in completed + 1..=completed + BUDGET_INFLIGHT {
        session
            .admit(&registry, &call(index), "native", 1500)
            .unwrap();
    }
    let next = completed + BUDGET_INFLIGHT + 1;
    assert_eq!(
        session
            .admit(&registry, &call(next), "native", 1500)
            .err()
            .unwrap(),
        "BUSY"
    );
    session.finish(&format!("backend-seq-{}", completed + 1));
    // A rejected request must not consume the sequence high-water mark.
    session
        .admit(&registry, &call(next), "native", 1500)
        .unwrap();
    assert_eq!(
        session
            .admit(&registry, &call(next), "native", 1500)
            .err()
            .unwrap(),
        "REPLAY_DENIED"
    );
}

#[test]
fn opaque_backend_cannot_reset_its_ledger_by_switching_to_ordered_ids() {
    let manifest = native("acme-independent");
    let registry = Registry::new([installed(manifest.clone())]).unwrap();
    let mut session = Session::new(
        manifest,
        "a".repeat(64),
        "native".into(),
        "backend".into(),
        1000,
    );
    for index in 0..BUDGET_SESSION_REQUESTS {
        let mut raw: Value = serde_json::from_str(&request(
            index,
            json!({"kind":"platform"}),
            "storage.get",
            json!({"key":"identity"}),
        ))
        .unwrap();
        if index == 1 {
            raw["requestId"] = json!("backend-seq-1");
        }
        let admitted = session
            .admit(&registry, &raw.to_string(), "native", 1)
            .unwrap();
        session.finish(&admitted.request.request_id);
    }
    session.renew(2000);
    let mut raw: Value = serde_json::from_str(&request(
        0,
        json!({"kind":"platform"}),
        "storage.get",
        json!({"key":"identity"}),
    ))
    .unwrap();
    assert_eq!(
        session
            .admit(&registry, &raw.to_string(), "native", 1500)
            .err()
            .unwrap(),
        "REPLAY_DENIED"
    );
    raw["requestId"] = json!("backend-seq-9000");
    assert_eq!(
        session
            .admit(&registry, &raw.to_string(), "native", 1500)
            .err()
            .unwrap(),
        "SESSION_EXHAUSTED"
    );
}

#[test]
fn opaque_sessions_keep_their_existing_request_budget_after_renewal() {
    let manifest = native("acme-independent");
    let registry = Registry::new([installed(manifest.clone())]).unwrap();
    let mut session = session(manifest);
    for index in 0..BUDGET_SESSION_REQUESTS {
        let admitted = session
            .admit(
                &registry,
                &request(index, json!({"kind":"backend"}), "call", json!({})),
                "main:frame-1",
                1,
            )
            .unwrap();
        session.finish(&admitted.request.request_id);
    }
    session.renew(4000);
    assert_eq!(
        session
            .admit(
                &registry,
                &request(
                    BUDGET_SESSION_REQUESTS,
                    json!({"kind":"backend"}),
                    "call",
                    json!({})
                ),
                "main:frame-1",
                2000
            )
            .err()
            .unwrap(),
        "SESSION_EXHAUSTED"
    );
    assert_eq!(
        session
            .admit(
                &registry,
                &request(0, json!({"kind":"backend"}), "call", json!({})),
                "main:frame-1",
                2000
            )
            .err()
            .unwrap(),
        "REPLAY_DENIED"
    );
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
