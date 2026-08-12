# ADR-0017: UniEnv interrupted-install recovery

- Status: Accepted
- Date: 2026-08-11
- Scope: M7F UniEnv

## Context

UniEnv already used fixed executable/argument recipes with `shell: false`, pinned HTTPS artifact digests, cancellable
tasks and same-volume staging promotion. Cancellation and ordinary failures removed their staging directories, but an
application or machine crash could leave `.unienv-staging-*` directories beneath a known tool-version root. The task
registry is deliberately in-memory and must not claim that an interrupted installer can be resumed safely.

The first-party capability is exposed on the stable manifest wire as `trusted:unienv`. It is the concrete
`environment.manage` capability: only the exact plugin name, version, API contract, file set and SHA-256 digest pinned
by the host can invoke it. Renaming the wire permission would unnecessarily invalidate installed manifests without
strengthening the host policy.

## Decision

- On trusted-service activation, enumerate only version roots derived from the strict tool/version catalog and the
  canonical configured install root.
- Remove only direct children whose names start with `.unienv-staging-`. Require both the version root and staging
  entry to be ordinary, non-symlink directories and reuse the existing direct-child cleanup guard.
- Never inspect or delete installed runtime siblings. Completed atomic promotions remain authoritative.
- If recovery or configuration validation fails, retain the reason and fail closed for install, combo install,
  uninstall and version switch until the service is restarted with a valid state. Read-only listing and detection
  remain available where safe.
- Do not attempt to resume an arbitrary installer process. A new task starts from a clean staging directory and
  revalidates the pinned artifact.

## Compatibility

The install root, per-tool directory layout, task protocol and user configuration are unchanged. Existing installed
runtimes are neither rewritten nor deleted. UniEnv moves to 0.5.7 and the host repins the trusted bundle digest.

## Verification

- A temp-directory test creates interrupted staging trees in two version roots and proves recovery removes both while
  preserving an installed runtime sibling.
- Existing tests cover checksum mismatch, mirror fallback/redirect handling, quoted paths with argv-only spawning,
  cancellation cleanup, staging boundary rejection and non-Windows fail-closed behavior.
- Full repository checks, deterministic plugin packaging, trusted digest verification, SBOM, native ABI, Electron
  fuses and packaged smoke gate the milestone.
