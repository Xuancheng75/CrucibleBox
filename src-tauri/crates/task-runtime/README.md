# Shared task runtime

A standalone Rust application service over task-core and task-store. It has no Tauri, window, plugin service or main database dependency. The host composition root supplies one runtime to platform task callers. The legacy task table remains a UI projection and retained history.

Cancellation records intent before signaling an executor and terminalizes only after stop confirmation. Publication reserves the durable state before running file I/O without a task or database lock. Batch publication retains output references while execution continues; later cancellation preserves them. Panics become diagnostic failures with the host release unwind profile. Synchronous return values agree with the persisted cancelled state.

Queues, process leases, retained views and snapshots are bounded. Retired terminal snapshots remain in task-store history. Restart marks unfinished work interrupted. The separate file-publication store retains SHA-256 receipts and original backups until task authority persistence succeeds. Matching single-file receipts recover success; matching partial receipts retain output references and recover interruption. The API 6 host uses this authority for plugin tasks, resource downloads and installation. Business implementation belongs to independent plugins. This is not complete crash recovery.

Independent tests exercise three executor adapters over a real SQLite journal, restart references, queue cancellation, panic isolation, empty/NUL diagnostics, partial publication and synchronous cancellation races. They do not replace native application and installer acceptance.
