---
id: 1570
title: "A NuttX C/C++ image does not rebuild when a component source or any header it includes changes — nuttx_ffi_build watches the source LIST, not the sources"
status: open
type: bug
area: build, nuttx
severity: high
found: 2026-09-29
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
