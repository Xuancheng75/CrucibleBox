# ADR-0014: GIF worker boundary and incremental edit history

- Status: Accepted
- Date: 2026-08-11
- Scope: M7B GIF Editor

## Context

GIF Editor already enforced file, frame, pixel, decoded-RGBA, operation, export and history budgets. Residue analysis
and repair ran in a cancellable one-shot worker, but ordinary same-canvas edits still stored a complete copy of every
frame for each undo entry. This made a small brush stroke cost the size of the whole document.

The milestone also called for evaluating OffscreenCanvas. The heavy residue implementation composites and compares
raw RGBA arrays and the GIF codec consumes typed arrays. It does not issue raster drawing commands. Copying every
result through OffscreenCanvas would add a second full-frame allocation and GPU/CPU transfer without removing codec
work or improving cancellation.

## Decision

- Keep residue analysis and repair in the isolated Blob Worker with transferable input/output buffers, strict payload
  validation, AbortSignal cancellation, worker termination and Blob URL revocation.
- Keep the worker's pixel engine on direct typed arrays. Do not add OffscreenCanvas until a drawing or resampling
  workload can replace an existing full-frame CPU pass; capability use without such a workload is rejected.
- Store same-canvas history as reversible XOR byte ranges plus before/after frame delay. Applying the same XOR range
  supports both undo and redo and clones only frames that changed.
- Fall back to the existing bounded deep snapshot when dimensions, frame count or stable frame IDs change. This keeps
  crop, rotate, split, import and other structural operations straightforward and safe.
- Count delta bytes and metadata against the existing 256 MiB / 50-entry history budget. A no-op document copy does
  not create a history entry.

## Compatibility

- The history format is in-memory only; no user data or exported GIF schema changes.
- Frame IDs remain stable through undo/redo, preserving incremental thumbnail cache behavior.
- Structural operations retain their previous snapshot semantics and all canvas invariants remain checked before and
  after commits and restoration.

## Verification

- Pure tests cover localized delta size, XOR undo/redo symmetry, structural fallback, no-op suppression and existing
  byte/entry eviction behavior.
- Existing worker tests cover transferable results, cancellation, malformed replies and size limits.
- GIF Editor typecheck, 44 tests and a clean production bundle pass before repository-wide release gates.
