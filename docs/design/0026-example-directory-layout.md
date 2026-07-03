---
rfc: 0026
title: "Example directory layout"
status: Stable
since: 2026-02
last-reviewed: 2026-07
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# Example directory layout

> **Revised 2026-06.** This RFC originally proposed a depth-4
> `platform/language/rmw/use-case` hierarchy. That layout was **superseded**:
> Phase 118 + 168 **collapsed the RMW dimension out of the path** (RMW is now a
> build-time choice, not a directory). The current canonical shape is below; the
> depth-4 history is in the Changelog.

## Canonical shape

```
examples/<platform>/<language>/<example>/
```

RMW is selected **at build time**, not encoded in the path:

- Rust → a cargo feature lowered from the declared RMW (RFC-0031), `default = ["rmw-zenoh"]`.
- C / C++ → `-DNROS_RMW=<rmw>` (the user-facing knob every example CMakeLists
  reads). The workspace helpers (`nano_ros_workspace()` /
  `nano_ros_workspace_pkg_guard()` in `cmake/NanoRosWorkspace.cmake`) publish
  the resolved backend under **both** names — `NROS_RMW` (the short helper
  alias) and `NANO_ROS_RMW` (the root variable the `cmake/platform/*` modules
  consume) — so either layer sees a consistent value. Examples document only
  `-DNROS_RMW`.
- Zephyr → a `prj-<rmw>.conf` Kconfig overlay.

So one `examples/zephyr/rust/talker/` builds against zenoh, xrce, or cyclonedds —
there are no `<rmw>/` siblings. Phase 168.6.C deleted the legacy
`<plat>/<lang>/<rmw>/<case>/` triples on Zephyr.

Each example directory is a **standalone copy-out template** (RFC, per its own
"Examples = Standalone Projects" rules): its own `Cargo.toml` + `.cargo/config.toml`
+ `CMakeLists.txt`, no workspace walk-up.

The copy-out contract is **tested** (phase-277 W6): Rust manifests declare
nano-ros crates registry-style (`nros = { version = "*" }`) with a tracked
`# nros-managed` `[patch.crates-io]` block that `nros sync` re-points at any
checkout; C/C++ CMakeLists resolve the nano-ros root through one guard —
`-DNANO_ROS_ROOT=<path>` cache var, else the `NROS_REPO_DIR` env var, else the
in-repo relative walk-up. Copying a directory out of the repo and building it
against a checkout is part of the CI-checked surface
(`just zephyr check-copy-out`, W6 smokes).

### Lockfiles

`examples/**/Cargo.lock` is **gitignored repo-wide** (phase-277 W7): standalone
example/workspace lockfiles are not reproducibility-critical (the fixture
prefetch in `scripts/build/cargo.sh` refreshes them without `--locked`) and
committed copies only go stale. Per-example `.gitignore` files keep their own
`/Cargo.lock` line so the ignore travels with a copy-out.

## Sibling categories

- `examples/<plat>/<lang>/<example>/` — the canonical per-platform examples.
- `examples/bridges/<name>/` — cross-RMW gateways (link ≥2 backends).
- `examples/templates/<name>/` — multi-platform copy-out recipes (Pattern A workspaces, etc.).
- `examples/workspaces/…` — multi-node workspace examples (Node pkg + Bringup
  pkg + Entry pkg; see RFC-0024/0025), in a **two-layer scheme**:
  - `examples/workspaces/<lang>/` — the four **base starter workspaces**
    (`rust`, `c`, `cpp`, `mixed`): the canonical talker+listener product shape,
    one per language mix.
  - `examples/workspaces/ws-<topic>-<lang>[-<variant>]/` — **topic showcases**
    (`ws-qos-rust`, `ws-lifecycle-cpp`, `ws-realtime-cpp-subnode`, …): each
    demonstrates one feature axis (params, QoS, lifecycle, launch, safety,
    custom-msg, bridge, realtime tiers) on top of the base shape.

Variant naming uses a **suffix** form so variants sort with their peers:
`talker-rtic`, `service-client-async`, `talker-rtic-mixed`.

**Interim exception (blessed until phase-275 closes):** the Entry-pkg sibling
dirs use a snake_case `_entry` suffix (`talker_entry`, `listener_entry`, … on
`qemu-arm-freertos`, `qemu-arm-nuttx`, `threadx-linux`). The kebab-case
`-entry` rename waits on phase-275, which owns the fixture-manifest/lane
renames; tracked in issue #132.

## README tiers

Three README tiers, linted by `scripts/check-example-matrix.sh`:

1. `examples/README.md` — the authoritative coverage matrix + copy-out contract.
2. `examples/<platform>/README.md` — per-platform: prerequisites, RMW knob,
   build/run one example, case table. Required for every platform dir.
3. Per-example `README.md` — **only** for variants, `bridges/*`, `ws-*` and
   `templates/*` (dirs whose purpose isn't obvious from the role name).
   Canonical role examples (`talker`, `listener`, …) deliberately carry no
   per-example README — the platform README covers them.

## Carve-outs

- `examples/zephyr/cpp/cyclonedds/talker-aemv8r/` **and its Rust sibling**
  `examples/zephyr/rust/cyclonedds/talker-aemv8r/` — one-board-one-RMW FVP
  AEMv8-R references, intentionally **not** collapsed.
- `examples/qemu-riscv-nuttx/` is a **partial platform**: it ships only
  `c/talker`, built by the separate `build-riscv-c` recipe in `just/nuttx.just`
  (own riscv toolchain/board lane, not the `qemu-arm-nuttx` path).
- Deliberately empty cells (no harness exists): bare-metal `{c,cpp}` (no hosted
  RTOS startup/heap/libc), and `px4/{c,rust}` (PX4 is uORB-only, C++-only port).

## Fixture-bin extraction

Test-only variants are **not examples**: anything whose purpose is a test/e2e
fixture rather than a user-facing template lives under
`packages/testing/nros-tests/bins/<name>/` and is wired through
`examples/fixtures.toml` + `fixtures/binaries/mod.rs`. Phase-277 W7 moved
`entry-poc`, `qemu-baremetal-main-e2e` and `rtic-run-plan-e2e` (ex
`phase216-rtic-e2e`) out of `examples/` under this rule.

## Authority

The authoritative matrix of which `<plat>/<lang>/<rmw>` triples exist lives in
`examples/README.md` ("Coverage matrix" + "Intentionally empty cells"). Phase 118
lint blocks untriaged cells. Non-example binaries (tests/benches/smokes) live
under `packages/testing/{nros-tests/bins,nros-bench,nros-smoke}/`, not `examples/`.

## Changelog

- 2026-07 — Phase-277 refresh: workspaces two-layer scheme (base 4 +
  `ws-<topic>-<lang>`); `-DNROS_RMW` documented as the user knob with the
  `NANO_ROS_RMW` root variable published by the workspace helpers; tested
  copy-out contract (W6) recorded; `examples/**/Cargo.lock` gitignore policy;
  README tier policy + lint; rust `cyclonedds/talker-aemv8r` carve-out added;
  `qemu-riscv-nuttx` partial platform noted; `_entry` naming exception blessed
  pending phase-275 (issue #132); fixture-bin extraction convention
  (test-only variants → `nros-tests/bins/`).
- 2026-06 — Revised to the collapsed `<plat>/<lang>/<example>/` shape (RMW is a
  build-time choice). Added bridges/templates/workspaces siblings + carve-outs.
- 2026-02 — Original proposal: depth-4 `platform/language/rmw/use-case` hierarchy
  with per-RMW directories. Superseded by the Phase 118 + 168 collapse.
