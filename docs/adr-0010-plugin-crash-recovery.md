# ADR-0010: Bounded plugin startup and crash-loop recovery

- Status: Accepted
- Date: 2026-08-11
- Scope: M3 lifecycle reliability

## Context

The host already creates its first window before restoring enabled plugins and coordinates per-plugin activation,
deactivation and maintenance with single-flight operations. Two reliability gaps remained: startup restoration used
unbounded `Promise.all`, and an unexpected backend exit required a manual restart. A runtime RPC timeout rejected only
the caller while leaving the unresponsive utility process alive.

## Decision

- Restore enabled plugins in the background with a default concurrency of two, configurable for tests and clamped to
  a maximum of eight. A slow plugin cannot block the first window or consume an unbounded number of utility processes.
- Treat a backend request timeout as a failed runtime: reject the request, force termination, classify the exit as
  unexpected and let the lifecycle supervisor recover it.
- Recover unexpected exits with bounded exponential delays of 1 s, 5 s and at most 30 s.
- Quarantine a plugin after three crashes within five minutes. Quarantine is fail-closed in memory and persists by
  setting the plugin disabled; the user must explicitly enable it to clear the crash history.
- Cancel pending recovery before explicit activation, deactivation, uninstall and application shutdown. Maintenance
  and deactivation operations always take precedence over an automatic restart.

Electron owns the utility-process parent/child lifetime. Application shutdown cancels recovery timers and awaits all
active or activating runtimes; a utility process is not treated as a separately persistent daemon.

## Consequences

- A transient backend crash recovers without user intervention.
- A deterministic crash loop cannot create an unlimited restart storm.
- A plugin disabled by quarantine is visible as an error and remains off after restart, protecting startup reliability.
- This policy does not make trusted Node backend code a security sandbox; it is a reliability boundary only.

## Verification

- Unit tests cover backoff, window expiry, reset and quarantine decisions.
- Lifecycle tests cover bounded startup concurrency, automatic recovery, crash-loop quarantine, startup timeout kill,
  running-request timeout kill, expected stop ordering and explicit operator restart.
- The normal release gates remain `npm run check`, `npm run build`, native ABI smoke, unpacked package and packaged
  smoke with isolated user data.
