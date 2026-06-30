---
id: 120
title: "phase-267 bridge-workspace fixtures (`workspace-rust-native-bridge`, `workspace-rust-threadx-linux`) fail the `build-test-fixtures` matrix when the cyclonedds submodule is absent — the gate leaks"
status: resolved
type: bug
area: testing
related: [phase-267, 0096, 0106, 0107, 0109, 0113]
resolved_in: "fix(0120) — explicit cyclonedds-submodule gate; threadx-linux leg cannot-reproduce"
---

## Update (2026-07-01) — cyclonedds gate implemented; threadx-linux leg still open

**Part 1 (cyclonedds bridge — DONE.)** `scripts/build/workspace-fixtures-build.sh` now runs an
explicit dependency gate in `build_workspace`: for a `platform = native` row whose
`-DNROS_RMW=cyclonedds` is set, it checks `third-party/cyclonedds/CMakeLists.txt` and, if the
submodule is absent, **fails LOUD before any build step** with an actionable message
(`requires the cyclonedds submodule … run: git submodule update --init --recursive
third-party/cyclonedds`) instead of letting the build fall through to the cryptic `E0433: cannot
find nros_board_native`. By design the bridge vendors C++ CycloneDDS, so this is a hard failure,
not a skip. Scoped to `native` so the embedded cyclonedds lanes (freertos/threadx/zephyr) keep
their own graceful idlc/submodule skips. Verified: with cyclonedds absent the native rust lane
now stops at the bridge with the explicit message and **zero** E0433 output.

**Part 2 (threadx-linux — NOT REPRODUCIBLE on current main; likely a broken-local-`nros`
artifact.)** Re-investigated 2026-07-01 on `9870968c8`. `workspace-rust-threadx-linux`
(`threadx_linux_entry`, a `nros::main!(launch=…)` bake macro) **builds green** for
`x86_64-unknown-linux-gnu` via three independent paths, **0 × E0463**:

1. `cargo build -p threadx_linux_entry --target x86_64-unknown-linux-gnu` (isolated).
2. The same after `rm -rf target-fixtures/threadx-linux` (cache-cleared clean rebuild — the
   exact condition the original report used).
3. The real recipe `scripts/build/workspace-fixtures-build.sh threadx-linux rust` (full
   `nros sync` + `codegen-system` + cargo, the row's `--target x86_64-unknown-linux-gnu`).

`nros-platform` is `#![no_std]` and exports `BoardConfig` / `BoardTransportConfig`
**unconditionally** (`src/lib.rs` `pub use board::{…}`), so a no_std rlib is produced for the
host target regardless of `platform-threadx`; `nros`'s `pub use nros_platform::{…}` resolves.

The original report flagged **uncommitted `nros-cli-core` working-tree churn** in that checkout.
Since `threadx_linux_entry` depends on a correct `nros sync` (run by the recipe) and the bridge
half (part 1) also had a broken-`nros sync` contributor, the most consistent explanation is that
the reporter's **locally-rebuilt `nros` CLI mangled `nros sync`** (e.g. dropped the
`nros-platform` dep / patch), yielding `E0463` — not a code defect. With a clean, committed
`nros` the leg is green. Recent `nros-platform` refactors (266-W4a moving `BakedBootConfig` to
`nros-platform-api`, 268-W1) may also have shifted resolution since the report.

Action: no code change for part 2 — left as cannot-reproduce. Reopen with a reproducer (and the
exact `nros` build used) if it resurfaces.

## Summary

`just build-test-fixtures` fails two **phase-267** workspace-fixture leaves —
`workspace-rust-native-bridge` (`examples/workspaces/ws-bridge-rust`) and
`workspace-rust-threadx-linux` (`examples/workspaces/rust`) — when the **cyclonedds submodule
is not checked out**. Both failures are deterministic (confirmed on a cache-cleared clean
rebuild). The rest of the matrix is green: **nuttx OK, freertos OK, zephyr OK, qemu OK**.

`examples/fixtures.toml:92` states the native-bridge row "is gated on the cyclonedds submodule,
like the imperative bridge bin" — but with cyclonedds **absent** the leaf still gets built by
`build-test-fixtures` and fails, so the gate does not suppress it in this lane.

This is **not** a regression from the phase-263 / build-infra work in this tree (the build-infra
fixes — issues 0090, 0110, plus the clang-format pin — are green and pushed). `git log` over both
fixture dirs shows only phase-267 commits; neither dir is touched by the pushed build-infra commits.

## Findings (file:line)

- **`workspace-rust-native-bridge` → E0433.**
  `examples/workspaces/ws-bridge-rust/src/native_entry/src/main.rs:22` (`nros::main!(launch = "demo_bringup")`)
  expands to:
  ```
  error[E0433]: cannot find `nros_board_native` in the crate root
  error[E0433]: cannot find `talker_pkg` in the crate root
  ```
  Root: `examples/workspaces/ws-bridge-rust/src/demo_bringup/` contains only `system.toml` — **no
  `nros-bridge.toml`**. The macro emits a bridge entry only when `nros sync` has generated
  `nros-bridge.toml` (it `include_str!`s it + calls `nros_bridge::run_from_config_str`); absent that
  file it falls back to a normal-launch entry that references `nros_board_native` + the node pkgs,
  which the bridge entry's `Cargo.toml` does not declare. So `nros ws sync` did not generate the
  bridge config — bridge descriptor staging needs cyclonedds, which is absent.

- **`workspace-rust-threadx-linux` → E0463.**
  Building `threadx_linux_entry` for `--target x86_64-unknown-linux-gnu` fails compiling `nros`:
  ```
  error[E0463]: can't find crate for `nros_platform`
    --> packages/core/nros/src/lib.rs:565
       pub use nros_platform::{BoardConfig, BoardTransportConfig};
  ```
  `nros` declares `nros-platform` unconditionally (`packages/core/nros/Cargo.toml:161`), so the rlib
  is simply not produced for this host-target config: the entry forces `nros-platform[platform-threadx]`
  (`src/threadx_linux_entry/Cargo.toml:34`) and feature/target unification on the x86_64-linux host
  build leaves no usable `nros_platform` crate for `nros`'s `pub use`.

## Environment

- cyclonedds submodule **absent** in this checkout (`third-party/cyclonedds/CMakeLists.txt` missing).
- Pre-existing **uncommitted** `nros-cli-core` working-tree churn was present
  (`src/cmd/codegen_system.rs`, `src/cmd/setup.rs`, `src/orchestration/board_metadata.rs`); if the
  local `nros` CLI was rebuilt from that source, broken `nros ws sync` bridge-gen is an alternate
  contributor to the native-bridge failure. Not investigated further (out of scope).

## Suggested fix (for the phase-267 owner)

1. Make `build-test-fixtures` honor the cyclonedds-submodule gate for **both** bridge leaves
   (`workspace-rust-native-bridge` and `workspace-rust-threadx-linux`) so they `skip` rather than
   fail when the submodule is absent — mirror the imperative-bridge bin gate.
2. Independently, fix the `threadx-linux` workspace feature unification so
   `nros-platform[platform-threadx]` does not poison the x86_64-host build of `nros` (separate from
   the cyclone gate — it has no cyclone dep).
