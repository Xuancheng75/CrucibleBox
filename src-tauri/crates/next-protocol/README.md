# Experimental Next protocol core

This crate is not a native plugin runtime and is not frozen.

- Request, response and manifest types and maximum concurrent admissions are generated from `contracts/next/contract.json`. Shared fixtures are exercised by Rust and the JavaScript SDK.
- `gateway::Session` is constructed only from verified host records. Its plugin ID is the installed database owner, not a plugin-supplied manifest ID. The caller identity must come from the transport, never request parameters.
- Each executable client instance needs a separately issued cryptographically random session token. SDK request counters restart with each client; sharing a token between backend and renderer instances would cause replay collisions. The main app now issues distinct Next renderer tokens from verified installed metadata; the separate Next backend supervisor issues its own hexadecimal token. The actual application parent bridge uses the shared validators and generated transport budgets.
- Admission checks token, caller, expiry, method capability, replay, concurrent capacity and a bounded lifetime ID ledger. IDs are not evicted during a valid session; a host must rotate an exhausted token.
- `runtime::dispatch` supports synchronous ping, small scoped JSON reads/writes, and bounded backend calls through a host-owned adapter. Storage failures release capacity and consumed request IDs stay consumed. Long operations must use the future task runtime, not this synchronous path.
- The main app's `Db` adapter preserves the current plugin storage table. Invalid or over-budget stored JSON is rejected without rewriting the original row. This is not the one-time Next migration or its rollback mechanism.

Shared structured response/error and manifest validation are implemented. The main app now has a Next-only IPC gateway with origin, active-session, permission snapshot and replay checks; existing legacy tokens are refused. Strict native installation metadata and a separate verified renderer issuer now create API 5 records without mapping them to legacy wire 2. Gateway revocation and database preservation have automated coverage, but those fixtures are not native installation acceptance.

Two real example ZIPs install through native staging/commit in an isolated profile. The actual application PluginHost opens the renderer-only ping and storage-backend note examples; ping returns pong, the independent wire 3 QuickJS backend saves Chinese JSON, and the note is restored after restarting the native application. A short renderer lease is rejected with SESSION_EXPIRED. Installation-token replay is denied. These are debug-host observations, not release installer acceptance.

An unignored real-sidecar regression covers module boot, capability exchange, persisted storage after worker restart, child crash without terminating the host, recovery, maintenance denial, and disabled-plugin denial. Existing legacy sidecar handling refuses API 5 rather than translating its protocol. The native opaque-iframe PoC validates resource loading and isolation separately.

The contract remains experimental and unfrozen. The full build CLI for existing TSX/assets/workers, one-time data migration with paired rollback, central task runtime, and remaining S2–S6 acceptance are still open. At the user's request, existing-plugin migration is paused; no existing 14-plugin migration is authorized by these examples. The original ignored tests remain separately reported.
