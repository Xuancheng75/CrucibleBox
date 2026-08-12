# ADR-0018: Ant Design 6 and semantic styling

- Status: Accepted
- Date: 2026-08-11
- Scope: M8 host UI

## Context

The host was already on React 19 and modern Chromium but declared Ant Design 5. Its cyber themes styled internal
`.ant-*` DOM nodes. Ant Design 6 changes component DOM structure, enables CSS variables by default and requires
`@ant-design/icons` 6; retaining those selectors would make the two flagship themes dependent on undocumented markup.

Replacing Ant Design with MUI or Chakra would rewrite stable forms, overlays, tables and token mappings without fixing
the plugin isolation or release boundaries. The existing component model remains suitable.

## Decision

- Pin Ant Design 6.6.0 and `@ant-design/icons` 6.3.2 after validating the final 5.x line.
- Keep React 19, zustand and the existing Theme v2 registry. React Server Components remain inappropriate for the
  static Electron renderer.
- Use ConfigProvider semantic `classNames` for layout, card, table cells/content, modal container, input, select,
  alert, tag and statistic nodes. All theme CSS targets stable `ob-*` names; host source contains no `.ant-*` selector.
- Replace v6-deprecated Alert, Spin, Space and Descriptions APIs. Place the Ant Design `App` context inside the theme
  provider and use `App.useApp()` instead of static message calls.
- Keep launcher controls keyboard reachable without nesting interactive descendants: an accessible full-card button
  is a sibling of the enable/configure/delete controls. The command palette exposes dialog, listbox and active-option
  semantics.

## Verification

- A static migration test pins matching majors, rejects internal Ant Design selectors and the deprecated props used by
  the old host UI, and evaluates the six-theme by six-plugin renderer contract matrix.
- Existing theme tests continue to cover token completeness, legacy aliases, contrast and distributable theme files.
- Typecheck, lint, all unit/integration tests, production budgets, dependency audit, deterministic plugin artifacts,
  SBOM, Electron ABI/fuses and Windows packaged smoke gate the release.

## References

- https://ant.design/docs/react/migration-v6/
- https://github.com/ant-design/ant-design/releases/tag/6.6.0
