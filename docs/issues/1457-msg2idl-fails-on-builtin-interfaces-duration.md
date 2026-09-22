---
id: 1457
title: "tier-2 nightly's zephyr module dies on `msg2idl.py failed on
  builtin_interfaces/msg/Duration.msg (exit 1)`, and the lane prints the exit code
  without the reason — a different stop from the four that preceded it"
status: open
type: bug
area: [ci, tooling, zephyr]
severity: high
found: 2026-09-22
related: [1389, 1360, 1158, 1387]
---

## What happens

Nightly run **35689793993** (schedule, 2026-09-22T05:12), job **106624135886**
(`tier 2 nightly (pairwise cover)`), step `just build tier2-nightly`. The lane
reaches its fixture build — `lane=tier2-nightly coords=35`, modules `esp32
freertos native nuttx qemu threadx_linux threadx_riscv64 zephyr` — and the
zephyr module fails:

```
== zephyr == FAILED (rc=2)
first error line(s) in …/tmp/build-test-fixtures-20260922-052701-405395/zephyr.log:
  123:  79:error: msg2idl.py failed on /tmp/tmpnus5wivz/builtin_interfaces/msg/Duration.msg (exit 1)
  124:  81:FATAL ERROR: command exited with status 1: /usr/bin/cmake --build \
        /home/runner/.nros/workspaces/zephyr/3.7/build-cpp-talker-cyclonedds
error: recipe `build-fixtures` failed with exit code 2
```

Four leaves report it (lines 123, 127, 207, 289 of the module log), all on the
same input: `builtin_interfaces/msg/Duration.msg`, a two-field message
(`int32 sec`, `uint32 nanosec`) that every other road in the tree converts
without complaint.

## Why this is a NEW stop, not one of the recorded ones

Tier 2's failures to date have all been EARLIER in the pipeline, and each has a
distinct signature this one does not match:

- **1389** — `entity-inventory schema version 6; this reader understands 3`.
- **1360** — `#error … codegen version the runtime does not accept`.
- **1158** — never reaching the cells, by provisioning or build STAGE.
- **1387** — `N cached value(s) across M build dir(s) name ANOTHER checkout`.

This run gets past all of them: `check-fast` is green, the fixture build starts,
seven modules are attempted and only `zephyr` stops. That is progress in the
lane and a new defect at the same time.

## What the log does NOT say, and why that matters

`msg2idl.py failed … (exit 1)` is a **status without a reason**. The wrapper
reports the child's exit code and drops its stderr, so the job log carries no
traceback, no missing import, no parse error — the same shape issue 1249 warns
about one layer down (`out="$(cmd)"` under `set -e`). Whatever msg2idl printed
died with the subprocess.

Two facts worth having before guessing:

1. the same run clones `rosidl` at `humble-5621b26` into the SDK store
   (`source humble-5621b26 — clone https://github.com/ros2/rosidl@5621b26…`), so
   `msg2idl.py` here is upstream's script at a pinned commit, not ours;
2. the input lives in a `/tmp/tmpnus5wivz/` staging tree the build materialises,
   so the failure may be about the staged file's surroundings (a missing
   `package.xml`, an empty parent) rather than the `.msg` content.

## What this is NOT

- **Not 1353.** No `No space left`, no truncation; the module log is written and
  quoted.
- **Not the C/C++ msg road in general.** `builtin_interfaces` is generated on
  every native leaf too, and those modules passed in this same run.
- **Not a pin move.** The `rosidl` clone is at the sha the SDK index pins.

## What would close it

The lane's zephyr module building `build-cpp-talker-cyclonedds` again. Before a
fix, one measurement: run `msg2idl.py` on that staged `Duration.msg` by hand and
capture its **stderr** — the wrapper's `exit 1` is not a diagnosis, and the next
person should not have to re-derive that. If the wrapper is ours, propagating
the child's stderr is worth doing whatever the root cause turns out to be.
