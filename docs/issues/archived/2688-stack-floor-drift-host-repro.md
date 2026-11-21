---
id: 2688
title: "The FreeRTOS C stack-floor drift row walks 4.8 KiB deeper on a dev host than in CI, so a local drift run cannot predict the queue's"
status: resolved
type: tech-debt
severity: low
area: [tooling, stack-floor, freertos]
related: [issue-2550, phase-557]
found: 2026-11-18
---

## What happens

`just zephyr stack-floor-drift` re-walks each `[[observed]]` and `[[drift]]`
row of `config/stack-floors.toml` and fails when the walk rises by more than
2 %. For the bare-metal Rust row (`qemu_bsp_talker_bringup@mps2-an385-baremetal`)
a dev host and CI agree to the byte. For the FreeRTOS C row they do not:

| Tree | Where | Walked | Against recorded 130,424 B |
|---|---|---|---|
| main, 2026-11-17 (scheduled run) | CI container | 130,000 B | -424 B, note |
| main, same day (pre-#4183 base) | dev host 133 | 134,824 B | +4,400 B, **FAIL** |
| #4183 (merge-queue run) | CI container | 134,144 B | +3,720 B, FAIL |
| #4183 | dev host 133 | 134,736 B | +4,312 B, FAIL |

So on main the host walks about 4.8 KiB deeper than CI, and fails a row CI
passes. With #4183 applied the two land 592 B apart. The rise that ejected
#4183 was real in CI's walk; the host could not show it, because the host was
already over the threshold on main.

## Why it matters

A contributor who runs the drift check before pushing gets a red on main,
and cannot tell whether their change moved the row. #4183 had to take its
re-walked value from the queue run's log, not from a local run.

## What is known

- Not the Rust side alone: the bare-metal Rust row reproduces exactly, with
  the same pinned rustc and `llvm-readobj`.
- The C row's frames come from GCC `-fstack-usage` `.su` files (zenoh-pico,
  FreeRTOS, the app), plus `.stack_sizes` for the linked Rust staticlib. The
  host's GCC is the store's `arm-none-eabi-gcc` 13.2.rel1 (`13.2-nros1`).
- The walk binds indirect calls by type, so a different set of functions in
  the image (a different libc, a different `-ffunction-sections` survivor
  set) can change the deepest path without any frame growing. That is what
  happened on #4183 in both places.
- The likeliest mechanism is issue 2550's. The C talker's call graph has one
  strongly connected component of about 2,000 functions, and the walk caches
  a depth per function, so where the cache cuts that cycle depends on the
  callee order. The order is fixed per graph, but two images whose function
  sets differ (host and CI) get different cuts. The same effect moves the row
  by kilobytes between unrelated pull requests: #4183 raised it 3,720 B with
  no frame growing, and #4188 lowered it 3,848 B. The fix belongs with 2550:
  compare against the deepest result over many callee orders, fail only on a
  rise, and report a fall as a note.

## Next step

Diff the two `report.json` files (CI writes it under
`build/stack-floor-drift/drift_talker_c/.../cmake/nros/stack-floor/report.json`;
upload it as a job artifact). The deepest path and the per-function frame
table say which input differs: a frame from a `.su` file (compiler or flags),
a function present in one image only (sysroot or link set), or an indirect
binding (the type-bound fold). Until then, a local drift verdict on this row
is not evidence about CI's.

## Resolution (2026-11-22)

**It no longer reproduces, and its cause can no longer pass silently.**

- **Measured at the same commit, it matches to the byte.** CI's last drift
  run (2026-11-20, `b28c0fbaca`) walked the FreeRTOS C row at 112,064 B and
  the nucleo C row at 112,184 B. This dev host, at the same commit, walked
  112,064 B and 112,184 B.
- **The cause was the toolchain, not the code.** When this was filed, the
  host's store GCC was `13.2-nros1`, older than CI's. Both now resolve the
  pinned `13.2-nros5`. Phase-557 W9c's checkpoint also showed that the walk
  no longer moves with unrelated code (0.00 % across six trees), so a host
  and CI on the same toolchain cannot disagree for that reason either.
- **The check now names a toolchain mismatch.** `stack-floor-drift.py`
  reads each image's `CMakeCache.txt` and compares what the build resolved
  with the tree's pins:
  - the C compiler's store version, against `nros-sdk-index.toml`;
  - the Zephyr SDK's store version, against the same index;
  - the Zephyr tree's tag, against its west manifest.

  A mismatch makes the row NOT VERIFIED (the skip ledger), naming what was
  found and what is pinned, and the walk's verdict is withheld, PASS or
  FAIL. A walk under another toolchain says nothing about the row. Every row
  also prints the toolchain it was walked under.
- **This host is an instance.** Its shared Zephyr 3.7 workspace is at
  v3.7.0, where `west.yml` pins v3.7.2. Zephyr's own `SDK_VERSION` follows
  the tree, so its builds take SDK 0.16.8 where the index pins 0.16.9. The
  nucleo C row reads NOT VERIFIED here, naming both. The walk happened to
  agree with CI's this time (the two SDKs' libgcc have the same code), but
  that is not something a check can assume.
