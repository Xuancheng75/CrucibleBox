# Next 3.0 independent build CLI (candidate)

Install the published-shape `@cruciblebox/next2-api` and `@cruciblebox/next2-build` packages with npm. No host checkout is required.

```
cruciblebox-next2 init my-plugin frontend
cruciblebox-next2 init my-js-plugin js
cruciblebox-next2 init my-sum-service provider
cruciblebox-next2 init my-native-plugin native --native-sdk=<unpacked-cruciblebox-next2-native-crate>
```

Inside the new project, install npm dependencies and run `npm run build`, `npm test`, then `npm run pack`. The generated test script invokes `cruciblebox-next2 check .` to verify the contract and built payload inventory without creating a ZIP. Add plugin-specific business tests separately. Native initialization/build uses Cargo with a committed lockfile and an already populated public crate cache (`cargo fetch --locked` can populate it explicitly). Local Cargo dependencies and any explicit CARGO_TARGET_DIR must stay inside the plugin. The Rust SDK is copied into `native/vendor`, so the resulting project is independent of the SDK's original location.

`plugin.source.json` contains metadata; `plugin.build.json` declares source entries, assets, browser workers and an optional native Cargo target. Build writes the exact payload inventory into `plugin.json`; pack rejects changed output and never replaces an existing ZIP. Backend methods remain plugin-defined strings. Model binaries belong in separately reviewed resource packages, not these templates.

Source aliases and workspace dependency reuse are build conveniences only. The packed renderer is bundled; no host-relative source import or runtime node_modules lookup is supported. Native plugins need explicit local-code trust at installation. The examples exercise platform transport and do not count as official-plugin, OCR, translation or fixed-host P6 acceptance.

The templates require CrucibleBox 2.2.0-beta.2 or newer. The `provider` template uses `defineProvider` with a scoped API client, runs a headless QuickJS backend and exports `sum` under the project ID. A consumer declares that ID and interface version range in `dependencies`, then calls `resolveCapability(id)` and `service(id, 'sum', { numbers: [7, 19] })`. Pin `providerId` when the consumer depends on a specific provider's identity or private state. JS/native templates include a declared ping command. Renderers use the public UI package, theme hooks and accessible status messages.
