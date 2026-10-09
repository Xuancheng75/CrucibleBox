# CrucibleBox repository

Independent SQLite repository with no Tauri dependency. Owns WAL initialization, transactional schema v1–v10 migrations and legacy plugin/storage/settings preservation. The app keeps a local Db adapter so host extensions and Next storage trait implementations remain outside this crate.

v8 adds session-bound Next storage staging. v9 adds executor revision guards for task projections; it does not modify existing plugin data. Legacy fixture tests require a coherent SQLite online backup in CRUCIBLEBOX_LEGACY_DB_FIXTURE. Tests work on copies and compare all pre-existing rows, failed migration rollback and old database restoration. They do not prove paired installer/program rollback.

Schema v10 retains exact config_data, plugin_storage values/timestamps and migration markers in a single uninstall transaction. Reinstall by stable plugin name restores them under the new installation ID atomically, then removes the retained generation. Conflicting retained generations fail closed. No JSON parsing, transport cap or automatic data expiry applies to these copies. Logs and installation metadata are not user-content backups. This does not establish paired old executable + database rollback or retention of removed package files.
