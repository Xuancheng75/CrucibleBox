# @cruciblebox/plugin-ui

Standalone React UI primitives for CrucibleBox plugin renderers. The package has a React peer dependency and no Ant Design or host runtime dependency. The JavaScript bundle installs its styles once in the plugin iframe, so a self-contained renderer needs no separate CSS asset.

```tsx
import { PluginPage, Toolbar, Button } from '@cruciblebox/plugin-ui'

export function Example() {
  return <PluginPage><Toolbar><Button>Run</Button></Toolbar></PluginPage>
}
```

Components consume the host's `--ob-*` theme variables and include local fallbacks so a plugin can also render independently.
