# ADR-0012: Theme v2 registry and reversible preview

- Status: Accepted
- Date: 2026-08-11
- Scope: M6 theme architecture

## Context

The host already mapped themes to Ant Design tokens and semantic CSS variables, but ThemeManager carried a second,
divergent copy of the six built-in themes. Plugin renderer RPC exposed theme read and write but not registry discovery.
Custom themes saved by older versions could omit newer semantic tokens, and switching a theme was immediately
persistent with no rollback affordance.

## Decision

- Keep `shared/themes/presets.ts` as the only built-in registry. Expose it to isolated plugin renderers through the
  versioned `theme.list` MessagePort RPC method; plugin frames never receive host globals or direct IPC access.
- Remove ThemeManager's copied presets. Its cards consume the host registry and therefore stay synchronized with all
  six built-in themes, including both cyberpunk variants.
- Treat a selection as a reversible host-brokered preview. The frame bridge captures the original theme, serializes
  subsequent previews, commits on Keep, and restores on Undo or bridge disposal even after the plugin port closes.
- Normalize custom and legacy themes onto a complete light/dark semantic token base. Accept only known token names,
  bounded colors, font families and radii; discard unknown or unsafe values.
- Continue emitting canonical kebab-case CSS variables and add the historic camel-case aliases during the migration
  window. Both names carry identical values.
- Establish `@openbox/ui` as a workspace-owned, renderer-safe package for typed semantic CSS-variable primitives.
  It intentionally contains no Electron access and no second theme registry.

## Compatibility

- Stored preset IDs resolve to the current canonical preset, so corrected token definitions apply without rewriting
  user settings.
- Partial old custom themes are completed from mode-specific defaults; invalid stored themes fall back to the default.
- Existing plugins using `--ob-colorBgContainer` and related camel-case variables continue to render. New code uses
  `--ob-color-bg-container` through `@openbox/ui`.
- `theme.get` and `theme.set` are unchanged. `theme.list` is additive to renderer API v2.

## Verification

- RPC contract and frame-bridge tests cover `theme.list` request, result validation and host delegation.
- Theme tests cover six unique presets, WCAG 4.5:1 primary-text contrast, complete legacy normalization, injection
  rejection and old/new CSS-variable equivalence.
- Frame-bridge tests cover preview, commit and disposal rollback at the host side.
- ThemeManager has a clean typecheck and self-contained browser bundle, followed by the repository check/build,
  deterministic plugin artifact and packaged smoke gates.
