# Phase 291 — `nros-zephyr-build`: dedupe the zephyr rust leaf `build.rs` (resolve #211)

Status: **Draft — 2026-07-16** · Resolves issue #211 · Touches RFC-0048 (sync
patch table) · Sibling of the phase-287 W9 leaf-config work.

> **Goal.** One canonical implementation of the zephyr leaf Kconfig→`rustc-env`
> bake (known-issue #17 locator/domain + issue-0163 XRCE synthesis) in a new
> zero-dependency build-helper crate, with every zephyr rust example and
> workspace `zephyr_entry` collapsed to a 4-line `build.rs`. Kills the 13-copy
> duplication AND the drift it already caused (the 7 workspace entries lack the
> XRCE block the 6 standalone examples carry).

## Design constraints (verified 2026-07-16, all source-checked)

- **The bake must stay in the leaf's own `build.rs`** — both macro paths
  (`nros::main!` zephyr branch, `zephyr_component_main!`) read
  `option_env!("NROS_LOCATOR")` at LEAF expansion; `cargo:rustc-env` from a
  dependency's build script never reaches other crates. A build-dependency
  helper is the only dedup shape that conserves this.
- **Kconfig stays the SSoT**: fixture lane `-DCONFIG_NROS_ZENOH_LOCATOR` →
  west `.config` → `DOTCONFIG` → leaf build.rs → `rustc-env` → macro. The
  #166 `--nros-locator` testargs runtime override sits on top unchanged.
- **The helper must be ZERO-DEP**: upstream `zephyr-build` resolves as a
  west-module PATH dep only (leaf `Cargo.lock` entry has no `source =`), so a
  helper depending on it would break host `cargo check --workspace`. The
  `zephyr_build::export_kconfig_bool_options()` call therefore STAYS in the
  leaf (it already deps `zephyr-build`).
- **`nros sync` wiring is one table entry**: sync scans leaf
  `[build-dependencies]` for registry-style names and patches any crate in
  `nros_crate_path_lookup()` leaf-locally. NOT central-patch material (only
  zephyr leaves name it; central membership needs every-graph presence).

## Waves

### W1 — the helper crate
- [ ] W1.a `packages/core/nros-zephyr-build`: `pub fn bake_nros_config()` —
  the canonical copy of today's leaf logic: `CONFIG_NROS_ZENOH_LOCATOR` →
  `NROS_LOCATOR`, `CONFIG_NROS_DOMAIN_ID` → `NROS_DOMAIN_ID`, plus the
  issue-0163 XRCE agent-locator synthesis self-gated on
  `CONFIG_NROS_RMW_XRCE=y`. Zero deps; `version.workspace = true`; root
  workspace member. `rerun-if-env-changed=DOTCONFIG` + per-env lines kept.
- [ ] W1.b Unit tests: temp `DOTCONFIG` fixtures → captured directive output
  (string/int bake, unset/empty no-ops, XRCE synthesis incl. addr/port
  defaults, XRCE absent ⇒ nothing).
- [ ] W1.c Sync wiring: `nros_crate_path_lookup()` gains
  `("nros-zephyr-build", "packages/core/nros-zephyr-build")` (+ its unit
  test row if the table has one); `just setup-cli` rebuild.

### W2 — migrate all 13 leaves
- [ ] W2.a 6 standalone examples (`examples/zephyr/rust/{talker,listener,
  service-server,service-client,action-server,action-client}`): `build.rs` →
  the 4-line shape; `[build-dependencies]` gains `nros-zephyr-build`
  (registry-style version so sync patches it).
- [ ] W2.b 7 workspace entries (`workspaces/rust` + `ws-{lifecycle,params,
  qos,realtime,safety}-rust` `src/zephyr_entry`): same collapse. These GAIN
  the XRCE synthesis (drift fix) — inert for zenoh images (self-gated).
- [ ] W2.c `nros sync` across the touched leaves → regenerated leaf-local
  patch blocks + `Cargo.lock`s committed.

### W3 — prove it
- [ ] W3.a `just zephyr build-fixtures` (west lane; fixture mtime treadmill —
  full rebuild, no stale binaries).
- [ ] W3.b Zephyr e2e sweep: the zenoh pubsub/service/action lanes + one XRCE
  lane (standalone example) + the ws-entry realtime lane (proves the ws
  entries' new XRCE block is inert for zenoh images).
- [ ] W3.c `just format` + `just check` green (CLI lane covers the sync table
  change).

### W4 — retire the old path (follow-up, after W3 soaks)
- [ ] W4.a Grep-gate: no `bake_kconfig_str(` / `bake_kconfig_int(` copies left
  under `examples/` (add to `check-example-shape` FORBIDDEN list or the
  string-conventions gate, whichever fits).
- [ ] W4.b Resolve + archive issue #211; one-line pointer in AGENTS.md
  practices if a durable lesson emerged.

## Non-goals
- Touching the C/C++ zephyr examples (no build.rs duplication there — the C
  path consumes `CONFIG_NROS_ZENOH_LOCATOR` directly).
- Replacing the Kconfig SSoT or the #166 runtime override.
- Wrapping/reimplementing upstream `zephyr_build` (west-only resolution —
  see constraints).

## Acceptance
- One implementation of the bake; every zephyr rust leaf `build.rs` ≤ 5 lines.
- Workspace entries pass the XRCE-configured build (previously impossible —
  the block was missing).
- Zephyr fixture + e2e lanes green on freshly rebuilt fixtures.
- #211 resolved with the grep-gate guarding recurrence.
