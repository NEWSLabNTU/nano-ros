# Phase 146 — Zenoh-Pico Embedded Link Regressions

**Goal.** Fix the link failures surfaced when building Rust examples
against the embedded zenoh-pico variants (FreeRTOS / NuttX /
ThreadX-Linux). All three regressions are **pre-existing on
`main`** (verified by `git stash` revert during Phase 136
verification on 2026-05-18); none were introduced by the 136
manifest-driven cc-rs collapse.

**Status.** Not started.

**Priority.** P1 — these failures block every Rust example build on
the affected RTOS targets. Hosted-RTOS QEMU E2E test runs
(`just freertos test`, `just nuttx test`, `just threadx_linux test`)
inherit the failure. Phase 127.G.3's deferred `just test-all`
refresh will trip on them.

**Depends on.** None blocking. Coordinates with the 137-140
build-system refactor — if `install-local` goes away, the link-layer
wiring around `_z_*_serial_internal` and `platform_aliases.c` may
need to be re-traced through the new entry path.

**Related.** Phase 134 (canonical `zenoh_config.h`), Phase 129.D
(platform-aliases.c default-on), Phase 132 (CMSDK UART
IRQ-driven), Phase 136 (manifest-driven cc-rs collapse — surfaced
these by trying to build embedded examples).

---

## Symptoms

### A — `_z_task_free` duplicate on ThreadX-Linux

```
rust-lld: error: duplicate symbol: _z_task_free
>>> defined at task.c
>>>            ac47caef935fb3b8-task.o:(_z_task_free) in archive
>>>            libzpico_sys-330e89a9d7d9e6ae.rlib
>>> defined at platform_aliases.c
>>>            363bfc8599a580b6-platform_aliases.o:(.text._z_task_free+0x0)
>>>            in archive libzpico_sys-330e89a9d7d9e6ae.rlib
```

**Reproducer.** `cd examples/threadx-linux/rust/zenoh/talker &&
cargo build --release`.

**Source.** Both translation units provide the symbol:
- `packages/zpico/zpico-sys/c/platform/threadx/task.c` — ThreadX-
  specific task lifecycle (kept in C because it needs `_z_task_t`
  struct layout = `TX_THREAD` + embedded stack + function/arg
  pointers).
- `packages/zpico/zpico-sys/c/zpico/platform_aliases.c` line 181 —
  always defines `_z_task_free` (the alias-TU forwards to
  `nros_platform_task_free`).

Per CLAUDE.md, the Phase 129.D alias TU is default-on:
> `default = ["platform-aliases"]`. The C alias TU
> (`platform_aliases.c`) now covers the full `z_*` / `_z_*`
> surface — memory, sleep, random, time, yield, threading,
> condvar, clock, network — that `zpico-platform-shim` used to
> provide.

ThreadX `task.c` was kept out of `platform_aliases.c` because of
the struct-layout dependency, so the alias TU's `_z_task_free`
collides. Two viable fixes:

1. Make `platform_aliases.c` skip `_z_task_*` symbols when the
   `threadx` feature is on (mirrors `nuttx_clock.c`'s
   `skip-clock-symbols` carve-out pattern).
2. Delete `c/platform/threadx/task.c` and inline its body into a
   ThreadX branch inside `platform_aliases.c`.

Option 1 is the smaller surface change; option 2 fits the unified-
alias direction Phase 129.D set.

### B — `_z_*_serial_internal` undefined on FreeRTOS QEMU

```
rust-lld: error: undefined symbol: _z_read_serial_internal
rust-lld: error: undefined symbol: _z_send_serial_internal
rust-lld: error: undefined symbol: _z_open_serial_from_pins
rust-lld: error: undefined symbol: _z_open_serial_from_dev
rust-lld: error: undefined symbol: _z_close_serial
rust-lld: error: undefined symbol: _z_listen_serial_from_pins
rust-lld: error: undefined symbol: _z_listen_serial_from_dev
```

**Reproducer.** `cd examples/qemu-arm-freertos/rust/zenoh/talker &&
cargo build --release`.

