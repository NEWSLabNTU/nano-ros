# Phase 180 - nano-ros as a version-spanning consumable Zephyr module

**Goal.** Make nano-ros consumable as a Zephyr module from a user's *own*
Zephyr workspace, on **both** Zephyr 3.7 LTS and current 4.x. A downstream
developer imports nano-ros via their `west.yml`, picks an RMW, copies out an
example, and builds it against their workspace — without ever cloning a
nano-ros-owned Zephyr tree and without editing the nano-ros source tree.
Reduce customer-support load by making the consumption story standard,
discoverable, and CI-verified.

**Status.** Complete (2026-05-26). 180.A foundation + 180.B (copy-out examples)
+ 180.C snippets + `board_root` (N/A) + 180.D patch delivery
(`patches.yml`/`west patch`) + 180.E onboarding docs + the CI/samples cluster
(dual-line `ci-both`, copy-out check, Twister samples) landed on `main`.
zenoh native_sim is e2e-proven on both 3.7 + 4.4. cyclonedds-4.4: the
`k_mutex` runtime abort is fixed **and 2-node native_sim discovery is
e2e-proven** (c/talker → c/listener, Published 11 / Received 10) over unicast
SPDP + distinct per-process `--seed` — see `RESOLVED — cyclonedds-4.4 2-node
discovery` in `docs/research/zephyr-4.4-baseline.md`. (The 2-node fix —
`session.cpp` native_sim embedded config — sits on branch
`cyclonedds-4.4-2node-discovery`, pending merge.) Full record in
`docs/research/zephyr-4.4-baseline.md` + `zephyr-3.7-to-4.4-divergence-audit.md`.

**Priority.** P2 (consumability / customer-support; unblocks external adoption).

**Depends on.** None hard. Supersedes the consumability angle of the
archived Phase 174.B.1 (shared/prebuilt picolibc) — that build-perf lever is
unrelated to this and stays archived. Phase 139 integration shells
(`integrations/<rtos>/`) and the existing `zephyr/module.yml` are the
starting point.

## Overview

nano-ros is *already* a Zephyr module (`zephyr/module.yml`, name `nros`,
auto-linked as `NanoRos::NanoRos` under `CONFIG_NROS`), and
`integrations/zephyr/west.yml` is a downstream-import fragment. So the core
consumption path exists. What is missing for a real BYO-workspace story:

1. **Examples are not copy-out clean.** 18 Zephyr example `CMakeLists.txt`
   walk the source tree (`set(_nros_repo ${CMAKE_CURRENT_LIST_DIR}/../../../..)`)
   to reach `build/install/bin/idlc`, `scripts/cyclonedds`,
   `packages/dds/.../cmake/NrosRmwCycloneddsTypeSupport.cmake`, and a shared
   `examples/zephyr/cmake/` helper. Copied out of the repo, or built from a
   BYO workspace, these paths do not resolve. (zenoh/xrce examples are
   already clean — they consume the core purely via the module.)
2. **Patches do not travel.** 16 `scripts/zephyr/*-patch.sh` mutate the
   nano-ros-owned workspace in place (NSOS, Cyclone-on-Zephyr, Rust glue).
   A user who imports nano-ros into their own workspace gets none of them.
3. **One pinned version.** The manifest pins Zephyr v3.7.0. A BYO customer
   runs whatever Zephyr they already have.

**Version strategy (decided 2026-05-25): span 3.7 LTS + latest 4.x.**
Zephyr 3.7 is the current LTS (supported to at least January 2027); the
project pins it for the Autoware safety-island use case (Phase 117). No 4.x
is LTS — 4.x is a rolling six-month release (4.4, April 2026, EOL April
2027); the next LTS is expected ~2027 (a future 4.x). A BYO consumer is on
*either* line, so nano-ros must work as a module on both rather than pin one.
4.x additionally unlocks the modern consumption mechanics below; 3.7 keeps
the legacy mechanics.

### Zephyr 4.x features this phase leans on

| Feature | Use |
| --- | --- |
| `west patch` (`patches.yml`: `path`/`sha256sum`/`module`/`upstreamable`/`--roll-back`/`gh-fetch`) | Versioned, checksummed, rollback-safe patch delivery to a BYO workspace |
| module `samples:` + Twister | Examples become discoverable, CI-tested samples |
| module `snippet_root` (`west build -S nros-<rmw>`) | RMW selection travels with the module; replaces per-example `prj-<rmw>.conf` |
| module `board_root` / `module_ext_root` | Ship nano-ros boards + extra CMake/Kconfig as module-contributed |
| Rust = official Zephyr module (4.1) | Pinnable Rust base (today `zephyr-lang-rust` floats at `main`) |

3.7 has none of `west patch` / `samples:` / `snippet_root`; on 3.7 the
legacy mechanics (sed-patch scripts, `prj-<rmw>.conf` overlays, manual
example build) remain.

