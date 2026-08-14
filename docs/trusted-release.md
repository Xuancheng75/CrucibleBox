# Windows release and automatic-update runbook

GitHub publishing is optional. A repository-free Windows installer remains the default local deliverable and runs with
updates explicitly disabled when `resources/app-update.yml` is absent. See
[`github-auto-update-handoff.md`](github-auto-update-handoff.md) for the repository-owner handoff checklist and the
offline packaging path.

## Supported product boundary

CrucibleBox supports Windows 10/11 x64. The release target is an NSIS installer. macOS, Linux and Windows ARM64 are
not supported or built. Windows installers are intentionally unsigned, so Windows can display Unknown publisher or a
SmartScreen warning. Release documentation must not describe them as Authenticode-trusted.

When online updates are enabled, the GitHub repository used by `electron-updater` must be public. Do not embed a GitHub token in the desktop client.
The source and release repository are currently the same repository.

## Repository setup

GitHub Actions uses its built-in `GITHUB_TOKEN` for releases and attestations. Repository or organization policy must
allow workflows to request `contents: write`, `id-token: write` and `attestations: write`.

First-party plugin signing remains independent of Windows Authenticode and uses free Ed25519 keys. Configure:

| Type     | Name                                |
| -------- | ----------------------------------- |
| Secret   | `OPENBOX_PLUGIN_SIGNING_KEY_BASE64` |
| Secret   | `OPENBOX_PLUGIN_VERIFY_KEY_BASE64`  |
| Variable | `OPENBOX_PLUGIN_SIGNING_KEY_ID`     |

Generate the keys once outside the repository and retain an offline backup:

```powershell
openssl genpkey -algorithm Ed25519 -out openbox-plugin-signing-key.pem
openssl pkey -in openbox-plugin-signing-key.pem -pubout -out openbox-plugin-verify-key.pem
[Convert]::ToBase64String([IO.File]::ReadAllBytes('.\openbox-plugin-signing-key.pem'))
[Convert]::ToBase64String([IO.File]::ReadAllBytes('.\openbox-plugin-verify-key.pem'))
```

Never commit either private-key material or its base64 representation.

## Stable and beta releases

- Stable versions use plain SemVer such as `1.5.23`, tag `v1.5.23`, a normal GitHub Release and `latest.yml`.
- Beta versions use SemVer such as `1.5.24-beta.1`, tag `v1.5.24-beta.1`, a GitHub prerelease and `beta.yml`.
- Any other prerelease channel is rejected by the workflow.
- The tag must exactly equal `v` plus the root package version.

Push the annotated tag only after the Windows compatibility workflow has passed:

```powershell
git tag -a v1.5.23 -m "CrucibleBox 1.5.23"
git push origin v1.5.23
```

The tag workflow performs `npm ci`, all quality gates, dependency audit, host/plugin build, plugin signing, SBOM
generation, Windows NSIS packaging, native ABI smoke, unpacked smoke, real silent install/launch/uninstall smoke and
fuse verification. It verifies that the updater metadata version, installer path, blockmap and SHA-512 agree before
collecting release files. It then creates deterministic SHA-256 checksums, a GitHub artifact attestation and the public
GitHub Release.

## Published files

A stable release contains at least:

```text
CrucibleBox-<version>-windows-x64-setup.exe
CrucibleBox-<version>-windows-x64-setup.exe.blockmap
latest.yml
SHA256SUMS
cruciblebox-*.cdx.json
first-party plugin ZIPs and signed manifests
```

Beta releases replace `latest.yml` with `beta.yml`.

## Client update behavior

Packaged builds check after startup but never download or install without a user action. The settings page displays the
current channel, availability, progress and failures. Downloaded NSIS artifacts are checked against updater metadata.
Before installation, the host stops plugin runtimes, flushes SQLite and closes diagnostic logging, then calls
`quitAndInstall`. The installer relaunches the application after replacement.

Switching from beta to stable is the only operation that temporarily permits downgrade. A failed release should be
withdrawn and replaced with a higher patch version. Keep the preceding installer available until the replacement has
passed the same-userData upgrade and rollback rehearsal.

## Required Windows acceptance

Run `.github/workflows/release-compatibility.yml` with a previous stable tag and the candidate tag. It builds both on
Windows and launches previous -> candidate -> previous against one temporary userData directory, checking the database
schema and a persisted sentinel after both transitions.

Before general distribution, also install the previous public NSIS release on a clean Windows 11 VM, create data in
Diary, Turntable and ThemeManager, publish the candidate, then exercise check -> download -> restart/install -> relaunch
from the application. Confirm the new version, preserved data, plugin activation, cancellation/error UI and the
expected unsigned-publisher warning.

## Consumer verification

```powershell
Get-AuthenticodeSignature .\CrucibleBox-*-windows-x64-setup.exe
Get-FileHash .\CrucibleBox-*-windows-x64-setup.exe -Algorithm SHA256
gh attestation verify --repo OWNER/REPOSITORY .\CrucibleBox-*-windows-x64-setup.exe
```

`Get-AuthenticodeSignature` is expected to report `NotSigned`. Compare the SHA-256 value with `SHA256SUMS`; the GitHub
attestation provides an independently verifiable link to the tag workflow that produced the artifact.
