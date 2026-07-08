---
id: 149
title: "phase-281 nuttx-realtime workspace fixtures fail from a fresh configure — generated C interface headers never materialize before the cargo kernel build"
status: resolved
type: bug
area: nuttx
related: [phase-281, issue-0136]
resolved_in: "phase-281-nuttx-restore-c-rust (f28ebc379, 2026-07-08)"
---

## Summary

The two phase-281 W3-nuttx workspace fixtures (`workspace-c-nuttx-realtime`,
`workspace-cpp-nuttx-realtime`; `examples/workspaces/ws-realtime-{c,cpp}`
nuttx lanes) fail to build from a FRESH configure — reproduced twice with the
canonical `just nuttx build-examples` after wiping
`build-workspace-fixtures-nuttx`:

```
ctrl_pkg/src/Ctrl.c:16:10: fatal error: std_msgs.h: No such file or directory
error: failed to run custom build command for `nros-nuttx-ffi v0.4.0`
```

Three symptoms, one cause — the entry carrier's `LINK_INTERFACES` walk comes
up empty for the generated interface libs:

1. `<build>/src/ctrl_pkg/nano_ros_c/std_msgs/` contains only the empty
   `action/ msg/ srv/` skeleton — the `<pkg>__nano_ros_c_gen` custom command
   never runs before the cargo kernel build (missing dependency edge).
2. `nuttx_entry_includes.txt` (the `file(GENERATE)` include closure handed to
   the FFI cc-rs build) lists only static dirs — no generated
   `nano_ros_c/...` include dir.
3. `APP_INTERFACE_SOURCES=` is empty, so the serdes TUs (`std_msgs_msg_int32.c`)
   that phase-281 W3-nuttx routes into the trailing `app_iface` archive are
   never compiled either.

`cmake/board/nano-ros-board-nuttx-qemu-arm.cmake` builds `_link_ifaces` from
`get_target_property(${target} LINK_LIBRARIES)` on the entry carrier; the
`std_msgs__nano_ros_c` lib is linked to the NODE component libs
(`ctrl_pkg`/`telem_pkg` CMakeLists), and whatever sidecar is expected to also
attach it to the entry carrier isn't doing so on a fresh configure.

This is the first NuttX workspace that uses `nros_find_interfaces` + typed C
nodes (the pre-existing `workspace-c-nuttx` chatter fixture is pure-C with no
generated serdes and builds fine), so the path had no prior coverage. The
phase-281 e2e presumably passed against incrementally-built state — same
latent-fresh-configure class as the phase-277 stale component-target names
(fixed cce254ffd).

Until fixed, `just build-test-fixtures`' staleness gate hard-fails on the two
missing fixtures on any machine that hasn't built them; `realtime_tiers_
{c,cpp}_nuttx_e2e` cannot run.

## Root cause + fix landed (2026-07-08)

The 219.J auto-link sidecar of a LAUNCH-only workspace entry links only the
`<pkg>_<exec>_component` libs; the generated interface libs
(`std_msgs__nano_ros_c`, …) hang off the COMPONENT via the 220.G.2
auto-link. `cmake/board/nano-ros-board-nuttx-qemu-arm.cmake` walked only the
ENTRY's direct `LINK_LIBRARIES`, so for workspace entries the iface libs were
invisible — codegen DAG dependency, generated include dirs, and the C serdes
TUs (`APP_INTERFACE_SOURCES`) all dropped out. Standalone examples never hit
it because they link the iface lib on the carrier manually.

Fix: the board glue's component branch now descends one level and pulls the
component's `__nano_ros_{c,cpp}` links up into `LINK_INTERFACES` (deduped).
With the fix the C lane builds past `std_msgs.h`.

Still open pending the phase-283 example/fixture rework settling: the
manifest entries for the rust + C nuttx-realtime fixtures were removed
mid-rework and the remaining cpp lane currently fails earlier with a
build-std toolchain error (`can't find crate for core`) — end-to-end
verification of `realtime_tiers_{c,cpp}_nuttx_e2e` blocks on that. The
`just nuttx build-fixtures` recipe gap (workspace lanes only in
`build-examples`) also remains.

## Repro

```
rm -rf examples/workspaces/ws-realtime-c/build-workspace-fixtures-nuttx \
       examples/workspaces/ws-realtime-cpp/build-workspace-fixtures-nuttx
just nuttx build-examples   # fails: std_msgs.h not found in Ctrl.c
```

Also note: `just nuttx build-fixtures` does NOT build the workspace lanes at
all (they live in `build-examples`) — the fixtures.toml entries have no
recipe home in the fixture verb.

## Root-cause analysis + resolution status (2026-07-08)

The fresh-configure failure was **three distinct bugs** stacked on the same
lane. Only the C++ lane is fully fixed so far; the C lane is deferred (below).

