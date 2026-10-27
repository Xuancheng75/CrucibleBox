# Next 2.0 UI helpers

`createClient(rendererContext)` provides the generic API 6 client plus storage, configuration, task, theme and dialog helpers. Domain methods remain in plugin code, called through `backend(method, params)` or a declared `service(id, method, params)`.

Storage preserves the existing owner namespace and atomic transaction/chunk protocol for values up to the contract limit. Large values are never truncated to fit an RPC frame. The package has no React, host source, plugin ID or domain-service dependency. Render with the framework bundled by the plugin and clean up in the function returned from `mount`.
