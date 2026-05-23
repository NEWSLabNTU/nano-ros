# SDK Tiers for Setup

Setup is split by customer workflow. `scripts/bootstrap.sh` is the
entrypoint for a fresh checkout with no `just` installed yet; once `just`
is available, all paths delegate to the same recipes.

```
scripts/bootstrap.sh                 # first-time quick start
scripts/bootstrap.sh platform zephyr # focused platform developer
scripts/bootstrap.sh all             # contributor / full test-all setup

just setup                           # base quick-start tier
just setup all                       # full contributor / test-all tier
just <platform> setup                # focused platform setup
just doctor                          # diagnose base tier
just doctor tier=all                 # diagnose full tier
```

`NROS_SETUP_TIER` overrides the default no-argument `just setup` /
`just doctor` tier. Valid tiers are `base` and `all`. Legacy aliases
`minimal` and `default` map to `base`; `extended` and `everything` map
to `all`.

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
