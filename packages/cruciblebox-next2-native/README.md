# Native backend SDK (candidate)

This standalone Rust crate implements the Next 3.0 API 7 and wire 5 backend loop. It depends only on public registry crates. Native backends run as trusted local programs with the current user's system permissions.

Version 0.2.0-beta.2 carries the public P4/P5 method types. Call new services with `Client::platform`; the matching host is 2.2.0-beta.2. Declared runtime tools use `process:run`, approved resource dependencies and a host task. Tool execution is trusted local code with bounded pipes and a host-managed working directory, not an OS sandbox.

Use `serve(factory, handler)` from the executable's main function. The factory initializes plugin state on its first request. The handler receives the host-authenticated caller, parameters, optional task ID and cancellation flag. `Client::platform` and `Client::service` exchange correlated capability calls. `Call::progress` reports only the current host task. Return a domain `Error` to preserve its code. Protocol stdout is reserved; use stderr for diagnostics. A panic terminates the backend after reporting an error; the host retains responsibility for its process tree.

Read persistent data from `Client::data_directory()` and resource mounts from `Client::resource(id)`. Missing optional models must produce an explicit business error, with no automatic download on activation. Model download/import is a separate user action in the host resource UI. Large output belongs in files; frames are limited to 64 KiB.

`cargo build --example arithmetic` builds the sample using only this crate and its public dependencies. The sample proves arithmetic/capability/progress transport and is not OCR, translation or P6 acceptance.

`Client::resolve_capability(id)` returns a generated `CapabilityBinding` with the provider ID, provider package version, lifecycle and exported method descriptors. Declare the dependency in your manifest before querying or calling it. The host checks a typed service's declared contexts, input schema and successful result. The generated binding, RPC error and transport constants come from the public contract.
