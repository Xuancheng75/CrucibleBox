# @cruciblebox/next2-api 6.0.0-alpha.2

Next 2.0 candidate SDK: Manifest/API 6, wire 4, storage data schema 1. This package does not make existing Manifest 5 hosts accept Manifest 6. Production host integration and fixed-host acceptance are separate work.

Optional `contributes.fileHandlers: [{extensions: ['zip', 'mrpack']}]` routes OS file drops to the active plugin's `context.onFilesDropped` callback. Extensions are unique lowercase alphanumeric strings without dots. This declaration requires the P6 candidate 2 host or newer; it is independent of plugin identity. OS drop paths are input paths, never commands to execute.

`validateManifest`, `validateResourceManifest`, `validateRequest`, `validateResponse` and `validateBackendFrame` use the types and schema generated from `contracts/next2/contract.json`. The published package contains its generated schema; consumers do not need that repository.

```js
import { createClient } from '@cruciblebox/next2-api'
const api = createClient({ session: context.session, exchange: context.exchange })
await api.backend('translate', { text: 'Hello', source: 'eng_Latn', target: 'zho_Hans' })
await api.service('ocr', 'recognize', { ref: 'host-issued-file-reference' })
```

These calls demonstrate routing syntax, not an implemented translation or OCR service. The provider and compatible service version must be declared in the calling plugin's dependencies. Only the host determines the caller, provider and storage namespace. Domain parameters cannot change that identity.

The `platform(method, params)` method calls only a versioned platform capability. Unknown capabilities and calls denied by the installation's permissions or execution context fail. Backend and declared service business methods do not require adding a host enum.

Native backends are explicitly trusted local software running as the current user. SDK permissions do not enforce an OS sandbox. A backend entry is an installed relative `.exe` path for Windows x64; a renderer never supplies an executable path or shell command. Model/runtime resources are separate, versioned packages and must be imported before use; startup does not fetch dependencies.

`files` lists runtime payload paths, sizes and SHA-256 digests. `plugin.json`, `resource.json` and detached `signature.json` are reserved metadata and are not listed as their own payload. Package paths are relative ASCII paths with `/`, without traversal, Windows device names or case collisions. Install verification must check actual files and executable architecture; manifest validation alone does not prove integrity or install consent.
