# Phase 166 — Duplicate `nros_platform_*` symbols across FreeRTOS board crates

**Goal.** Eliminate the duplicate-symbol linker error when both
`nros-board-freertos` (common FreeRTOS overlay) and a board-specific
overlay (`nros-board-mps2-an385-freertos`, eventual STM32F4 / NXP /
TI variants) are pulled into the same binary. The `platform.c` C
body that exports the canonical `nros_platform_*` ABI is being
compiled twice with non-weak linkage.

**Status.** Not Started.

**Priority.** P1 — blocks `just freertos build-fixtures` for the
Rust DDS example today, and will block every future board crate
that layers on top of `nros-board-freertos`.

**Depends on.** Nothing.

---

## Symptom

```
$ just freertos build-fixtures
…
rust-lld: error: duplicate symbol: nros_platform_clock_ms
>>> defined at platform.c
>>>            91eeb4584ba792b3-platform.o:(nros_platform_clock_ms)
       in archive .../libnros_board_mps2_an385_freertos-…rlib
>>> defined at platform.c
>>>            91eeb4584ba792b3-platform.o:(nros_platform_clock_ms)
       in archive .../libnros_board_freertos-…rlib

rust-lld: error: duplicate symbol: nros_platform_task_init
>>> …
… (~20 more `nros_platform_*` symbols)
…
error: could not compile `qemu-freertos-dds-listener`
       (bin "qemu-freertos-dds-listener")
```

## Root cause

Both crates compile `packages/core/nros-platform-freertos/src/platform.c`
into their respective rlibs. Each definition exports the
`nros_platform_*` C ABI without `weak` linkage, so when the linker
walks both rlibs it sees two strong defs for every function.

- `packages/boards/nros-board-freertos/build.rs` invokes the
  platform compile.
- `packages/boards/nros-board-mps2-an385-freertos/` depends on
  `nros-board-freertos` AND also compiles `platform.c` (via its
  Cargo dep on `nros-platform-freertos` whose `build.rs` rebuilds
  it).

End state: two strong defs reach the linker, rust-lld refuses to
pick one.

## Fix options

Pick exactly one:

1. **Only the board-specific overlay emits the C body.**
   `nros-board-freertos` stops compiling `platform.c`; the
   board-specific crate that depends on it picks the compile up.
   Pro: minimal, board-specific build steps already exist.
   Con: every future board crate has to remember to compile it.

2. **Only `nros-platform-freertos` emits the C body.**
   `nros-board-freertos` stops compiling `platform.c` AND
   `nros-board-mps2-an385-freertos` does NOT add it either.
   `nros-platform-freertos`'s own `build.rs` produces a staticlib
   that every consumer links against.
   Pro: canonical — one platform crate, one C body.
   Con: needs build-script reshape to actually emit a staticlib,
   not just a `cc::Build` invocation.

3. **Gate `platform.c` emission behind a Cargo feature.**
   `nros-platform-freertos` exposes `emit-c-port` (default `on`).
   Board overlays that want to emit themselves opt out via
   `default-features = false`.
   Pro: flexible — opt-in / opt-out per consumer.
   Con: another feature axis to remember; misconfiguration leaves
   `undefined-symbol` instead of `duplicate-symbol`.

Recommend option **2** (canonical platform crate emits, board crates
consume). Matches the platform-cffi pattern documented in
`book/src/internals/platform-c-abi.md`.

## Work items

- [ ] **166.1** Audit which crates currently compile `platform.c`:
      - `nros-platform-freertos/build.rs` or CMakeLists.txt
      - `nros-board-freertos/build.rs`
      - `nros-board-mps2-an385-freertos/build.rs` (or Cargo.toml
        feature inheritance)
- [ ] **166.2** Pick the fix option (recommend option 2). Land the
      build-script reshape on a feature branch.
- [ ] **166.3** Verify with `just freertos build-fixtures` that
      both Rust zenoh + Rust DDS examples build cleanly. Verify
      C / C++ examples still link.
- [ ] **166.4** Repeat for any other RTOS that has a board-overlay
      pattern (Zephyr `nros-board-fvp-aemv8r-smp` over a generic
      Zephyr platform crate, NuttX QEMU board crates, etc.). Audit
      whether the same dup-symbol risk exists.
- [ ] **166.5** Regression test in `nros-tests`: build every
      board crate against every supported example tree as part of
      `just test-all`, asserting clean link.

## Files (likely touched)

- `packages/core/nros-platform-freertos/build.rs` (new — turn
  current `cc::Build` into a staticlib emission)
- `packages/boards/nros-board-freertos/build.rs` (drop the
  platform.c compile)
- `packages/boards/nros-board-mps2-an385-freertos/Cargo.toml`
  (confirm it pulls in the staticlib via the platform crate, not
  via its own compile)

## Acceptance criteria

- [ ] `just freertos build-fixtures` runs clean — all 20 binaries
      (rust + c + cpp × {pubsub, service, action} + DDS pair) build.
- [ ] `cargo build` from each `examples/qemu-arm-freertos/<lang>/<rmw>/<ex>/`
      links without `duplicate symbol` errors.
- [ ] `nm libnros_board_mps2_an385_freertos-*.rlib` shows zero
      `nros_platform_*` symbols defined in the rlib (they should
      resolve from the platform crate's staticlib).
- [ ] No regression on other RTOS targets that exercise the same
      board-overlay pattern.

## Notes

- This regression most likely landed during one of the Phase
  121/129 platform-cffi reshape commits. Surfaced by the user-as-
  tester agent against the FreeRTOS starter on 2026-05-19.
- The symptom is masked when only ONE board crate participates
  in a binary (the Rust zenoh examples that bypass
  `nros-board-freertos` still build cleanly because they only
  pull in `nros-board-mps2-an385-freertos`).
