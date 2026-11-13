//! Unshipped harness: only the Tauri shell is mocked. Signature verification,
//! download and the Windows installer launch use the locked production updater.
use std::{env, error::Error, fs, path::PathBuf, time::Duration};
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri_plugin_updater::UpdaterExt;

fn main() -> Result<(), Box<dyn Error>> {
    // Installation is forbidden on the developer machine. This explicit gate is
    // separate from compilation and from ordinary CI/test execution.
    for (key, expected) in [
        ("GITHUB_ACTIONS", "true"),
        ("RUNNER_ENVIRONMENT", "github-hosted"),
        ("CRUCIBLEBOX_INSTALLER_TEST_APPROVED", "true"),
    ] {
        if env::var(key).as_deref() != Ok(expected) {
            return Err(format!("requires approved disposable GitHub runner: {key}").into());
        }
    }
    let mode = env::var("CRUCIBLEBOX_UPDATER_TEST_MODE")?;
    if !matches!(mode.as_str(), "reject" | "install") {
        return Err("expected reject or install mode".into());
    }
    let root = PathBuf::from(env::var("CRUCIBLEBOX_UPDATER_TEST_ROOT")?).canonicalize()?;
    let installed = root.join("installed/cruciblebox.exe");
    if !installed.is_file() {
        return Err("old installed executable is missing".into());
    }
    let configuration: serde_json::Value =
        serde_json::from_slice(&fs::read("src-tauri/tauri.conf.json")?)?;
    let mut context = mock_context(noop_assets());
    context.package_info_mut().version = "2.1.1".parse()?;
    context.package_info_mut().name = "CrucibleBox".into();
    context.config_mut().plugins.0.insert(
        "updater".into(),
        serde_json::json!({
            "pubkey": configuration["plugins"]["updater"]["pubkey"],
            // Loopback serves already-downloaded release files in this harness
            // only; no product transport or certificate setting is modified.
            "dangerousInsecureTransportProtocol": true,
            "windows": { "installMode": "quiet" }
        }),
    );
    let app = mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(context)?;
    let updater = app
        .updater_builder()
        .endpoints(vec![env::var("CRUCIBLEBOX_UPDATER_TEST_ENDPOINT")?.parse()?])?
        .executable_path(&installed)
        .timeout(Duration::from_secs(60))
        .no_proxy()
        .installer_arg(format!("/D={}", installed.parent().unwrap().display()))
        .on_before_exit(|| {})
        .build()?;
    tauri::async_runtime::block_on(async {
        let update = updater.check().await?.ok_or("update was not offered")?;
        if update.version != configuration["version"].as_str().unwrap() {
            return Err("offered version differs from checked-out release".into());
        }
        let downloaded = update.download(|_, _| {}, || {}).await;
        if mode == "reject" {
            match downloaded {
                Err(tauri_plugin_updater::Error::Minisign(error)) => {
                    fs::write(
                        root.join("tampered-rejected.json"),
                        serde_json::to_vec_pretty(&serde_json::json!({
                            "passed": true, "error": error.to_string(),
                            "updater": "2.10.1", "installerExecuted": false
                        }))?,
                    )?;
                    return Ok(());
                }
                Err(error) => return Err(error.into()),
                Ok(_) => return Err("tampered artifact passed signature verification".into()),
            }
        }
        let bytes = downloaded?;
        fs::write(
            root.join("signature-verified.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "passed": true, "from": "2.1.1", "to": update.version,
                "bytes": bytes.len(), "updater": "2.10.1",
                "scope": "production updater download/signature/install; mock Tauri shell"
            }))?,
        )?;
        update.install(bytes)?;
        Err("Windows updater unexpectedly returned instead of exiting".into())
    })
}
