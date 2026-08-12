# ADR-0019: Windows-only GitHub releases and update channels

Date: 2026-08-11
Status: Accepted; supersedes the earlier cross-platform/signing scope of this ADR

## Context

CrucibleBox needs a complete Windows installation and automatic-update loop without paid Authenticode credentials or
paid hosting. macOS, Linux and Windows ARM64 are out of scope. Private GitHub update repositories would require a token
on every client, so the update source must be a public GitHub repository.

The product already has explicit update UI, controlled shutdown, data migrations, package smoke tests, deterministic
plugin artifacts, Ed25519 plugin signatures, SBOMs, checksums and provenance tooling.

## Decision

- Build only Windows 10/11 x64 NSIS artifacts. Remove macOS/Linux targets, entitlements and CI jobs.
- Publish intentionally unsigned installers. The application and documentation disclose Unknown publisher and
  SmartScreen behavior; unsigned artifacts are never represented as Authenticode-trusted.
- Use the public GitHub provider from `electron-builder`. Stable tags emit `latest.yml`; `beta` prerelease tags emit
  `beta.yml`. No GitHub token is embedded in the client.
- Before publication, validate the metadata version, installer basename, blockmap and installer SHA-512. Continue to
  produce deterministic SHA-256 checksums, CycloneDX SBOMs, signed first-party plugin manifests, Electron fuse checks
  and GitHub provenance attestations.
- Keep update download and installation user-initiated. Only an explicit beta-to-stable channel switch may enable one
  downgrade check.
- Qualify previous -> candidate -> previous on Windows with one isolated userData directory. A real two-release NSIS
  update exercise remains the final promotion gate.
- Continue rejecting newly installed Manifest v1 packages while preserving existing installed v1 data and runtime
  compatibility.

## Consequences

- A public GitHub repository and GitHub Actions permission are required, but no hosting subscription or Windows
  certificate secret is required.
- First installation and some updates can show Windows reputation warnings. This is an accepted product limitation,
  not a failure of the update transport.
- The project no longer claims macOS or Linux compatibility. Historical cross-platform experiments remain documented
  only as superseded engineering history.
- Plugin Ed25519 keys remain repository-external because plugin trust is independent of Authenticode.

## References

- https://www.electron.build/docs/features/auto-update/
- https://www.electron.build/docs/configuration/publish/
- https://docs.github.com/actions/security-for-github-actions/using-artifact-attestations/establishing-provenance-for-builds
