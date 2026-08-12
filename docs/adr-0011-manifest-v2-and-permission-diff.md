# ADR-0011: Manifest v2 and permission-difference consent

- Status: Accepted
- Date: 2026-08-11
- Scope: M4 contracts and M5 SDK compatibility

## Context

Backend and renderer RPC already have versioned v2 contracts, but the plugin manifest did not declare its own schema
generation. A package could omit both API versions and silently enter compatibility behavior. Installation confirmation
listed the final permission set but did not distinguish permissions newly requested by an upgrade.

## Decision

- Add optional `manifestVersion: 1 | 2` to the runtime type and strict parser. Missing or explicit v1 remains the
  Legacy Full Trust compatibility path.
- Manifest v2 requires both `backendApiVersion: 2` and `rendererApiVersion: 2`; unknown schema or API versions fail
  closed before staging.
- ADR-0013 refines this rule for explicit `backend: false` packages: renderer API v2 remains required, while the
  unused backend API version may be omitted.
- Migrate the template and all six production plugins to manifest v2 with patch-version releases.
- Extend the immutable install preview token with manifest/API mode, previous version and exact added/removed
  permissions. The native confirmation uses a warning style for Legacy Full Trust or newly added permissions.
- Keep the v1 adapter isolated: legacy renderer code runs only inside the cross-origin sandboxed iframe and legacy
  backend semantics still travel over the authenticated, budgeted utility-process RPC transport.
- Continue signing the deterministic release artifact manifest with Ed25519 outside the repository. Direct local
  sideload remains an explicit Full Trust action rather than pretending an unsigned ZIP is authenticated. Marketplace
  key distribution and platform installer signing remain M9 release concerns.

## Compatibility and migration

- Existing installed manifests without `manifestVersion` are not rewritten and continue as v1.
- ADR-0019 closes the new-install and upgrade boundary to Manifest v1. The parser and runtime adapter remain only to
  preserve existing installations and user data during migration.
- User plugin IDs, configuration and storage namespaces are unchanged.
- Production plugin versions move to Diary 0.4.10, Dice 0.1.5, GIF 0.3.7, ThemeManager 0.1.10, Turntable 0.1.9 and
  UniEnv 0.5.6. UniEnv's trusted bundle policy is repinned to the new manifest bytes.

## Verification

- Parser tests reject unknown manifest versions and v2 manifests missing either v2 API declaration.
- Install transaction tests assert immutable preview data and exact permission additions/removals.
- Production builds validate all six manifests, renderer bundles and the pinned UniEnv trusted-service digest.
- Deterministic artifact manifests now include `manifestVersion` and remain covered by the existing Ed25519 signing
  and tamper tests.
