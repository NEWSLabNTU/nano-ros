---
id: 2506
title: "The stack walk conflates callers through the machine outliner's OUTLINED_FUNCTION_* bodies, so an opt-level=z archive reads 10-20x its real floor"
status: resolved
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

## Resolution (2026-11-19)

The diagnosis above was wrong about the mechanism. An outlined body's callees
ARE a fixed set (the outliner only folds identical sequences, the tail call
included), so one module's `OUTLINED_FUNCTION_15` gives every caller the same
callees, and that is right. The conflation came from the NAME: the outliner
numbers its bodies from 0 in every module it runs on, they are LOCAL symbols,
and the walker keyed every function by bare name. So `OUTLINED_FUNCTION_15` of
one module and `OUTLINED_FUNCTION_15` of another, different code, became one
node, and a root of the first read the callees of the second.

Fix: `load_object` keys a LOCAL function by its codegen unit (a Rust unit is
one identity whether rustc's own object or llc's object from its bitcode
holds it; any other object is its own), and `separate_locals` keeps apart
only the outliner's bodies whose name another unit or a global also uses.
Every other local is walked by name, as before.

That second half is deliberate, and measured. A first version split every
local of a shared name. On the nucleo Zephyr rows it raised the walks
12-14 %: zenoh-pico's header `static inline` helpers (`_z_slice_clear` in 13
translation units) became 13 nodes, distinct steps a type-bound indirect
chain takes before it repeats, where one name is a cycle the walk cuts at
once. Source-named copies are almost always one body, so merging them is the
better answer; two unrelated C statics of one name still merge, as they
always did, which over-counts.

Option 2 above (no outliner while walking) was never needed.

### Measured

The mps2-an385 bare-metal talker
(`qemu_bsp_talker_bringup@mps2-an385-baremetal`) built at opt-level `z`, each
walker on one clean build:

| walker | OPEN path |
| --- | ---: |
| before | 199,968 B (through 20 outlined bodies of different modules) |
| after | 92,032 B |

Its runtime peak at `z`, with the board's MPU stack guard under QEMU against
a local router (about 49 publishes in 50 s per run): 24, 26 and 27 KiB trip
`nros: stack overflow` (27 KiB in 3 of 3 runs), and 28 KiB runs clean in 3 of
3. So the peak is at most 28,672 B, which the walk covers 3.2x.

Every recorded row, re-walked by `just zephyr stack-floor-drift` with each
walker on the same build: no row moves.

| row | before | after |
| --- | ---: | ---: |
| `qemu_bsp_talker_bringup@mps2-an385-baremetal` | 98,628 B | 98,628 B |
| `talker/c/zenoh@mps2-an385-freertos` | 113,120 B | 113,120 B |
| `talker/rust/zenoh@nucleo-f767zi-zephyr` | 65,080 B | 65,080 B |
| `action-server/rust/zenoh@nucleo-f767zi-zephyr` | 67,888 B | 67,888 B |
| `talker/c/zenoh@nucleo-f767zi-zephyr` | 112,556 B | 112,556 B |

The nucleo C row is 4.6 % over its recorded 107,652 B on main already; that
is not this change.

Self-test: two modules' outlined bodies stay apart (the walk reads 48 B, not
4,048 B); a local only one unit uses reads bare; an outlined body is kept
apart when a global has its name or an input calls it by name; a
source-named static in two objects is one node; a Rust unit is one identity
native or llc-compiled. Mutation-checked: never splitting fails four;
splitting every local fails three.
