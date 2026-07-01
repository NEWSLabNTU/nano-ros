---
id: 125
title: "NuttX Rust `*_entry` demos can't be build-asserted as fixtures — standalone `[[bin]]` link fails on unresolved libc/syscall symbols"
status: open
type: tech-debt
area: testing
related: [phase-275, rfc-0026]
---

## Summary

The six `examples/qemu-arm-nuttx/rust/{role}_entry` demos are the last uncovered
slice of Phase 275 W1 (#102 H2): they ship with no fixture and no test. Adding
`examples/fixtures.toml` `[[fixture]]` rows (mirroring the role examples + the
now-landed threadx-linux entry rows) does **not** work — the `armv7a-nuttx-eabihf`
cross build compiles but **fails to link** the final `[[bin]]`:

```
undefined reference to `write'
undefined reference to `clock_gettime'
undefined reference to `__errno'
undefined reference to `exit'
```

(from `std`'s `sys::stdio::unix`, `time::Instant::now`, `sys::exit::exit`, …)

The role examples in the same platform build fine, so this is specific to how the
Entry-pkg `[[bin]]`s are linked, not a missing SDK.

## Two distinct blockers found (Phase 275 investigation, 2026-07-02)

1. **Duplicate `[patch.crates-io]` → invalid TOML (fixable).** `nros ws sync`
   renders a `[patch.crates-io]` table for the Entry pkg's generated msg crates,
   then `scripts/build/nuttx-libc-patch.sh` appends a **second**
   `[patch.crates-io]` header for the build-std libc fork → `could not parse TOML
   configuration in .cargo/config.toml`. (Role examples don't retain a
   sync-rendered patch table, so they only ever get one header.) A localized fix
   works — insert the `libc` line under the existing table via `awk` instead of a
   new header — but it is **not landed** here because blocker (2) makes it
   unexercised (nothing builds to prove it). Re-apply that awk-insert fix
   alongside the fix for (2).

2. **Standalone `[[bin]]` link vs NuttX libc (the real blocker).** Even with a
   valid config, `cargo build --release` produces a fully-linked ELF that
   requires every libc/syscall symbol resolved at link time. NuttX apps resolve
   `write`/`clock_gettime`/`__errno`/`exit` from the NuttX kernel's libc, which
   the role-example link path supplies but the Entry-pkg standalone link does
   not. Needs the Entry-pkg link to either (a) link against the NuttX staging
   libc the same way the role examples do, or (b) emit a relocatable object the
   NuttX app-build stage links later — a design decision, not a mechanical row.

## Status in Phase 275 W1

- freertos entry demos: built+run by `freertos_run_plan_runtime.rs`.
- threadx-linux entry demos: **landed** — `[[fixture]]` rows +
  `tests/threadx_linux_entry_build.rs` (host build).
- **nuttx entry demos: blocked here.** Tracked as exceptions in the W6 gate
  (`tests/examples_fixture_coverage.rs` `ALLOWLIST`) so they are not a silent gap.

## Fix direction

Resolve (2) first (Entry-pkg NuttX libc link), then re-apply the (1) awk-insert
fix to `nuttx-libc-patch.sh`, add the 6 `[[fixture]]` rows + a `nuttx_entry_build.rs`
build-assert test (mirror `threadx_linux_entry_build.rs`), and drop the 6 nuttx
entries from the W6 gate ALLOWLIST.
