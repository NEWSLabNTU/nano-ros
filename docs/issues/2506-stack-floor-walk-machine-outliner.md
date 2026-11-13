---
id: 2506
title: "The stack walk conflates callers through the machine outliner's OUTLINED_FUNCTION_* bodies, so an opt-level=z archive reads 10-20x its real floor"
status: open
type: bug
severity: medium
area: [memory, build, px4]
related: [issue-2400, phase-557]
found: 2026-11-13
---

## What

At `opt-level = "z"` (and `"s"`) on Arm, LLVM's MachineOutliner moves repeated
instruction sequences into shared `OUTLINED_FUNCTION_<n>` bodies, which many functions
call. `scripts/nros-stack-floor.py` treats each one as an ordinary function, so any caller
of an outlined body inherits every callee of every other caller of it. The deepest path
then runs through unrelated code.

## Measured

`nros-cpp` for PX4 fmu-v6x (`thumbv7em-nuttx-eabihf`, `-Zbuild-std`, features
`alloc,log,panic-platform,rmw-cffi,platform-nuttx`, fat LTO, one codegen unit,
PX4 1.17's NuttX), walked by the placer (`--stack-floor px4-uorb`, phase-557 W3):

| profile | OPEN static walk | SPIN static walk |
| --- | ---: | ---: |
| `opt-level = 3` (release) | 7,528 B | 3,672 B |
| `opt-level = "z"` (the phase-547 spike's flash-fitting profile) | 77,240 B | 69,680 B |

The O3 numbers agree with the frames issue 2398 read directly: SPIN 3,624 B. OPEN differs
only by `open_marshalling_properties`, which issue 2401 shrank from 8,472 B to 240 B. The
`z` path starts at `nros_cpp_spin_once` and alternates through outlined bodies:
`OUTLINED_FUNCTION_62` → `nros_rmw_cffi::to_c_str` → `OUTLINED_FUNCTION_15` →
`CdrWriter::write_u32` → … → `alloc::raw_vec` → `core::num::dec2flt`. No executor pass
calls that chain.

## Impact

The floor is far too high, never too low. So the failure is a false shortfall, not an
overflow.

- An UNMEASURED image only warns. On PX4 that means a configure warning that names a
  floor of tens of KB.
- A MEASURED image would be refused at a size it does not need.

No in-tree lane builds a `z` image that is walked today. The PX4 NuttX placer is
phase-557 W3b, and the FreeRTOS and Zephyr roads build at their own profiles. So this is
latent, but the PX4 spike profile is `z`.

## Fix

Treat an outlined body as part of each caller rather than as a shared callee. The walker
can do that in two ways:

1. Inline the body's frame and its callees into each call site, but only those the call
   site's own sequence reaches. An outlined body is straight-line code ending in a return
   or a tail call, so its callees are a fixed set; the conflation comes from merging those
   sets across callers, which a per-call-site expansion avoids.
2. Build with `-C llvm-args=-enable-machine-outliner=never` when walking. This changes the
   code that ships, so the walked image would no longer be the shipped one. Rejected
   unless the ship profile adopts it too.

Option 1 also needs to handle frames: an outlined body that saves LR has a frame of its
own, which the walker adds to the caller's.
