---
id: 2322
title: "An image embeds its absolute workspace path, so the same project built in another directory is not byte-identical, and the book does not say so"
status: resolved
type: enhancement
area: build, reproducibility, book
purpose: [industrial]
related: [phase-523]
found: 2026-11-07
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- I5: the same scaffold and knob, built in `~/a/sensor_ws` and `~/b/sensor_ws`, give ELF `55f96fa6…` and `6fc6b81d…`. The `objcopy -O binary` images also differ. `strings` finds 69 occurrences of `/home/dev/a`. Rebuilding at the same path is byte-identical.
- E (note): two seats' firmware hashes differed for the same reason.
- The default build is cargo's `dev` profile, a 16 MB ELF. First Project does not mention this.
- Choose your path promises "the same inputs give the same image".

## Next

Remap paths by default (`--remap-path-prefix` for rustc and `-ffile-prefix-map` for C) so the image does not depend on where it was built. Say in the book which profile First Project builds, and what is expected to reproduce.

## Progress (2026-11-07)

Measured in `ros:jazzy` on `0.6.0-nros8`, with the same scaffold built at
`/root/a` and `/root/b`:

- With `--remap-path-prefix` (Rust) and `-ffile-prefix-map` (C), the 69
  absolute-path strings are gone. The images still differ: the ELF and the
  `objcopy -O binary` image (whose sizes differ), with 20,446 symbol lines
  changed.
- Cause: cargo's `-C metadata` for a path dependency outside the cargo root
  hashes its absolute path. The generated root is
  `build/<image>/<entry>/`, and the workspace's `src/` and `generated/` crates
  sit outside it. The hash moves the mangled names and the codegen-unit
  partitioning, so the code layout changes too. Path remapping alone cannot
  fix it, so it was not landed: it would write absolute paths into the
  generated cargo config and buy no reproducibility.
- The fix is a cargo root that contains every local crate (the workspace
  root), or a stable `trim-paths` (unstable on 1.99).
- The book now states the contract: the same pin, lock and ROS 2 packages
  give the same bytes at the same absolute path. It recommends a fixed
  container path, records the digest through `nros doctor --report`, and says
  First Project builds the debug profile (Deploy to production › the
  readiness checklist).

## Resolution (2026-11-09)

Fixed by the layout change the progress note named, under the maintainer's
go-ahead (RFC-0098 amendment, same date). The generated entry carries a
relative link `_ws -> ../../..`, and every dependency path into the workspace
is spelled through it, so cargo hashes those crates relative to the entry's
root instead of by their absolute path. The workspace's remaining absolute
path (debug info's compile directory, `$OUT_DIR`) is remapped with
`--remap-path-prefix=<ws>=.` on the cargo command line.

Measured: a scaffold built at two paths gives a byte-identical ELF on `linux`
(debug profile, `2ffca93c…` both) and on `mps2-an385-baremetal` (ELF and the
`objcopy -O binary` image, zenoh-pico's C included), with no workspace path
in either. The book's checklist (Deploy to production) now promises the same
bytes from any project directory, with nano-ros installed at the same path.
