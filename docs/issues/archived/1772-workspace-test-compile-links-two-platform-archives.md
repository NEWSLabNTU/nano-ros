---
id: 1772
title: "`check workspace-features` fails to link `nros-board-threadx`'s lib test: the POSIX and ThreadX platform archives both define `nros_platform_*`"
status: resolved
type: bug
area: build
severity: medium
found: 2026-10-10
related: [issue-1732, issue-1309, issue-1779]
resolved_in: "one nros_platform_* provider per linked graph: boards ask nros-platform-cffi first (issue 1779)"
---

## Symptom

`just ci gate`, step `check::build`, gate `workspace-features`, on a tree based
on `origin/main` `2b81536214`:

```
cargo test --no-run --workspace --exclude nros-c --no-default-features --quiet
error: linking with `cc` failed: exit status: 1
  rust-lld: error: duplicate symbol: nros_platform_clock_ns
  >>> defined at platform.c:57 (packages/platform/nros-platform-threadx/src/platform.c:57)
  >>>   ... in archive .../nros-board-threadx-*/out/libnros_platform_threadx.a
  >>> defined at platform.c:43 (../nros-platform-posix/src/platform.c:43)
error: could not compile `nros-board-threadx` (lib test)
```

The same for `nros_platform_clock_resolution_ns`, `_epoch_us`, `_alloc`,
`_realloc`, `_dealloc`, `_heap_used_bytes`, `_heap_total_bytes`, `_sleep_us`
and `_sleep_ms`, until lld stops counting.

The same gate was green on 2026-10-08 (phase-482 W3's `ci gate`, 12m10s).

## Likely cause (not yet measured)

Issue 1732's fix (`0e01ffb4d4`) added `nros_posix_install_termination_guard`
and `nros_posix_termination_requested` to
`packages/platform/nros-platform-posix/src/platform.c`, reached from the new
`nros_platform::termination` module. A static archive member is pulled into a
link when any of its symbols is referenced. If `--workspace` feature
unification puts the POSIX platform archive on the board-threadx lib-test link
line, the new reference now pulls in `platform.o` from that archive, and every
`nros_platform_*` it defines collides with the ThreadX `platform.o`. Before
`0e01ffb4d4`, nothing on that line referenced that member, so it was never
pulled.

To confirm: check out `0e01ffb4d4^` and run the same `cargo test --no-run`.

## Not caused by

phase-483 W1. That branch changes no Rust code in either platform crate, so
its build of these crates is byte-identical to main's.

## Resolution

Resolved together with issue 1779, which is the same defect filed the same day. The mechanism this issue predicted is the one measured there: issue 1732 put the termination guard in the POSIX `platform.o`, and that member is now pulled into a binary that also whole-archives the ThreadX port. See `archived/1779-*` for the fix, the gate and the mutation checks.
