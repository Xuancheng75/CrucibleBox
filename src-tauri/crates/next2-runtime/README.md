# Next 2.0 runtime primitives (P2, not production host integration)

This crate contains no PDF/OCR/archive/environment/translation business implementation. It provides package inspection, approval binding, native IPC/process supervision and offline resource-directory import for Manifest/API 6 and wire 4.

- Package payload paths, sizes and SHA-256 digests are verified against the manifest. Unlisted/missing/non-regular files, reparse points, invalid PE files and architecture mismatches fail before launch. PE payloads require a native trust declaration.
- Detached Ed25519 signatures are checked against a supplied publisher/key policy. An invalid or unknown signature fails; it does not become an unsigned development package. Unsigned local approval, native-code approval and permission approval are explicit and bound to the exact manifest digest. Display names do not establish official identity.
- Approved payloads are revalidated before launch. Executable paths come from that package, never renderer arguments. Backends start suspended, join a Windows Job Object, then resume. The host uses bounded stdin/stdout queues and versioned handshake. Stdout is protocol only.
- Domain error codes/messages are returned intact. Framing failures, timeout, unexpected exit and unacknowledged cancellation stop the process tree. A stop handle can terminate a busy backend without acquiring its request lock. Job Objects manage lifecycle; they are not an OS security sandbox.
- Independent resource directories have publisher/source/license/version metadata and their own budgets. Imports copy bounded payloads into a same-volume staging directory, flush them, revalidate and publish a new immutable version using rename. Failure preserves previous versions. No startup downloader, recovery retry loop or automatic fallback is implemented.

Actual Windows test commands, with a writable scratch directory chosen explicitly:

```powershell
$env:TEMP = 'E:\CrucibleBox_Sourses\artifacts\next2\runtime-tests-temp'
$env:TMP = $env:TEMP
$env:CARGO_INCREMENTAL = '0'
cargo fmt --manifest-path src-tauri/crates/next2-runtime/Cargo.toml --check
cargo clippy --manifest-path src-tauri/crates/next2-runtime/Cargo.toml --offline --all-targets --features test-backend -- -D warnings
cargo test --manifest-path src-tauri/crates/next2-runtime/Cargo.toml --offline --features test-backend
```

The test-backend feature builds a real PE test executable outside the host production bundle. It performs arithmetic, exchanges a host capability, reports progress, acknowledges cancellation, crashes, and creates a child that ignores cancellation. Tests check actual process termination. Package tests use real signing keys and file corruption; their resource data is explicitly an integrity fixture, not an inference model.

Still required for P2: connect these primitives to the existing installer journal/metadata and main-window confirmation UI; persist approvals; implement installed/versioned code discovery and resource mount resolution; connect native task ownership/events/results to task-runtime and the task center; stop/await all active calls before disable/upgrade/uninstall/host exit; finish JS backend integration; add resource ZIP import/removal and existing recovery acceptance. None of those production operations is established by the primitive tests.

No current host command accepts Manifest 6 as a result of adding this crate alone. No fixed Next 2.0 installer, migrated official plugin, O1 product, NLLB model or translation performance result is included here.

References: [Windows Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects), [AssignProcessToJobObject](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-assignprocesstojobobject). Runtime package entries differ from build-time [Tauri externalBin](https://v2.tauri.app/develop/sidecar/).
