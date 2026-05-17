# SDK Tiers for `just setup`

`just setup` orchestrates per-module setup recipes. Phase 142
introduces tiered selection so contributors can pick coverage
explicitly instead of getting a two-hour first-run install on a
laptop they only use to fix a `nros-core` typo.

```
just setup                  # default tier (recommended)
just setup tier=minimal     # Rust-only contributor, no embedded
just setup tier=extended    # adds heavy / private-SDK modules
NROS_SETUP_TIER=extended just setup
```

`just doctor [tier=<tier>]` mirrors the same selection so diagnosis
output matches the install. CI matrix selects per runner.

## Tiers

| Tier | Modules | Use case |
|------|---------|----------|
| `minimal` | `workspace`, `verification`, `zenohd` | Rust-only contributor working on `nros-core` / `nros-rmw` / `nros-platform-api` |
| `default` | `minimal` + `qemu`, `freertos`, `nuttx`, `threadx_{linux,riscv64}`, `esp32`, `zephyr`, `xrce`, `rmw_zenoh`, `orin_spe`, `cyclonedds`, `platformio` | Full `just ci` coverage; what every contributor should run unless they have a specific reason not to |
| `extended` | `default` + `esp_idf`, `px4` | Every Phase 139 integration smoke test runnable |

Tiers are strict supersets: `minimal ⊂ default ⊂ extended`. A
module never moves between tiers without bumping this document
and the orchestrator switch in `justfile::_orchestrate`.

## Policy: when does a module join `default`?

A module joins `default` when **all** of:

1. **Cheap to install.** ≤ 500 MB on disk AND ≤ 5 min wall-clock on
   a standard contributor laptop.
2. **Exercised by `just test-all`.** At least one
   `nros_tests::skip!` call (or a `cmake` / `cargo build` smoke
   step in a `tests/` integration test) references SDK paths /
   binaries the module provides.
3. **Idempotent on re-run.** No destructive ops, no `sudo`, no
   network-only stages that flap.

A module joins `extended` when (1) or (2) fails but the module is
still needed by some Phase 139 integration shell or supported
platform. Examples: ESP-IDF (≈ 2 GB clone + Python deps + xtensa
toolchain — fails (1)); PX4 (PX4-Autopilot clone + gz-sim + Python
deps — fails (1)).

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
3. Decide tier per §Policy above.
4. Add `run <module>` to the matching tier branch of
   `_orchestrate` in `justfile`.
5. Update the table in §Tiers in this doc.
6. Update `CLAUDE.md`'s "## Build" section if the change is
   user-facing.

If the module is opt-in entirely, skip steps 3–5 and add a note
to its own README explaining why.

## Cold-start verification

Phase 135.4's deferred `rm -rf build && rm -rf
target* && just setup && just ci` check assumes `tier=default`
(the documented contract). Re-run with `tier=extended` only when
verifying Phase 139 integration shells end-to-end on a fresh
checkout.

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
