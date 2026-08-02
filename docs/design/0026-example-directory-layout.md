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
`<plat>/<lang>/<rmw>/<case>/` triples on Zephyr, and phase-316 removed the last
three levels that still looked like them, leaving
`check-example-matrix.sh`'s allowlist **empty**.

### A level that looks like an RMW may not be one

Two of those last three were not RMW levels, and the fix was to *rename* them,
not flatten them — flattening would have destroyed real information:

| was | is | what the level actually names |
| --- | --- | --- |
| `px4/rust/xrce/` | `px4/rust/companion/` | **where the code runs** — beside PX4, on a host or peer MCU, rather than in firmware. Its RMW is fixed by whatever `uxrce_dds_client` speaks; it was never a choice, so it was never a variant axis. |
| `px4/cpp/uorb/` | *(gone —* `packages/testing/nros-px4-register-check/` *)* | nothing: the tree held one link-check module, not an example. |
| `zephyr/{cpp,rust}/cyclonedds/talker-aemv8r/` | `zephyr/{cpp,rust}/talker-aemv8r/` | a **board**, already stated by the `-aemv8r` suffix. |

The px4 `companion/` level is deliberate and must not be "helpfully" collapsed
back into `px4/rust/`: an in-firmware px4 example would be a sibling of it, and
the two differ in deployment target, not in backend. The test is whether the
level names something the leaf name does not — `-aemv8r` failed that test;
`companion/` passes it.

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

> **The C/C++ CMakeLists shape is being reshaped by [RFC-0048](0048-cmake-ament-consumption.md)**
> (phase-287, #171 D5): the `-DNANO_ROS_ROOT` guard becomes source-backed
> `find_package(nano_ros)`, one ament-convention `CMakeLists.txt` byte-identical
> across platforms, with board/RMW moved into `package.xml <export>`. The
> copy-out contract itself (build a copied-out leaf against a checkout) is
> preserved; only the spelling changes.

### Lockfiles — the tracked-vs-ignored policy

Two rules, by who owns the dependency graph:

- **Core / workspace crates → `Cargo.lock` is TRACKED.** Their dependency
  graph is fixed at the repo, so the committed lock is a genuine
  reproducibility promise, and the `--locked` cargo shim
  (`scripts/bin/cargo`, issues 0359/0378) enforces it: a build that would
  rewrite the lock fails instead of drifting silently.

- **`examples/**/Cargo.lock` → gitignored repo-wide** (phase-277 W7). An
  example leaf's lock **cannot** be produced at the repo, because the leaf
  redirects its message deps through `[patch.crates-io]` to a `generated/`
  tree that does not exist until the **user** runs message codegen
  (`nros sync` / `nros generate-rust`) on THEIR machine — the lock is only
  resolvable *after* that user-side step. A lock committed from the repo would
  pin paths/hashes for generated crates the user hasn't produced yet: stale
  and misleading, never reproducible. So the leaf lock is a regenerable local
  artifact, created on the user's first build, not a tracked promise.
  (Standalone copy-out leaves are also not reproducibility-critical — the
  fixture prefetch in `scripts/build/cargo.sh` refreshes them without
  `--locked`.) Per-example `.gitignore` files keep their own `/Cargo.lock` line
  so the ignore travels with a copy-out.

- **In-tree testing/bench leaves → `Cargo.lock` MAY be tracked, iff the leaf
  commits its `generated/` tree** (RFC-0067 / phase-333). This third class did
  not exist while a leaf's message identity varied with the host: the crate
  version was the ament version (`std_msgs` 4.9.1 on jazzy, 5.3.6 on rolling)
  and the reference was a registry name rescued by `[patch.crates-io]`, so a
  committed lock recorded WHICH ROS install produced it and every other distro's
  `--locked` build failed as drift. Observed in practice: locks under
  `packages/testing/**` pinning 4.9.1, 4.9.0 and 5.3.6 — three distros — and one
  lock refreshed, reverted and refreshed again by hosts disagreeing.

  RFC-0067 removes both variables: message deps are `path` deps (D1) and the
  generated crate's version is the constant `0.0.0`, with the ament version
  demoted to `[package.metadata.nros] ament_version` (D2). A path-dep lock entry
  carries no `source` line and records `0.0.0` on every host, so such a lock is
  byte-identical across distros — a genuine promise, and `--locked` holds for it.

  A testing/bench leaf that does NOT commit `generated/` keeps a path dep to an
  absent directory: it fails CLOSED (`failed to read …/generated/std_msgs/
  Cargo.toml`) until `nros sync`, and therefore cannot commit a meaningful lock —
  the same reasoning that gitignores `examples/**`, reached from the other side.
  **Settled 2026-08-03:** ten such locks were tracked and were deleted; a leaf's
  `generated/` tree belongs to the USER (it is generated from THEIR ament
  packages), so the fix is always to drop the lock, never to commit a
  `generated/` tree in order to keep one. The exception that proves the rule is
  `packages/interfaces/*`: those message crates are pre-generated INTO the repo
  under `nros-`prefixed names — the prefix exists so core code can depend on them
  before any user codegen runs, without colliding with a user package of the same
  ROS name — so they resolve from a bare clone and their locks are tracked. Their
  crate VERSION is still the `0.0.0` constant, and consumers path-dep them with no
  version: the generator emits the constant unconditionally, so any pinned value
  is re-broken by the next regeneration (issue 0394 hit this twice, once per
  spelling). `check-leaf-lockfiles` enforces the whole invariant:
  **tracked lock ⟺ (no message deps) ∨ (committed `generated/`)**.

**Tooling agrees with the policy by construction (issue 0386):** the `--locked`
shim keys off `git check-ignore Cargo.lock`, so it forces `--locked` exactly for
the tracked (core) locks and skips it exactly for the ignored (example/leaf)
locks — the leaf may create its first lock and re-resolve after a later
`nros sync`, while a core lock still cannot drift.

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

**Entry-pkg sibling dirs** use the kebab-case `-entry` suffix (`talker-entry`,
`listener-entry`, … on `qemu-arm-freertos`, `qemu-arm-nuttx`, `threadx-linux`),
consistent with every other example dir. (The former snake_case `_entry` interim
exception was blessed only while phase-275 owned the fixture-manifest/lane
renames; phase-275 closed 2026-07-08 and the rename landed — #136 item 4.)
The Rust *package* names inside keep their `<plat>_rs_<role>_entry` scheme —
that is a crate identifier, not a directory name.

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

- `examples/zephyr/{cpp,rust}/talker-aemv8r/` — FVP AEMv8-R references that
  build against CycloneDDS only. They are a **board** variant, which the
  `-aemv8r` suffix already states; phase-316 W2 removed the `cyclonedds/` path
  level they used to sit under, along with the last two entries in
  `check-example-matrix.sh`'s allowlist. Being single-RMW is a property of the
  example, not a directory axis.
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

The authoritative matrix of which platform × language × RMW cells exist lives in
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
  pending phase-275 (issue #132) — **resolved 2026-07-08**: phase-275 closed and
  the `_entry` → `-entry` rename landed (#136 item 4); fixture-bin extraction
  convention (test-only variants → `nros-tests/bins/`).
- 2026-06 — Revised to the collapsed `<plat>/<lang>/<example>/` shape (RMW is a
  build-time choice). Added bridges/templates/workspaces siblings + carve-outs.
- 2026-02 — Original proposal: depth-4 `platform/language/rmw/use-case` hierarchy
  with per-RMW directories. Superseded by the Phase 118 + 168 collapse.