Canonical structural reference: `zephyrproject-rtos/example-application`
(out-of-tree module + samples).

## Architecture

- **Version-aware module.** `zephyr/CMakeLists.txt` + `zephyr/Kconfig`
  branch on `Zephyr_VERSION` / `KERNEL_VERSION_*` where 3.7↔4.x APIs
  diverge (NSOS driver, net/socket). The module exports its location and
  tooling paths (`CMAKE_MODULE_PATH`, `NROS_CYCLONE_SCRIPTS_DIR`, …) so
  consumers never walk the source tree.
- **Dual patch delivery.** 3.7 → existing `scripts/zephyr/*-patch.sh`
  (idempotent, anchor-guarded, skip-safe on a foreign tree). 4.x →
  `patches.yml` consumed by `west patch`. Patches flagged `upstreamable`
  get pushed upstream to shrink both sets over time.
- **Dual RMW selection.** 3.7 → `prj-<rmw>.conf` overlays (today's shape).
  4.x → module-shipped `nros-<rmw>` snippets (`-S nros-cyclonedds`). The
  Cyclone descriptor-gen step stays visible in each example and sources a
  module-exported CMake helper (no path walks) on both lines.
- **Tooling discovery contract.** Cyclone `idlc` comes from
  `find_package(CycloneDDS)` → `CycloneDDS::idlc` (the consumer's SDK), not
  a nano-ros build artifact. ROS message dirs come from env
  (`NROS_STD_MSGS_DIR`), not a hardcoded `/opt/ros/humble/...`.

## Work Items

### 180.A — version-spanning module foundation
Bring the nano-ros Zephyr module up green on both Zephyr 3.7 LTS and a
chosen 4.x (4.4). Version-conditional CMake/Kconfig for the diverging APIs;
manifest/recipe parametrized by Zephyr version; CI builds and tests the
example matrix on both lines. No new consumption features here — the bar is
"compiles + `just zephyr test` green on 3.7 and 4.4."
**Files.** `zephyr/CMakeLists.txt`, `zephyr/Kconfig`, `west.yml`,
`integrations/zephyr/west.yml`, `just/zephyr.just`,
`scripts/zephyr/*-patch.sh` (re-verify against 4.x tree shapes),
`docs/guides/zephyr-setup.md`.
- [x] Audit 3.7↔4.4 API divergence (`zephyr-3.7-to-4.4-divergence-audit.md`)
- [x] Version-gate the module CMake/Kconfig (force-include scoping, net_ip_mreq guard, version-aware NSOS line overlay)
- [x] Parametrize `just zephyr` by NROS_ZEPHYR_VERSION (manifest/workspace/venv/build-one/build-fixtures)
- [x] Re-verify 16 patches vs 4.4: NSOS recvmsg+multicast+adapt ported (4.4 scripts), getsockname dropped (upstream), Rust audited (native_sim needs none), Kconfig no-op, k_mutex runtime fix landed; cyclone-submodule patches baked into the pinned cyclonedds commit (no diff)
- [x] CI builds the example matrix on both 3.7 and 4.4 (`just zephyr ci-both` verified PASS/PASS; `.github/workflows/zephyr-dual-line.yml`)

### 180.B — copy-out-clean examples (version-agnostic)
Sever the 18 repo-walks. Module exports `CMAKE_MODULE_PATH` (+ cache vars);
the shared `NrosZephyrCycloneddsActionTypes.cmake` helper moves into the
module's exported cmake dir; examples use `find_package(CycloneDDS)` for
`idlc`, `include(NrosRmwCycloneddsTypeSupport)` by name, and
`$ENV{NROS_STD_MSGS_DIR}` for the message dir. Works identically on 3.7 and
4.x; lands value on the 3.7 LTS in use today and is a prerequisite for
180.C's `samples:` packaging.
**Files.** `zephyr/CMakeLists.txt` (exports),
`packages/dds/nros-rmw-cyclonedds/cmake/` (helper home),
`examples/zephyr/*/CMakeLists.txt` (18 edits),
`examples/zephyr/cmake/NrosZephyrCycloneddsActionTypes.cmake` (move),
`examples/README.md`.
- [x] Add module exports (`NROS_CYCLONE_IDLC` / `NROS_CYCLONE_SCRIPTS_DIR` / `NROS_CYCLONE_CMAKE_DIR`)
- [x] Move shared cyclone cmake helper into the module-exported dir (`packages/dds/nros-rmw-cyclonedds/cmake/`)
- [x] Rewrite the 18 example CMakeLists to module-discovery (use `NROS_CYCLONE_IDLC` + `list(APPEND CMAKE_MODULE_PATH ${NROS_CYCLONE_CMAKE_DIR})` + `include(<name>)`; Zephyr shadows the cache `CMAKE_MODULE_PATH`, so each example appends the exported dir itself — still copy-out clean, no repo path)
- [x] Replace `/opt/ros/humble` hardcode with `NROS_<PKG>_DIR` env contract (recipes default to `/opt/ros/humble/share/<pkg>`)
- [x] CI check: a copied-out example builds from outside the repo tree (`just zephyr check-copy-out`, verified PASS)

### 180.C — 4.x-native consumption
Layer the 4.x-only mechanics on top of A+B. Ship `nros-zenoh`/
`nros-cyclonedds`/`nros-xrce` snippets via `snippet_root`; declare examples
under `samples:` with `sample.yaml` + Twister; contribute boards via
`board_root`. 4.x build path only; 3.7 keeps `prj-<rmw>.conf`.
**Files.** `zephyr/module.yml` (`settings:`),
`snippets/nros-*/snippet.yml`, `examples/zephyr/*/sample.yaml`,
`boards/` (module board_root), `just/zephyr.just`.
- [x] `snippet_root` + per-RMW snippets (`nros-{zenoh,cyclonedds,xrce}`) carrying the RMW Kconfig (4.x; verified `-S nros-cyclonedds` discovered + applied + built)
- [x] `samples:` + `sample.yaml` + Twister cases (6 talker/listener samples; module.yml `samples:`; Twister discovery verified)
- [x] `board_root` — N/A (resolved): nano-ros ships NO Zephyr board definitions (no board.yml/_defconfig/.dts in-repo); it targets upstream/vendor boards (native_sim, fvp_baser_aemv8r, s32z2xxdc2, qemu_cortex_a9) with app-level overlays only. board_root contributes board DEFINITIONS, so there is nothing to export; per-board CONF travels via the example/snippet path. No work needed.
- [x] Document `-S nros-<rmw>` selection on 4.x

### 180.D — patch story / upstreaming
3.7 keeps sed-scripts; author `patches.yml` for 4.x consumed by
`west patch`. Triage the 16 patches: which are `upstreamable`, push those
upstream to shrink both sets. Patch delivery to a BYO workspace becomes the
manifest import + `west patch apply`.
**Files.** `patches.yml`, `scripts/zephyr/*-patch.sh` (3.7 retained),
`integrations/zephyr/west.yml`, `docs/development/zephyr-patches.md` (new).
- [x] Convert the 4.x-relevant patches to `patches.yml` (3 NSOS patches, sha256sum, module: zephyr; cyclone patches baked into the pinned submodule — excluded)
- [~] Triage `upstreamable`: 3 NSOS patches marked upstreamable to Zephyr; opening the upstream PR(s) is a human follow-up
- [x] Wire `west patch apply` into the downstream flow + document (`integrations/zephyr/README.md`; verified list/apply/clean on 4.4)

### 180.E — support / onboarding docs
A BYO-consumer onboarding guide covering both Zephyr lines: import the
module, pick an RMW, copy out an example, build. Make the support story
self-serve.
**Files.** `book/src/getting-started/zephyr-module.md` (new),
`integrations/zephyr/README.md`, `examples/README.md`.
- [x] BYO-workspace quickstart, 3.7 + 4.x paths (`book/src/getting-started/integration-zephyr.md`: import, west patch, -S snippet, copy-out)
- [x] Version-compatibility matrix (3.7 vs 4.x) + 4.x build-gotcha troubleshooting
- [x] Linked from top-level README + book SUMMARY

## Acceptance

- A BYO Zephyr workspace (3.7 *and* 4.4) can import nano-ros via its own
  `west.yml`, `west update`, copy out `examples/zephyr/c/talker`, and
  `west build` it for `native_sim` with a chosen RMW — no nano-ros source
  tree edits, no nano-ros-owned workspace.
- On 4.x, RMW selection is `-S nros-<rmw>`; required patches apply via
  `west patch`.
- CI builds the example matrix on both Zephyr lines and validates at least
  one copied-out example from outside the repo.

## Notes

- **Decisions (2026-05-25).** Customer = both BYO and fresh-start, phased.
  Example consumption = copy-out template (strict: zero repo-tree reach).
  Version = span 3.7 LTS + latest 4.x. First sub-project to spec = 180.B
  (version-agnostic, immediate value, prerequisite for 180.C).
- **LTS tension.** 3.7 LTS → Jan 2027; 4.4 (rolling) → Apr 2027; next LTS
  ~2027. Spanning both is the maintenance cost of serving the safety-island
  (LTS) customer and the modern-features customer simultaneously. Revisit
  consolidating onto the next 4.x LTS when it ships.
- **Sources.** Zephyr `west patch`
  (https://docs.zephyrproject.org/latest/develop/west/zephyr-cmds.html),
  module settings
  (https://docs.zephyrproject.org/latest/develop/modules.html), snippets
  (https://docs.zephyrproject.org/latest/build/snippets/writing.html),
  3.7 LTS announcement
  (https://www.zephyrproject.org/announcing-zephyr-3-7-new-long-term-support-release-of-zephyr-rtos/),
  `example-application`
  (https://github.com/zephyrproject-rtos/example-application).
