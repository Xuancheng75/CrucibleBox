# ADR-0013: Renderer-only plugins and unbiased Web Crypto randomness

- Status: Accepted
- Date: 2026-08-11
- Scope: M7A Dice

## Context

Dice had no backend behavior, but its manifest still caused the host to start a utility process containing only an
activation stub. Its rolls used `Math.random()`, which is neither a cryptographic entropy source nor suitable for an
unbiased integer mapping when implemented with floating-point multiplication.

## Decision

- Add the optional Manifest v2 field `backend: false`. Missing means enabled for full backward compatibility.
- A renderer-only plugin still carries the existing `main` compatibility entry so installed metadata and package
  layout do not require a database migration. The host validates the entry but never loads it or creates a sandbox.
- Track renderer-only activation in the same lifecycle surface as backend runtimes. Enable, disable, restart during a
  config update, bulk shutdown and active-plugin reporting retain their existing semantics.
- Reject backend messages to renderer-only plugins with a deterministic error instead of silently creating a worker.
- Dice samples `crypto.getRandomValues()` as unsigned 32-bit integers. Rejection sampling discards the modulo tail,
  producing an exact uniform mapping for supported dice sizes.

## Compatibility

- Existing manifests omit `backend` and continue to start their declared backend.
- Manifest v2 still requires renderer API v2. `backendApiVersion: 2` is required only when the backend is enabled.
- Dice keeps `dist/main.js` in its deterministic archive as a compatibility artifact, but the host does not execute it.
- No user settings, plugin IDs, history data or database schema are changed.

## Verification

- Manifest tests cover renderer-only v2 parsing, default backend behavior and invalid field types.
- Lifecycle tests assert activation/status/deactivation without sandbox construction and reject backend messages.
- Dice unit tests cover exact boundaries, rejection of the modulo-bias tail, inclusive dice bounds, a deterministic
  uniform cycle and invalid entropy/dimension inputs.
- Repository check/build, deterministic plugin packaging, native Electron and packaged-app smoke remain release gates.
