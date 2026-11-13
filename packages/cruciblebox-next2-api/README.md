# @cruciblebox/next2-api 7.0.0-beta.3

Next 3.0 candidate SDK for Manifest 7, API 7, wire 5 and storage data schema 1. Existing manifest shapes can retain the 2.2.0-beta.1 minimum; tools, process/download permissions and command placements/keywords require beta.2. Runtime Capability declarations, host capability dependencies, mutable files, binary HTTP, uploads, resumable downloads, WebSockets and process working-directory/environment options require 2.2.0-beta.7. Check each generated method's `since` version; beta.7 is undergoing integration and release acceptance.

Optional `contributes.fileHandlers: [{extensions: ['zip', 'mrpack']}]` routes OS file drops to the active plugin's `context.onFilesDropped` callback. Extensions are unique lowercase alphanumeric strings without dots. It is independent of plugin identity. OS drop paths are input paths, never commands to execute.

`validateManifest`, `validateResourceManifest`, `validateRequest`, `validateResponse` and `validateBackendFrame` use the types and schema generated from `contracts/next2/contract.json`. The published package contains its generated schema; consumers do not need that repository.

```js
import { createClient } from '@cruciblebox/next2-api'
const api = createClient({ session: context.session, exchange: context.exchange })
await api.backend('translate', { text: 'Hello', source: 'eng_Latn', target: 'zho_Hans' })
await api.service('ocr', 'recognize', { ref: 'host-issued-file-reference' })
```

These calls demonstrate routing syntax, not an implemented translation or OCR service. The provider and compatible service version must be declared in the calling plugin's dependencies. Only the host determines the caller, provider and storage namespace. Domain parameters cannot change that identity.

The `platform(method, params)` method calls only a versioned platform capability. Unknown capabilities and calls denied by the installation's permissions or execution context fail. Backend and declared service business methods do not require adding a host enum.

The client exposes managed files, directory operations, copy/move/remove, bounded OS notifications, temporary resources and streamed writes. Mutations that publish outputs require a host task. A frontend can submit supported built-in operations using `api.tasks.start({kind:'platform'}, method, params)`; streamed open/append/commit/abort run together inside one backend task and abandon uncommitted streams when that task ends. Temporary resources and watches belong to the creating session and have explicit release/close operations. Replacing existing files or recursive operations requires an explicit option.

`network.request` supports bounded Base64 request/response bytes (16 KiB); `network.upload` streams an authorized file in a task. `network.resumable` prepares, queries, resumes and explicitly discards persistent private checkpoints. Downloads require exact bytes and SHA-256, run through the host network policy and publish an owner-bound file reference only after verification. A mismatched Range response fails while retaining the checkpoint; there is no automatic restart. Verified checkpoints remain until explicitly discarded. WebSocket handles are session-bound, use the same proxy policy and native TLS trust, and expose bounded send/poll/close operations. Poll drives buffered writes and control frames; close releases the local connection without promising peer acknowledgement.

Tools name an approved native runtime resource and EXE inventory entry; callers supply a tool ID and argument array. `process.run` supports an authorized directory reference and explicit environment variables. Process input is limited to 4 KiB, each captured output to 8 KiB, execution to 60 seconds. A returned nonzero exit code remains the caller's responsibility. A Runtime Provider declares `runtimeCapabilities`; its consumers declare both a compatible Capability dependency and `runtimeRequirements`, then resolve through `api.runtime.resolve(id)` and submit the selected `run` service as a task. Capability interface, resource package and engine versions are independent. Only the provider owns the approved runtime resource; consumers can reuse it without carrying copies.

`capabilities.list/describe` describe declared bindings and actual availability. `capabilities.providers` lists installed host/plugin providers, including stopped providers, without granting permission to call them. Built-in services use the same registry and version resolver as plugin exports; `cruciblebox-host` is reserved. Availability distinguishes registered, starting, ready, failed, stopping, stopped and unavailable states.

QuickJS providers can export `activate = defineProvider(({api}) => ({ sum: ({numbers}) => numbers.reduce((a, b) => a + b, 0) }))`. The activation context contains the scoped public client and host-issued session. Declare exported methods, input/output schemas and service dependencies in the manifest. `logging.write` accepts optional error code and bounded context; it never implicitly includes application secrets. See the generated method types for permissions and limits.

Native backends are explicitly trusted local software running as the current user. SDK permissions do not enforce an OS sandbox. A backend entry is an installed relative `.exe` path for Windows x64; a renderer never supplies an executable path or shell command. Model/runtime resources are separate, versioned packages and must be imported before use; startup does not fetch dependencies.

`files` lists runtime payload paths, sizes and SHA-256 digests. `plugin.json`, `resource.json` and detached `signature.json` are reserved metadata and are not listed as their own payload. Package paths are relative ASCII paths with `/`, without traversal, Windows device names or case collisions. Install verification must check actual files and executable architecture; manifest validation alone does not prove integrity or install consent.
