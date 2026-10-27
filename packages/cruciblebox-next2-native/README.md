# Native backend SDK (candidate)

This standalone Rust crate implements the Manifest 6 / wire 4 backend loop. It has registry dependencies only and does not import host sources or Tauri. Native backends are trusted local programs running as the current user; the SDK is not an OS sandbox.

Use `serve(factory, handler)` from the executable's main function. The factory initializes plugin state on its first request. The handler receives the host-authenticated caller, parameters, optional task ID and cancellation flag. `Client::platform` and `Client::service` exchange correlated capability calls. `Call::progress` reports only the current host task. Return a domain `Error` to preserve its code. Protocol stdout is reserved; use stderr for diagnostics. A panic terminates the backend after reporting an error; the host retains responsibility for its process tree.

Read persistent data from `Client::data_directory()` and resource mounts from `Client::resource(id)`. Missing optional models must produce an explicit business error, with no automatic download on activation. Model download/import is a separate user action in the host resource UI. Large output belongs in files; frames are limited to 64 KiB.

`cargo build --example arithmetic` builds the sample using only this crate and its public dependencies. The sample proves arithmetic/capability/progress transport and is not OCR, translation or P6 acceptance.
