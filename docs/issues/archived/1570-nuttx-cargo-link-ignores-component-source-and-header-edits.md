---
id: 1570
title: "A NuttX C/C++ image does not rebuild when a component source or any header it includes changes — nuttx_ffi_build watches the source LIST, not the sources"
status: resolved
type: bug
area: build, nuttx
severity: high
found: 2026-09-29
resolved: 2026-09-29
related: [issue-0475, issue-1568, issue-1569, issue-0196]
---

# The NuttX cargo link has no edge to the component sources it compiles

## Symptom (measured)

Issue 1568 raised `NROS_PUBLISHER_SIZE` in the committed NuttX snapshot
(`nros_cpp_config_generated_nuttx.h`) and rebuilt the realtime-c
qemu-armv7a-nuttx fixture incrementally
(`NROS_FIXTURE_ID=workspace-c-nuttx-realtime just nuttx build-fixtures-arm`).
The build reported success; the image did not change:

- cmake rebuilt `pkg/ctrl_pkg/.../Ctrl.c.obj` (its `.d` names the snapshot)
  and `libctrl_lib.a`; `nm -S` on that object shows `__nros_c_inst_ctrl_pkg`
  at the NEW size, `0x288`;
- cargo said `Finished ... in 0.42s` and `Copying nuttx_entry`; `nm -S` on
  the image still shows `__nros_c_inst_ctrl_pkg` at the OLD size, `0x238`.

Touching the generated entry TU (`APP_MAIN_CPP`, the one source the build
script does watch) and rebuilding produced `0x288` in the image.

## Cause

The image is linked by the cargo `nros-nuttx-ffi` build, and
`packages/boards/nros-board-common/src/nuttx_ffi_build.rs` compiles the
component sources ITSELF from `APP_EXTRA_SOURCES` / `APP_INTERFACE_SOURCES`
(the cmake `ctrl_lib` archive is not what gets linked —
`nuttx_entry_ffi_libs.txt` is empty). For those it emits only
`rerun-if-env-changed=APP_EXTRA_SOURCES` — the LIST — and nothing for the
files in it or for any header they include (cc-rs emits no header deps). So
an edit to a component `.c`, to `component.h`, to the NuttX snapshot or to
any other included header leaves cargo's fingerprint unchanged and the image
is a museum binary: CLAUDE.md's issue-0475 class, one lane over.

It is load-bearing for 1568/1569 in particular: a fix to a NuttX storage
size never reaches an incrementally built image.

## Fix direction

Emit `cargo:rerun-if-changed=` for every file in `APP_EXTRA_SOURCES` /
`APP_INTERFACE_SOURCES`, and for their headers — either by compiling with
`-MMD` into OUT_DIR and replaying the `.d` files as `rerun-if-changed` on
the next run, or by having cmake hand over a depfile it already computes.
Acceptance is a BUILD: edit a header one component includes, rebuild
incrementally, and see the symbol change in the image.

## Resolution

Two layers were blind, and the reproduction on
`examples/workspaces/realtime-c` (qemu-armv7a-nuttx, cmake's Unix Makefiles
generator) showed each separately:

- **Header edit** (`NROS_PUBLISHER_SIZE` 640 → 720 in the committed
  `nros_cpp_config_generated_nuttx.h`): cmake rebuilt `Ctrl.c.obj`
  (`__nros_c_inst_ctrl_pkg` 0x288 → 0x2d8) and **never re-ran cargo** — the
  custom command's `DEPFILE` is cargo's dep-info for `nros-nuttx-ffi`, and that
  file (352 entries) named no C header at all. Image stayed 0x288, build green.
- **Source touch** (`Ctrl.c`, which IS in the rule's `DEPENDS`): make re-ran
  cargo, cargo found the ffi build script fresh, the artifact's mtime did not
  move.

The fix is one mechanism for both layers. `nros_cc_flags::header_deps`:
`track_header_deps(&mut build)` adds `-MMD`, so the compiler writes a depfile
beside each object; `emit_header_deps(out_dir)` replays every name in them as
`cargo:rerun-if-changed` after the compile and deletes them (so a source
dropped from the list cannot leave a stale `.d` declaring a missing path — the
permanent-rerun treadmill). Cargo copies build-script `rerun-if-changed` paths
into the artifact's dep-info, so the same list becomes the cmake `DEPFILE`
edge that re-runs cargo at all: 352 → 491 entries, including `Ctrl.c`,
`component.h` and the snapshot. Applied to every compile in `run_nuttx` (arm
and riscv ffi crates share it) and to the two platform-port compiles in the
same lane (`run_platform`, `compile_entry_seams` — the platform-api headers
`platform.c` includes were unwatched too). Each `APP_FFI_LIBS_FILE` archive
also gets `rerun-if-changed`: cargo does not fingerprint a `rustc-link-lib`.

Measured after the fix, same leaf, incremental throughout:

| step | cargo ran? | image `__nros_c_inst_ctrl_pkg` |
| --- | --- | --- |
| no-op | no | 0x2d8 (artifact mtime unchanged) |
| snapshot 720 → 640 | yes, only `nros-nuttx-ffi` recompiled (8 s) | **0x288** |
| `touch Ctrl.c` | yes, relinked (6 s) | 0x288, new mtime |
| `touch component.h` | yes, relinked (9 s) | 0x288, new mtime |
| no-op | no | unchanged |

The NuttX headers now declared all come from the per-arch export snapshot,
plus the live `include/nuttx/config.h` that `nuttx_include_root` already
watched on purpose (issue 0477's rule) — so no new cross-arch rerun source.

**Sweep.** `git grep -n 'env::var("[A-Z_]*SOURCES' -- '*.rs'` matches only
`nuttx_ffi_build.rs`: NuttX is the one family whose image links component TUs
through cargo. FreeRTOS, ThreadX (both boards) and Zephyr link them with cmake
`add_executable`, whose objects carry the compiler's own `.d` edges (and
0475's `LINK_DEPENDS` for the whole-archived backends).

**Gate.** `just check cc-header-deps` (fast lane): a Rust source that compiles
with cc-rs from an env `*SOURCES*` list must call both helpers, and neither
helper may appear without the other; fails if it matches nothing, self-tests
its classifier each run, and was mutation-checked against the real file.

**Residual, not this issue.** The board crates' own cc-rs compiles (FreeRTOS
kernel/lwIP/glue, ThreadX kernel/NetX/glue, threadx-linux, mps2 lan9118)
compile IN-CRATE or vendored sources and watch them by hand — per file or by
directory — with the same blindness to headers outside those paths. That is
the general cc-rs shape rather than a lane linking the image, and the helper
is now there for them.
