---
id: 324
title: "spin_once discards session.drive_io() errors and no session-health surface exists — a dead session spins Ok(()) forever"
status: open
type: bug
severity: high
area: core
related: [issue-0268]
---

## Finding (audit 2026-07-28, P1 — lead-verified)

`packages/core/nros-node/src/executor/spin.rs:4893-4896`:

```rust
let _ = self.session.drive_io(primary_drive_timeout_ms);
for extra in self.extra_sessions.iter_mut() {
    let _ = extra.drive_io(0);
}
```

The primary session's transport I/O error is discarded, and there is **no
session-health surface anywhere in the crate** — `git grep` for
`session_health` / `consecutive_fail` / `io_error_count` across
`packages/core/nros-node/src` returns nothing. So a session that has died
(router gone, lease expired, socket closed) keeps returning `Ok(())` from
`spin()` indefinitely: the node looks alive, publishes go nowhere, and no
callback ever fires.

Same shape in the C blocking spins — `packages/core/nros-c/src/executor.rs:1911`
and `:1949` loop on `let _ = nros_executor_spin_some(...)`, so a persistent
transport failure never reaches the C caller and the blocking spin eventually
returns OK on shutdown.

## Why this matters beyond tidiness

Silent-death-of-transport is the single hardest failure mode to diagnose in this
codebase — issue 0268 burned days on a symptom (`declare -128`,
`register_subscription -1`) whose upstream cause was invisible because nothing
surfaced it. An executor that cannot report "my session is not doing I/O" forces
every such investigation to start from packet captures.

Note the distinction the current code *intends*: extra sessions are
best-effort (bridge/multi-domain), the primary is not. That intent is not
expressed — both use `let _ =`.

## Fix

1. Propagate the primary session's `drive_io` error out of `spin_once`, or (if
   the loop must not abort on a transient) keep a sticky health flag plus a
   consecutive-failure counter that `spin()` reports and
   `nros_executor_last_error` exposes to C.
2. Keep the extra-session best-effort behaviour, but make it explicit
   (`let _ = /* best-effort: bridge sessions must not abort the primary spin */`).
3. `nros-c`: break out of the blocking spins on a non-OK code (or an
   error-count threshold) so the C contract matches Rust's.
