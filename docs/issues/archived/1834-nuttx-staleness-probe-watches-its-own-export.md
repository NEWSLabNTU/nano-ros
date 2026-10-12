---
id: 1834
title: "A NuttX kernel build re-stales every other arch's fixtures, because `locate` watches the whole in-place tree"
status: resolved
type: bug
area: [build, testing, nuttx]
severity: medium
found: 2026-10-12
related: [0491, 0525, 1767, phase-484]
---

## Measured

After `just nuttx build-fixtures-arm` (arm fixtures at 06:12) and a later
riscv NuttX kernel export (06:40), every arm NuttX Rust cell refused to run:

```
BuildFailed("Test fixture is STALE — a source is newer than the built binary:
  binary: …/cargo-fixtures/nuttx/armv7a-nuttx-eabihf/nros-minsizerel/service-server
  newer:  …/third-party/nuttx/nuttx
```

Nothing the arm image reads had changed. The riscv build had written
`nros-nuttx-export-riscv/`, `nuttx.hex` and `staging/` INTO the kernel tree.

## Cause

Phase-484 W2c routed the NuttX readers (`nuttx_dir()`, `nuttx_ffi_build`,
`nuttx_image_link`, zpico's NuttX arm) through
`nros_build_paths::locate::try_source`, which ends in `watch_path` on the
answer — a `cargo:rerun-if-changed` on the whole kernel tree. `watch_path`'s
own documentation forbids exactly that for a vendored SDK whose build writes
into it: cargo takes the newest mtime under a watched directory, so the
watch is dirty after every kernel build of ANY arch. Before W2c these
readers resolved the directory without watching it, and watched the files
they read (`staging/`, the board sources, the linker script).

## Fix

`[source.<name>] builds_in_place = true` in `nros-sdk-index.toml` (NuttX
kernel and apps), parsed by the CLI's `SourcePackage` and by
`nros_build_paths::locate::Row`. `try_source` returns such a tree without
watching it; the callers' own per-file watches are unchanged. A parser unit
test covers the bare boolean.
