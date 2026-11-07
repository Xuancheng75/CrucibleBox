# Next 3.0 UI helpers

`createClient(rendererContext)` provides the Next 3.0 API 7 client plus storage, configuration, task, theme and dialog helpers. Domain methods remain in plugin code, called through `backend(method, params)` or a declared `service(id, method, params)`.

Storage preserves the existing owner namespace and atomic transaction/chunk protocol for values up to the contract limit. Large values are never truncated to fit an RPC frame. The package has no React, host source, plugin ID or domain-service dependency. Render with the framework bundled by the plugin and clean up in the function returned from `mount`.

Version 7.0.0-beta.2 adds `files.choose`, `storage.usage()` and `renderStatus(element, state, text)`, with empty/loading/success/error states and status/alert semantics. Forms use HTML labels and the `.ob-input`, `.ob-select`, `.ob-button` theme hooks. Use task start/get/cancel for progress and dialog.confirm for confirmation; theme style version 2 is negotiated by createClient. These helpers require the matching beta.2 host services and do not inject React components into the host.
