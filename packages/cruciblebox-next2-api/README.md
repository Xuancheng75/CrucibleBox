# @cruciblebox/next2-api 7.0.0-beta.3

Next 3.0 candidate SDK for Manifest 7, API 7, wire 5 and storage data schema 1. Use CrucibleBox 2.2.0-beta.2 for this SDK's new public services. Existing manifest shapes can retain the 2.2.0-beta.1 minimum; tools, process/download permissions and command placements/keywords require beta.2.

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

The client exposes `files.info/list/writeResult`, `network.download`, `process.run` and `runtime.list`. Writing results, downloading files and running tools require a backend execution task. Downloads require exact bytes and SHA-256, run through the host network policy and return an owner-bound file reference. Tools name an approved native runtime resource and EXE inventory entry; callers supply a tool ID and argument array. Process input is limited to 4 KiB, each captured output to 8 KiB, execution to 60 seconds. A returned nonzero exit code remains the caller's responsibility.

QuickJS providers can export `activate = defineProvider(({api}) => ({ sum: ({numbers}) => numbers.reduce((a, b) => a + b, 0) }))`. The activation context contains the scoped public client and host-issued session. Declare exported methods, input/output schemas and service dependencies in the manifest. `logging.write` accepts optional error code and bounded context; it never implicitly includes application secrets. See the generated method types for permissions and limits.

Native backends are explicitly trusted local software running as the current user. SDK permissions do not enforce an OS sandbox. A backend entry is an installed relative `.exe` path for Windows x64; a renderer never supplies an executable path or shell command. Model/runtime resources are separate, versioned packages and must be imported before use; startup does not fetch dependencies.

`files` lists runtime payload paths, sizes and SHA-256 digests. `plugin.json`, `resource.json` and detached `signature.json` are reserved metadata and are not listed as their own payload. Package paths are relative ASCII paths with `/`, without traversal, Windows device names or case collisions. Install verification must check actual files and executable architecture; manifest validation alone does not prove integrity or install consent.
