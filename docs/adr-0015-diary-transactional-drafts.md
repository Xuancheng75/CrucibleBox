# ADR-0015: Transactional plugin storage batches and Diary drafts

- Status: Accepted
- Date: 2026-08-11
- Scope: M7D Diary

## Context

Plugin storage already bound every key to the authenticated plugin ID and migrated Diary's legacy global table into
that private namespace. Its four single-key operations were atomic individually, but saving an entry and removing its
recovery draft required two calls. A crash between them could leave ambiguous state. The renderer also treated any
resolved backend request as a successful save and navigated away even when persistence returned a negative result.

A separate SQLite file per plugin was considered. It would add connection, backup, migration and native-resource
management without improving the current JSON document model. The host-owned namespaced store remains the smaller,
more enforceable boundary; the missing capability was a bounded cross-key transaction.

## Decision

- Add `storage.batch` to SDK/backend RPC v2. A batch contains 1-64 strictly validated set/delete mutations and never
  accepts a namespace or SQL.
- Pre-validate every key and serialized JSON value before opening `BEGIN IMMEDIATE`. Commit all mutations together or
  roll back the entire batch on any error.
- Store Diary entries at `entry:<YYYY-MM-DD>` and recovery drafts at `draft:<YYYY-MM-DD>`. A successful explicit save
  writes/deletes the entry and deletes the draft in one batch; delete follows the same rule.
- Serialize Diary mutations in the backend so autosave, explicit save and discard cannot reorder across MessagePort
  requests.
- Return a discriminated `DiaryMutationResult`. The renderer clears dirty state and performs a pending navigation only
  after an `ok` result for the exact editor revision that initiated the save.
- Autosave drafts after 500 ms, best-effort flush on unmount, restore them on the next open and visibly report recovery
  or storage failure.
- Parse calendar keys from their numeric components and derive weekday in UTC. A date-only key is never parsed as a
  local instant, preventing timezone-dependent day rollover.

## Compatibility and recovery

- Existing `entry:*` values and the schema-v1 `diary_entries` migration remain unchanged. No user-data rewrite is
  required for this release.
- Drafts use new keys and can be ignored safely by older plugin versions. Old tables remain byte-preserved by the
  existing migration and packaged-smoke backup gate.
- The storage batch is an additive backend RPC v2 method; existing plugins continue using get/set/delete/list.

## Verification

- SQL.js fault injection proves an error after the first mutation rolls back both the entry and draft changes.
- Diary tests cover leap-day/calendar parsing, exact-revision navigation, draft recovery, atomic draft cleanup and
  explicit storage failure with preserved draft.
- Repository typecheck, lint, tests, production build, dependency audit, deterministic plugin artifacts, SBOM, native
  ABI, Electron fuses and packaged smoke are release gates for the milestone.
