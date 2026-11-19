---
id: 2322
title: "An image embeds its absolute workspace path, so the same project built in another directory is not byte-identical, and the book does not say so"
status: resolved
resolved: 2026-11-19
resolved_in: "the change that archives this issue"
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

## Status (2026-11-17): needs a design decision

The path-remap attempt 69c5bcf04c was reverted the same day (e56647ee05;
the `_ws` link broke in-tree builds, issue 2385). Only the book's
contract statement remains: the same pin, lock and ROS 2 packages give the
same bytes at the same absolute path. A real fix needs either a cargo root
that contains every local crate, or a stable `trim-paths` (unstable on the
pinned 1.99).

**Suggested owner: the phase-523 / build owner, after a maintainer
decision** between those two.

## Resolved (2026-11-19): option A, per-workspace

User ruling 2026-11-19: revive the `_ws` link, guarded, scoped to "a
copied-out project builds byte-identical from any directory".

- The entry reaches its workspace through `_ws -> ../../..` again (the
  revert of e56647ee05), and the workspace path is remapped out of debug
  info.
- The guard that 2385 called for: `builder::paths::workspace_path_escape`
  walks every `Cargo.toml` in the workspace and follows each relative
  dependency closure. If any relative path leaves the workspace, NO crate of
  it is routed, and `nros build` names the crate and the path. It is all or
  nothing per workspace because two crates reaching a shared crate by two
  spellings would build two units of it (issue 0616's class).
- RFC-0098's amendment is restored with the guard added.

Measured, a scaffold (`nros new ws --board <b> --template talker`) built in
two scratch directories:

| board | ELF sha256 | `objcopy -O binary` | project-path strings |
| --- | --- | --- | --- |
| linux (debug) | `f8a341fb...` both | `2b4ec32b...` both | 0 |
| mps2-an385-baremetal | `78dd18a1...` both | `2992fe02...` both | 0 |

Issue 2385's repro, a cold `nros configure` in
`examples/workspaces/features`, completes. It prints the guard's notice
(`generated/custom_msgs` names `../../../../../packages/...`) and builds as
before the link existed.

An in-tree workspace still reproduces only at the same path; the book's
promise is for a copied-out project.
