# ADR-0009: TypeScript 6 and ESLint 10 flat configuration

- Status: Accepted
- Date: 2026-08-11
- Scope: M2B build and quality toolchain

## Context

The workspace already used Electron 43, electron-vite 5 and Vite 7, but the compiler and linter remained on
TypeScript 5 and ESLint 8 with the deprecated eslintrc format. Each plugin also declared its own loose TypeScript
range, so a clean install could type-check the host and plugins with different compiler versions.

`eslint-plugin-react` 7.37 does not support ESLint 10. Keeping it would either block the supported linter line or
require an unmaintained peer-dependency override.

## Decision

- Pin TypeScript 6.0.3 once at the workspace root and in all six plugin manifests. npm deduplicates every workspace
  to that exact compiler.
- Remove the deprecated `baseUrl` option. Path mappings use explicit relative targets.
- Use `Node16` module resolution for plugin projects that emit CommonJS backend code and `bundler` resolution for
  renderer projects built by esbuild.
- Replace `.eslintrc.cjs` with `eslint.config.mjs` and pin ESLint 10.8.1, `@eslint/js` 10.0.1,
  `@typescript-eslint` 8.67.0, `eslint-plugin-react-hooks` 7.1.1 and `@eslint-react/eslint-plugin` 5.18.3.
- Keep React linting deliberately bounded to correctness rules equivalent to the prior gate. React compiler and
  stylistic rules are not enabled implicitly; adopting them is a separate, reviewable change.
- Keep `lint` read-only and reserve mutation for `lint:fix`. CI continues to use the read-only command with zero
  warnings allowed.
- Express the shared file API with `Uint8Array` instead of Node's `Buffer`. Backend implementations may still return
  a `Buffer`, which is a `Uint8Array`, while renderer-facing declarations no longer require ambient Node globals.

## Alternatives considered

- Keep TypeScript 5 / ESLint 8: lowest immediate migration cost, but leaves the project on legacy configuration and
  permits compiler drift between workspaces.
- Override the old React plugin's ESLint peer range: rejected because it creates an unsupported quality gate.
- Enable every rule in the new React recommended preset immediately: rejected for this milestone because it mixes a
  toolchain migration with broad application refactoring and makes regressions harder to attribute.

## Compatibility and risk

- Plugin runtime bundles remained byte-stable from Git's perspective after rebuilding all six plugins.
- `Node16` resolution can expose invalid CommonJS-to-ESM imports. Diary uses bundler resolution because its output is
  produced exclusively by esbuild; the three TypeScript-emitting plugin backends remain CommonJS-compatible.
- The lockfile changed substantially because unsupported ESLint 8 and `eslint-plugin-react` dependency trees were
  removed. A clean install and dependency audit are required release gates.

## Verification

- `npm ls` reports one deduplicated TypeScript 6.0.3 and ESLint 10.8.1 across the host and all plugins.
- `npm run check` passes: 172 host tests, 42 GIF tests, 131 UniEnv tests and 9 supply-chain tests.
- `npm run build` passes for all plugins and all Electron processes, including renderer performance budgets.
- Electron 43 native smoke reports Node 24.18.1 and module ABI 148.
- Windows unpacked packaging, fuse verification and isolated packaged smoke pass; packaged startup was 2,075 ms with
  a 464,072 KiB working set.
