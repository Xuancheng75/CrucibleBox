# ADR-0016: Turntable secure selection and pointer geometry

- Status: Accepted
- Date: 2026-08-11
- Scope: M7E Turntable

## Context

Turntable selected a winner with `Math.random`, while its canvas drew sectors from the top pointer angle but computed
the final rotation against the right-hand axis. The backend result and visible pointer therefore disagreed. Each CRUD
request also performed an asynchronous read-modify-write without serialization, so concurrent adds could reuse an ID
and overwrite one another. Reorder silently accepted partial and unknown ID sets.

## Decision

- Generate the winner sample from Web Crypto `getRandomValues`. Map one unsigned 32-bit value to the half-open unit
  interval and select from half-open cumulative weight intervals. Reject empty, non-finite or non-positive weights.
- Use the same Web Crypto source for the cosmetic full-spin count so Turntable contains no `Math.random` path.
- Define the pointer at `-pi/2`, compute each weighted sector center from that origin and choose the smallest forward
  rotation that places the selected center under the pointer, plus the requested whole turns.
- Serialize every backend read-modify-write through one mutation queue. Persist the complete ordered list with the
  host's atomic `storage.batch`; concurrent mutations therefore observe the previous committed result.
- Require reorder payloads to contain every existing ID exactly once. Bound item count, labels, weights, IDs and
  colors before persistence, and normalize legacy stored arrays on read.

## Compatibility

- The `items` storage key and `TurntableItem` JSON shape do not change. Existing schema-v1 rows continue through the
  established host migration and are normalized without rewriting unless the user performs a mutation.
- IDs, order and timestamps survive deactivate/activate and application restart. No migration or user-data deletion
  is introduced.

## Verification

- Pure tests cover exact weighted boundaries, invalid samples/weights and the invariant that every winner center lands
  on the top pointer across unequal weights and arbitrary prior rotations.
- Backend tests force concurrent adds through delayed persistence, verify unique IDs/no lost writes, strict full-set
  reorder, restart persistence and deterministic Web Crypto winner selection.
- Full repository checks, production build, performance budgets, dependency audit, deterministic artifacts, SBOM,
  native ABI, Electron fuses and packaged smoke gate the release.
