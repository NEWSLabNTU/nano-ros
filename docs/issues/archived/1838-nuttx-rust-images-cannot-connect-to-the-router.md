---
id: 1838
title: "NuttX Rust e2e images fail `Executor::open` with `ConnectionFailed`; the C and C++ images on the same lane connect"
status: resolved
type: bug
area: [nuttx, zenoh, testing]
severity: high
found: 2026-10-12
related: [1811, 1836]
---

## Measured

The nightly `nuttx` job, step `Test / e2e (nuttx)`, fails the three Rust
cells of `rtos_e2e` and passes the C and C++ ones:

```
FAIL test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_1_Lang__Rust
FAIL test_rtos_pubsub_e2e::platform_2_Platform__Nuttx::lang_1_Lang__Rust
FAIL test_rtos_service_e2e::platform_2_Platform__Nuttx::lang_1_Lang__Rust
Summary 9 tests run: 6 passed, 3 failed
```

Each Rust image boots under `qemu-system-arm -M virt -cpu cortex-a7` with
`-netdev user`, and its session open gives up about 9 s in:

```
[ERROR] nros: [    8.901000] zpico Session -> ConnectionFailed (the two are indistinguishable downstream)
[ERROR] nros: [    8.933000] RMW session open failed — ConnectionFailed
Executor::open failed: Transport(ConnectionFailed)
```

The same three failures, with the same text, appear in:

- the scheduled nightly on `main`, run 38218729431 (`ba86ca6cf5`);
- a dispatched nightly on `fix/1811-sources-from-store`, run 38215925408.
  That branch touches no NuttX or zenoh code.

## Window

- Last green `nuttx` job: run 38120811245, 2026-10-11 07:16 UTC, at `90159b5d9e`.
- First red: the next scheduled run, 38218729431, 2026-10-12 07:36 UTC.
- `git log 90159b5d9e..ba86ca6cf5` has 171 commits.

## Ruled out

- **`96a880b1c0` (issue 1835)** adds a process-wide open lock to
  `Context::new` and `Context::with_config`. A NuttX image opens one session
  on one thread, so the lock is never contended. It cannot turn a connect
  into `ConnectionFailed`.
- **`c3eb63a741` (phase-484 W4b, `[board.net]`)** changes only the two
  ThreadX boards.

## Suspects, not yet checked

These are the phase-484 commits in the window that move a knob or a board fact
on the Rust road:

- W5a's knob ladder and registry changes (`c5da4b6cc4`, `84e2cf12e5`,
  `23de8cb964`, `e0e6c79d47`, `728aedf62d`). A locator, IP or gateway knob that
  the Rust NuttX build reads under a renamed spelling would fall back to a
  default that QEMU's user network does not route.
- W2d (`f039374550`): `activate.sh` exports no SDK root any more.

The C images pass on the same runner and the same QEMU, so the router and the
user network are up. The difference is in what the Rust build bakes in.

## Next step

Build one Rust NuttX fixture (`listener`) at `90159b5d9e` and at
`ba86ca6cf5`, then diff the locator and network constants each one embeds
(`strings` on the ELF, or the `nros_config_generated.h` and `zpico` config
used by the build) before bisecting.

## Resolution: a duplicate of issue 1836

Issue 1836 found the cause in parallel and fixed it in `a974d3f13c`.
`nros-board-nuttx` read only the build-time `NROS_LOCATOR` and never applied
the image's `nros::main!` deploy overlay. The board's empty default therefore
won, `_z_open` saw no connect key, and the session failed to open before it
sent a single packet. The C and C++ images carry their locator by another
road, which is why only Rust failed. Phase-484 W4e had dropped the NuttX
rows' env copy of the locator, and that is what exposed the bug.

The suspect list above was wrong. The W5a knob commits and `f039374550` are
not the cause.

Verified by the dispatched nightly 38251102237 on `main` (`199f34afbd`).
The `nuttx` job's `Test / e2e (nuttx)` ran 9 tests and all 9 passed,
including all three Rust cells that failed here:
`test_rtos_{action,pubsub,service}_e2e::platform_2_Platform__Nuttx::lang_1_Lang__Rust`.
