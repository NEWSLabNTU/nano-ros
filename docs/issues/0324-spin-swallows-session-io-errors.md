---
id: 324
title: "spin_once discards session.drive_io() errors and no session-health surface exists — a dead session spins Ok(()) forever"
status: resolved
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

## Resolved (2026-07-28)

### Item 1 — a health counter, not propagation

The issue offered either. **Propagation was rejected**, and the reason is in
the code: `RmwSessionHandle::drive_io` returns `Err` for *any* non-OK backend
code (`nros-rmw-cffi/src/lib.rs:1580`), and whether a benign poll timeout maps
to one is backend-specific across zenoh / XRCE / Cyclone. Returning early from
`spin_once` on a single failure would risk converting a transient hiccup into a
dead node — a worse bug than the one being fixed, in the same hot path.

So `Executor` gained `consecutive_io_failures`, reset by any successful drive
and incremented (saturating) by a failure, plus two accessors:

```rust
pub fn session_io_failures(&self) -> u32   // 0 = last drive succeeded
pub fn session_io_healthy(&self) -> bool
```

That is the surface whose absence the issue documents — `git grep` for
`session_health` / `consecutive_fail` / `io_error_count` previously returned
nothing, which is why a dead transport could only be diagnosed from packet
captures.

### Item 2 — the intent is now visible

Extra (bridge / multi-domain) sessions keep `let _ =`, with a comment saying
best-effort *by design*. Previously the primary and the extras were written
identically, so nothing distinguished "must not fail" from "must not abort the
spin". There is now exactly one `let _ =` of the two, and it is the one that
means it.

### Item 3 — the C blocking spins

Both loops in `nros-c/src/executor.rs` (`spin` and the fixed-period spin) now
count consecutive non-OK returns from `nros_executor_spin_some` and return the
failing code once `SPIN_ERROR_TOLERANCE` (16) is reached, instead of looping
forever and reporting OK on shutdown.

The tolerance is deliberate on both sides: not 1, because a spin can fail
transiently; not unbounded, because that was the bug.

### Receipts

- `cargo build` + `clippy` clean for `nros-node` and `nros-c`.
- Action e2e suite green through the modified drive path — `actions` (4 tests)
  plus the `action_multigoal` gate from issue 0322, all PASS with real sessions
  spinning.
- `just check` green.

### Not done

No test asserts the counter *rises*. Doing that honestly needs a session whose
transport is killed mid-spin (drop the router, then assert
`session_io_failures() > 0`), which is a fixture-level change rather than a
test-level one. The accessors make it possible for the first time; nothing
exercises them yet.