**Source.** Phase 128.E.1 made `serial = true` unconditional in
`LinkFeatures::from_env()` because "the vendor C sources have no
external build-host dependency and the wire format isn't selected
until session-open consults the locator string". Side effect:
`zenoh-pico/src/system/common/serial.c` now compiles for every
target and calls `_z_*_serial_internal` regardless of whether the
target ships a serial backend.

FreeRTOS + lwIP does not. The serial backend for bare-metal is
`zpico-serial` (Phase 132's CMSDK UART crate); on FreeRTOS the
serial wrappers expect lwIP-side glue that was never written.

Three viable fixes:

1. Add empty `_z_*_serial_internal` stubs to `platform_aliases.c`
   under a "no-serial" platform marker.
2. Roll back the Phase 128.E.1 "serial always on" for FreeRTOS
   only — re-gate `serial` behind `CARGO_FEATURE_LINK_SERIAL`
   when the FreeRTOS feature is selected.
3. Wire a FreeRTOS-lwIP serial-over-UART implementation via the
   board crate (only useful if any FreeRTOS user actually wants
   serial; none do today).

Option 1 (stub) is the cheapest; option 2 (re-gate) is the most
correct.

### C — `_z_*_serial_internal` undefined on NuttX

Same shape as B; reproducer `cd examples/qemu-arm-nuttx/rust/
zenoh/talker && cargo build --release`. Same set of undefined
symbols. Same root cause (serial backend not wired for NuttX).
Same fix options.

---

## Verification approach

- For each fix candidate, run `cargo build --release` against the
  matching example (talker + listener + service-server +
  service-client + action-server + action-client per RTOS).
- After Cargo build clean, run the matching RTOS smoke test under
  QEMU (`just freertos test --rerun-failed` etc.) to catch
  runtime regressions.
- Add a permanent regression gate: `cargo build` of one example
  per affected RTOS lands in `just ci` so the next link-symbol
  drift surfaces immediately rather than during `test-all`.

---

## Work Items

- [ ] **146.1 — Symptom A: `_z_task_free` duplicate on ThreadX-Linux.**
      Decide option 1 (skip carve-out in `platform_aliases.c`) vs
      option 2 (fold `task.c` into the alias TU). Land + verify
      `cargo build` of `threadx-linux` Rust examples.
      **Files.** `packages/zpico/zpico-sys/c/zpico/platform_aliases.c`,
      possibly `packages/zpico/zpico-sys/c/platform/threadx/task.c`.

- [ ] **146.2 — Symptom B/C: `_z_*_serial_internal` undefined.**
      Decide option 1 (stub) vs option 2 (re-gate `serial = true`
      per-platform). Apply across FreeRTOS, NuttX, and any other
      target that surfaces the same set on its next build. Land
      + verify `cargo build` of each affected RTOS's Rust talker
      + listener.
      **Files.** `packages/zpico/zpico-sys/build/policy.rs`
      (`LinkFeatures::from_env`) or
      `packages/zpico/zpico-sys/c/zpico/platform_aliases.c`,
      depending on option choice.

- [ ] **146.3 — CI gate.** Add `cargo build` of one example per
      affected RTOS to `just ci` so the next regression of this
      shape fires immediately.
      **Files.** `justfile`, `just/*.just`.

- [ ] **146.4 — Phase 127.G.3 cross-link.** Once 146.1–146.3 land,
      the deferred `just test-all` refresh in Phase 127.G.3 can run;
      update that closeout pointer.

---

## Notes

- All three regressions are pre-existing on `main`. They surfaced
  during Phase 136.4's verification pass — Phase 136 did not touch
  the linker wiring (only collapsed the cc-rs source-selection
  layer), and the failures reproduce identically on the immediate
  parent commit via `git stash`. Do NOT blame the manifest-driven
  collapse.
- Phase 132 (CMSDK UART IRQ-driven) is the right home for the
  bare-metal-serial story; Phase 146 stays focused on the
  hosted-RTOS link-symbol gap.
- Symptom A's ThreadX carve-out is the same shape as the existing
  NuttX `skip-clock-symbols` pattern (see
  `packages/zpico/zpico-platform-shim/Cargo.toml`); reuse that
  approach if option 1 wins.