1. **Interface libs never surfaced onto the entry carrier.** A typed node links
   its generated interface lib (`<pkg>__nano_ros_c(pp)`) on the NODE, not on the
   entry carrier, so `nros_board_link_app`'s `LINK_LIBRARIES` walk came up empty
   → no dependency edge onto `<pkg>__nano_ros_c*_gen` (so `std_msgs.h`/`.hpp`
   didn't exist at cargo-build time), no include dir, no serdes. **Fixed** by
   `23b7a55e1` (descend into the node's `LINK_LIBRARIES`, surface the interface
   libs into `LINK_INTERFACES`).
2. **C++ FFI glue built under host STABLE.** The C++ interface serdes is a
   separate `nano_ros_cpp_ffi_<msg>` cargo staticlib cross-built with
   `+${Rust_TOOLCHAIN}`. In a WORKSPACE configure the standalone NuttX toolchain
   file (which pins `Rust_TOOLCHAIN` to the NuttX nightly) is not loaded, so it
   ran under host stable → `E0463: can't find crate for core` for the tier-3
   `armv7a-nuttx-eabihf` build-std. **Fixed** (`cd072b608`): the board overlay
   pins `Rust_TOOLCHAIN` to the NuttX nightly from the
   `examples/qemu-arm-nuttx/rust-toolchain.toml` SSoT.
3. **`_ffi_lib` invisible across subdirectories.** `<lib>_ffi_lib` (IMPORTED)
   was created in the node package's subdirectory scope; the entry package's
   `nros_board_link_app` runs in the ENTRY scope, where a non-GLOBAL IMPORTED
   target is invisible → `if(TARGET <lib>_ffi_lib)` false → the C++ serdes `.a`
   never reached the kernel link → `undefined reference to nros_cpp_publish_<msg>`.
   **Fixed** (`cd072b608`): `IMPORTED GLOBAL`.

With (1)+(2)+(3) the **`workspace-cpp-nuttx-realtime`** lane builds from a FRESH
configure (verified: wipe `build-workspace-fixtures-nuttx` → kernel ELF). The
`(cpp, nuttx)` matrix cell is COVERED.

### C lane — still deferred (the open coordination item)

The C typed-serdes lane needs its generated serdes `.c` (`std_msgs_msg_int32.c`,
defining `std_msgs_msg_int32_init/serialize`) **recompiled for `armv7a-nuttx`**
— the interface lib's own `.a` is host-arch (`file format not recognized`), and
there is no `<lib>_ffi_lib` for a C interface. The original c/nuttx work
(`cc9ee0811`) did this via a dedicated `INTERFACE_SOURCES` channel (board cmake)
+ an `APP_INTERFACE_SOURCES` receiving end (`nros-nuttx.cmake`) that compiled the
serdes into a trailing `app_iface` archive. The `23b7a55e1` rework **removed that
entire mechanism** (both ends now absent) and reverted the `workspace-c-nuttx-realtime`
+ `workspace-rust-nuttx-realtime` fixtures/examples/tests, moving `(c, nuttx)` and
`(rust, nuttx)` back to **DEFERRED** in the matrix gate. So on current main:

- `(cpp, nuttx)` — COVERED, builds fresh.
- `(c, nuttx)`, `(rust, nuttx)` — DEFERRED; no fresh-buildable example/fixture.

To re-COVER the C lane, the serdes-recompile mechanism (or an equivalent) must be
restored on top of the `23b7a55e1` surfacing — the surfacing gets the include dir
+ dep edge, but a C interface still has no ARM-compiled serdes without it. Left
open for the nuttx-realtime owner to reconcile (two independent fixes collided
here; not re-forced unilaterally).

## Resolved (2026-07-08)

The C-serdes-recompile mechanism was restored on top of the `23b7a55e1` surfacing
(the surfacing feeds `_link_ifaces`; the re-added block collects each C interface
lib's serdes `.c` into `INTERFACE_SOURCES` → `nros-nuttx.cmake`
`APP_INTERFACE_SOURCES` → the `nuttx_ffi_build.rs` trailing `app_iface` archive),
and the reverted c/rust nuttx examples/fixtures/tests + the Rust `run_tiers` path
were recovered (`f28ebc379`). **Both lanes now build from a FRESH configure** —
verified by wiping `build-workspace-fixtures-nuttx` and rebuilding: C → kernel ELF
with `std_msgs_msg_int32_init/serialize` defined, `realtime_tiers_c_nuttx_e2e`
green; Rust → kernel ELF, `realtime_tiers_rust_nuttx_e2e` green + a manual delivery
proof (ctrl 10 ms = 51 receives, telem 100 ms = 8, cross-process over slirp). The
`exec_model_matrix` DEFERRED set is empty — all 12 lang×platform Model-1 cells
COVERED. `just check` green.

Residual (separate, minor): `just nuttx build-fixtures` still doesn't build the
workspace lanes (they live in `build-examples`) — a fixture-verb wiring follow-up,
not a build correctness issue.
