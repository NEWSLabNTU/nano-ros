---
id: 833
title: "`just doctor` kept its own copy of the cross-target list, so it reported
  `[OK] rust-targets` on a host that could not configure the FreeRTOS C++ lane"
status: resolved
resolved_in: "phase-386 (follow-on)"
type: bug
area: build
related: [phase-372, phase-385, phase-386, issue-0196, issue-0326]
---

## Problem

`just freertos build-fixtures` exited 2 on a host where `just doctor` was fully
green. The failure was a cmake CONFIGURE error, four layers from anything the
diff touched:

```
CMake Error at .../corrosion/0.6.1-nros1/share/cmake/FindRust.cmake:812 (message):
  Target armv8r-none-eabihf is not installed for toolchain
  stable-x86_64-unknown-linux-gnu.
Call Stack (most recent call first):
  .../CorrosionConfig.cmake:33 (include)
  cmake/NanoRosCorrosion.cmake:608 (find_package)
  CMakeLists.txt:91 (nros_resolve_corrosion)
```

`armv8r-none-eabihf` is Cortex-R52 / ARMv8-R AArch32 — the target
`cmake/toolchain/arm-freertos-armcr52.cmake` sets and both
`nros-board-s32z270-freertos` (phase-372) and `nros-board-mps3-an536-freertos`
(phase-385) declare. Two `[[workspace_fixture]]` rows cross-build to it.

## Root cause: two hand-authored copies of one list

The target list existed twice, in two files, with no connection between them:

| Copy | What it did | Had `armv8r`? |
| --- | --- | --- |
| `just workspace rust-targets` | installs | **yes** (added by phase-372 W1) |
| `just doctor` | verifies | **no** |

phase-372 added the target to the installer and not to the checker. So a host
provisioned before phase-372, or provisioned by any path that did not re-run
`rust-targets`, passed `just doctor` while being structurally unable to
configure the lane.

That is CLAUDE.md's "fix the CLASS, not the reported site" in its most literal
form — the #282→#326 shape, where the fix added a *second idiom* instead of a
shared helper. It is also the issue-0196 rule: a gate whose coverage is
narrower than the rule it enforces. A doctor is worth exactly as much as the
completeness of what it checks; a doctor with its own private idea of the list
is worse than no doctor, because a green one stops the search.

## Why the symptom was unreadable

Three separate maskings stacked, and each is worth naming because none is
specific to this bug:

1. **The build is a parallel `make` fan-out** (one target per workspace dir), so
   the last line of the log is the last line *emitted*, not the failing step.
   Here it was `built: examples/workspaces/realtime-cpp/…/freertos_entry` — a
   **success** message from the group that passed.
2. **`make` exits 2 on error.** The recipe's exit 2 read like a bespoke status
   code from the build script; it was just make.
3. **The stderr tail was ~180 lines of benign newlib warnings**
   (`_read is not implemented and will always fail`, `LOAD segment with RWX
   permissions`), so the visible end of the log looked like a link problem.

A first pass at this diagnosis blamed the inputsig stamp step
(`workspace-fixture-signature.sh`), on the reasoning that it is the step
immediately after the last `built:` echo and that a non-zero exit there would
end the subshell silently. That was recorded as an explicit hypothesis rather
than a cause, and it was **wrong**: run against the actual manifest record for
that row, the signature script exits 0. The correct move was to run the failing
stage alone and read `grep '\*\*\*'` on its stderr, which named
`ws-group-0` and the real cmake error in one line.

## Fix

**One list, as data.** `config/rust-targets.txt` — one row per target, column 2
saying how the target is provided:

- `rustup` — a prebuilt `rust-std` exists; the installer installs it and the
  doctor verifies it.
- `build-std` — Tier 3 / custom JSON target (the two NuttX triples). Nothing to
  install; the row exists so the coverage gate can tell *deliberately not
  installable* from *somebody forgot*.

Both consumers read it through `scripts/lib/rust-targets.sh`
(`nros_rust_targets [rustup|build-std|all]`). They can no longer disagree.

**And the gate for the next one.** A shared list fixes the divergence that
existed; it does nothing about a board that lands a *new* triple tomorrow —
which is how this one happened. `check-rust-targets-covered`
(`scripts/check-rust-targets-covered.py`, on the fast line) asserts every target
**declared** anywhere in the tree has a row, across all three declaring
producers:

- `packages/boards/*/nros-board.toml` → `[target.<triple>]`
- `cmake/toolchain/*.cmake` → `set(Rust_CARGO_TARGET "<triple>" …)`
- `**/.cargo/config.toml` → `[build] target = "<triple>"` (`.json` stem stripped)

The gate is one-directional: the list may be a superset (it carries
`armv7r-none-eabihf` for the Orin SPE board, which no committed board declares
yet). Over-provisioning costs a download; under-provisioning costs a red build
whose error names a cmake module.

## Verification

- `rustup target add armv8r-none-eabihf`, then
  `scripts/build/workspace-fixtures-build.sh freertos cpp` → **rc=0** (was 2).
- The whole lane, not just the stage that failed: `just freertos build-fixtures`
  → **exit 0**, ending `FreeRTOS test fixtures built.` That matters because this
  lane also runs `build-examples`, `build-fixture-extras` and
  `build-fixtures-posix`, and a stage-level green would not have shown whether
  something else was queued behind the one that failed first.
- Gate negative control, on a real data row rather than header prose: deleting
  the `armv8r-none-eabihf` row makes the gate fail and name all three declaring
  sites.
- Doctor loop negative control: fed an installed-set without `armv8r`, it
  reports `missing: armv8r-none-eabihf` — the state this host was actually in
  and the old copy could not see.

## Not fixed here

`scripts/zephyr/setup.sh` and `just/zephyr-setup.just` add their own targets
(`armv7a-none-eabi`, `x86_64-unknown-none`) on a separate provisioning path.
They are not declared by any board toml, toolchain file or leaf config, so the
gate does not reach them and this issue does not claim they are covered.
