# SDK Tiers for Setup

Setup is split by customer workflow. `scripts/bootstrap.sh` is the
entrypoint for a fresh checkout with no `just` installed yet; once `just`
is available, all paths delegate to the same recipes.

```
scripts/bootstrap.sh                 # install/check just, then show setup choices
scripts/bootstrap.sh base            # first-time quick start
scripts/bootstrap.sh platform zephyr # focused platform developer
scripts/bootstrap.sh all             # contributor / full test-all setup

just setup                           # print setup choices
just setup base                      # base quick-start tier
just setup all                       # full contributor / test-all tier
just <platform> setup                # focused platform setup
just doctor                          # diagnose base tier
just doctor tier=all                 # diagnose full tier
```

`just setup` with no argument is a menu and does not fetch/install.
`NROS_SETUP_TIER` still overrides the default no-argument `just doctor`
tier. Valid tiers are `base` and `all`. Legacy aliases `minimal` and
`default` map to `base`; `extended` and `everything` map to `all`.

## Tiers

| Tier | Modules | Use case |
|------|---------|----------|
| `base` | `workspace`, `zenohd` | First-time users who want native nano-ros examples and standard ROS/zenoh workflows without every RTOS SDK |
| platform-specific | one module, e.g. `zephyr`, `nuttx`, `esp_idf`, `px4` | Developers focused on one target platform |
| `all` | `workspace`, `verification`, `zenohd`, `qemu`, `freertos`, `nuttx`, `threadx_{linux,riscv64}`, `esp32`, `zephyr`, `xrce`, `rmw_zenoh`, `orin_spe`, `cyclonedds`, `platformio`, `esp_idf`, `px4` | Contributors preparing for `just build-test-fixtures` and `just test-all` |

`all` is intentionally explicit because it pulls many submodules and
installs large SDKs. A module never moves between tiers without bumping
this document and the orchestrator switch in `justfile::_orchestrate`.

## Policy: when does a module join `base`?

A module joins `base` when **all** of:

1. **Expected by first-time users.** It supports native examples,
   standard ROS/zenoh interop, or core workspace development.
2. **Moderate surprise.** It does not fetch large platform SDKs such as
   PX4-Autopilot, ESP-IDF, Zephyr, or full RTOS trees.
3. **Idempotent on re-run.** No destructive ops, no `sudo`, no
   network-only stages that flap.

A module joins `all` when it is needed by full-matrix development or
`just test-all`, but is too heavy or platform-specific for the default
quick start.

A module stays **opt-in entirely** (neither tier) when it pulls
in private SDKs, requires `sudo`, or carries license-restricted
bits. ARM FVP, NVIDIA SDK Manager, Cadence Tensilica toolchain,
proprietary vendor BSPs all fall here. Contributors needing them
run `just <module> setup` explicitly out-of-band.

### CycloneDDS — self-provisioned in CMake (Phase 186)

`nros-rmw-cyclonedds` consumes Cyclone via `find_package(CycloneDDS)` and never
compiled it itself. **Phase 186** moved provisioning into the build system: the
backend's `nros_provide_cyclonedds()` resolves Cyclone in order —

1. an already-defined `CycloneDDS::ddsc` target,
2. `find_package(CycloneDDS CONFIG)` — a prebuilt install on `CMAKE_PREFIX_PATH`
   / `CycloneDDS_DIR` (a user install),
3. **self-provision from source** — `add_subdirectory(${CYCLONEDDS_SOURCE_DIR})`
   (defaults to the pinned `third-party/dds/cyclonedds` submodule; a user points
   it at their own checkout), with the per-platform flags staged in
   `cmake/platform/nano-ros-<plat>.cmake` and **sccache** as the compiler launcher.

So a bare `cmake`/`cargo` build self-provisions Cyclone with **no `just
cyclonedds` pre-step** — freertos / threadx-rv64 / native all build it on demand
(`just <plat> build-fixtures`), gated on the relevant cross toolchain. There is no
longer a `build/cyclonedds-<rtos>-install` artifact (the old cross-probe scripts
are deleted); the targets link a **static** `ddsc` (no runtime `libddsc.so`, so no
rpath and no risk of ld.so substituting a mismatched system `/opt/ros` Cyclone).

Availability still follows the tier that ships the cross toolchains:

- **`all` tier** (ships `arm-none-eabi-gcc` via `freertos`, RV64 GCC via
  `threadx_riscv64`): the embedded-Cyclone `test-all` cases build + pass.
- **`base` tier** (no cross toolchains): the embedded-Cyclone tests are
  **filtered out** of `test-all` (gated on `command -v <cross-cc>`, Phase 185.2/
  186.4) so they report `skipped`, not `failed`. ThreadX-RV64 Cyclone fixtures
  stay behind the experimental `NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=1`.

**Host `build/install` (the `cyclonedds` setup module) is still built** — for the
backend standalone CI (`just cyclonedds build-rmw/test/ci`), the Zephyr-Cyclone
host `idlc`, and the not-yet-migrated `threadx-linux` host-Cyclone path. Example
builds no longer depend on it (Phase 186 "host build.sh" follow-up tracks full
removal).

Run `test-all` in the `all` tier for full Cyclone coverage.

## Adding a new module

When introducing a new RTOS / SDK module:

1. Land `just/<module>.just` with `setup` (idempotent) and
   `doctor` (read-only) recipes.
2. Add `mod <module> 'just/<module>.just'` to `justfile`.
3. Decide whether it belongs in `base`, `all`, or platform-only setup.
4. Add `run <module>` to the matching tier branch of
   `_orchestrate` in `justfile`.
5. Update the table in §Tiers in this doc.
6. Update `AGENTS.md`'s "Build, Test, and Development Commands" if the change is
   user-facing.

If the module is opt-in entirely, skip steps 3–5 and add a note
to its own README explaining why.

## Cold-start verification

For full-matrix cold start, run:

```
rm -rf build target*
scripts/bootstrap.sh all
source ./setup.bash
just build-test-fixtures
just test-all
```

For quick-start cold start, use `scripts/bootstrap.sh` without `all`.

## Relation to Cargo features

Tiers describe what gets **installed** (host SDKs, toolchains,
clones). Cargo features describe what gets **compiled** inside
the nano-ros workspace. The two axes are orthogonal: a
contributor may install `tier=extended` (every SDK) but build
with `--no-default-features` (minimum nros feature set).

## Relation to packaged releases (Phase 139.8)

Downstream consumers of a packaged nano-ros release (ESP-IDF
component registry, PlatformIO library, Zephyr `west.yml`,
NuttX-apps, PX4 external module) never invoke `just setup`.
They pull a pinned release through their RTOS's package manager
and the host-SDK install is handled by the RTOS toolchain. Tier
policy applies to local development of nano-ros itself, not to
consumers. See [registry-publishing.md](../release/registry-publishing.md).
