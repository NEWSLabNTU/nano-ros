# Phase 275/276 — branch implementation notes (2026-07-01)

> Working notes for branch `phase-275-276-example-fixture-coverage`. The dev host has failing RAM
> (issue #115) so **nothing here is build/run-verified** — this branch is a staging area to be
> built, verified, and completed on a known-good machine. Below: what was done, what each remaining
> item actually requires (study revealed most are NOT mechanical row-adds), and the risks found.

## Done on this branch (needs verification)

- **275 W2 (native/rust subset).** Added `examples/fixtures.toml` build-assert rows for the three
  native/rust variant examples that shipped with zero fixtures: `service-client-async`,
  `action-client-async`, `logging`. Safe because each has `default = ["rmw-zenoh"]` and the same
  crate shape (`Cargo.toml` + `src/main.rs`) as the covered base examples, so a bare row mirrors the
  working `native/rust/listener` pattern. **Verify:** `just build-test-fixtures` builds them; then
  add runtime assertions (async client receives; logging sink emits) under `nros-tests/tests/`.

## Remaining — findings, exact steps, risks

### 275 W1 — `*_entry` demos (NOT mechanical)
17 of 18 `*_entry` dirs are unexercised (only freertos `talker_entry`). **Risk/finding:** the one
that IS exercised is built by a **compile-in-test antipattern** — `freertos_run_plan_runtime.rs`
(`build_or_locate_entry_binary`) runs `cargo build` at test time, which CLAUDE.md explicitly
forbids ("No compilation inside tests"). Correct fix is NOT to copy that: convert entry-pkg builds
to **prebuilt fixtures** (a fixtures.toml `entry=`/`bringup=` row per `_entry`, or a driver), then a
test consumes the artifact. Needs the entry-pkg fixture-build schema (see the `native_*_entry`
rows using `entry=`/`bringup=` in `fixtures.toml` ~lines 60–160) mapped onto the embedded triples
(thumbv7m freertos, arm nuttx, host threadx-linux). Design + build loop required.

### 275 W2 — native C/C++ variants (remaining)
Uncovered: native/c `{custom-msg, custom-platform, custom-transport-loopback, logging}`, native/cpp
`{component-poc, component-node-poc, transform-poc, logging}`. These need the **native C/C++ cmake
row schema** (`cmake_defs`, `codegen_out`, `build_subdir`, `rmw`, per-RMW `build-<rmw>/`), not the
3-line rust row. Mirror an existing native C/C++ row block. Higher risk blind — do on good hardware.

### 275 W3 — zephyr non-role leaves
`zephyr/cpp/{cyclonedds,talker-typed}`, `zephyr/rust/{cyclonedds,service-client-async}` sit outside
the 6-role driver matrix. Extend `scripts/build/fixture-matrix.sh` (role/variant enumeration read by
`zephyr-fixture-leaves.sh`) to add them, **or** de-scope in `examples/README.md`. Needs driver
understanding + a zephyr build to confirm the variant actually compiles on `native_sim`.

### 275 W4 — threadx-riscv64 cyclone svc/action (RISK: may be unsupported)
`examples/fixtures.toml` comments this cell **"experimental Cyclone C/C++ (gated; talker/listener
only)."** The svc/action absence may be an intentional on-target limitation, not a missing row —
blind-adding rows could create failing fixtures. **Verify Cyclone svc/action actually run on
threadx-riscv64 QEMU first**; if they don't, keep de-scoped + document in README rather than add
broken rows.

### 275 W5 — stale cleanup (mostly already satisfied)
- `examples/px4/rust/uorb` and `examples/zephyr/rust/service-client-async` are **already
  de-scoped** in `examples/README.md` (rows 77 / 81) with rationale + a Phase-118.I lint guarding
  retired roots. They are documented exceptions, not silent gaps → H6 is effectively met for them.
  Deleting the (empty stub) dirs is optional cleanup; do it only alongside a lint run to confirm no
  breakage. **Left untouched** here to avoid an untestable lint break.
- Real remaining H6 item: `examples/stm32f4/rust/listener-embassy` is fully uncovered (sibling
  `talker-embassy` is compile-checked via `embassy_main_macro`). Fix (add a compile-check row) or
  delete — needs a build to confirm it compiles.

### 275 W6 — silent-gap gate
Add an explicit exception-allowlist to `examples_canonical_shape.rs` so a matrix cell without a
fixture (and without an allowlist entry) fails a shape test. Test code + a run required.

### 276 W1–W6 — capability-on-embedded (all build+run)
Each capability (parameters, RT-tiers-on-zephyr, lifecycle, safety/CRC, QoS, multihost) = an
embedded fixture + a runtime test asserting the capability on-target. Model exactly on
`packages/testing/nros-tests/tests/orchestration_tiers_freertos.rs` +
`scripts/build/compile-check-fixtures.sh:222` (`orch_tiers_freertos`). Targets: Zephyr `native_sim`
+ FreeRTOS QEMU. Heavy build+run; none doable blind.

## Verification checklist (run on known-good hardware)

1. `source ./activate.sh && just format && just check` — clippy/format clean.
2. `just build-test-fixtures` — the new native/rust rows build; add rows per item above and confirm.
3. `cargo test -p nros-tests` — existing + any new behavior tests green.
4. Re-run the coverage cross-check (the 2026-07-01 audit method in issue #102) to confirm the
   uncovered set shrank as intended.
