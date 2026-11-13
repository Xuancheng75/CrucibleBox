# cookie 0.18.2 local build configuration

Source: the crates.io cookie 0.18.2 release archive. Its SHA-256 is
`1a373e3602691c3cdea496d2f0ee5935151e6168fe87739483c463db1b2f2f87`, as recorded
in the host lockfile before this path override. Upstream source revision:
`1ec8fded5654679482a91c784e7ff2874b0fdeff`.

The 13 files under `src/`, README and both upstream license files are unchanged
from that archive. The library retains its upstream MIT OR Apache-2.0 license.
This directory is build input, not an archived program or a private plugin.

Only Cargo.toml is modified:

- `build = false`; build.rs is not included.
- The `version_check` build dependency is removed.
- Cargo's `unexpected_cfgs` lint declares `cfg(nightly)` as a known name without
  enabling it. No executable is used to discover this configuration.

The omitted upstream build.rs only probes support for the unstable `doc_cfg`
feature and sets `nightly` for documentation annotations. The production host
uses stable Rust; cookie parsing, rendering, expiry, jars and WebView interfaces
remain the upstream library implementation. Nightly-only documentation labels
are not enabled by this local build configuration.

The original build-script executable was blocked by Kaspersky. It is not
restored, renamed or rebuilt by this patch. This change does not establish that
the original executable was a false positive. Keep antivirus protection active
and verify the replacement build and its outputs independently.

When updating cookie or Tauri, compare the new upstream build script, library
and required features before carrying this override forward. Verify the release
archive against its registry checksum, preserve the upstream licenses, run the
upstream library tests and the host's WebView/IPC checks. Remove the override
when a reviewed upstream release no longer requires this local configuration.
