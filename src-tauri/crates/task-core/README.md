# Experimental task authority

Pure state transitions and a repository port for the Next host. This crate has no Tauri, SQL, filesystem, window or plugin dependency. `task-store` supplies a separate SQLite journal; it does not migrate the beta3 task table.

Mutations compare the expected sequence, persist, then notify. Queue and resource leases are bounded. Cancellation records intent and requires executor stop confirmation. Preparing an output journal reserves publication before file I/O; cancellation cannot win after that reservation. A failed terminal write leaves the durable reservation available for startup reconciliation. A checkpoint is retained as a hint and does not prove safe resumability.

Startup blocks new work until previous queued/running records are reconciled. Verified publication can recover to succeeded; other unfinished work becomes interrupted. Late events cannot mutate terminal records. Terminal snapshots exceeding the hot-set bound are archived by task-store; repositories without durable archival fail closed.

Tests cover persistence failure without false events, queue/resource limits, owner and revision checks, cancellation/publication races, terminal protection and restart reconciliation. The standalone task-runtime supplies a shared host authority for document, UniEnv, archive, process and download adapters. Batch publication retains references to already published outputs. Actual native business-flow acceptance, filesystem publication reconciliation and complete plugin migration remain open; plugin migration is authorized.
