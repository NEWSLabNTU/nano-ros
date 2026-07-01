# Phase 275/276 — branch implementation notes (2026-07-01)

> Working notes for branch `phase-275-276-example-fixture-coverage`. The dev host has failing RAM
> (issue #115) so the original staging pass was **unverified**. Below: what was done, what each
> remaining item actually requires (study revealed most are NOT mechanical row-adds), and the risks.

## Verified on a known-good machine (2026-07-02)

- **W2 (native, all langs)** — `just native build-fixture-rust` + `build-fixture-extras` build
  green; all 3 rust + 4 c + 4 cpp new rows produce artifacts (`build-zenoh/<exe>` confirmed).
- **W5 (stm32 listener-embassy)** — `cargo check --target thumbv7em-none-eabihf` clean.
- **W4 (threadx-riscv64 cyclone svc/action)** — DONE by de-scope: documented in
  `examples/README.md` "Intentionally empty cells" (runtime blocked on Phase 177.22; two-QEMU e2e
  is pub/sub only). No unverified rows added.
- **W6 (silent-gap gate)** — DONE: `examples_fixture_coverage.rs` green in ~6s; clippy clean. The
  17 remaining `*_entry` demos (W1) + zephyr rust cyclone leaf (W3) are its tracked exceptions.

## Done on this branch (verified above)

- **275 W2 (native, all langs) — build-assert rows.** Added `examples/fixtures.toml` rows for every
  native variant example that shipped with zero fixtures:
  - native/rust: `service-client-async`, `action-client-async`, `logging` — bare rows
    (`default = ["rmw-zenoh"]`, same shape as covered `listener`).
  - native/c: `custom-msg`, `custom-platform`, `custom-transport-loopback`, `logging` — `rmw="zenoh"`
    rows (all use the standard `-DNROS_RMW` cmake path).
  - native/cpp: `component-poc`, `component-node-poc`, `transform-poc`, `logging` — `rmw="zenoh"` +
    `target=<cmake project NAME>` (mirrors the `cpp/parameters` row; NAMEs verified from CMakeLists).
  **Verify:** `just build-test-fixtures` builds them; then add runtime assertions (async client
  receives; custom-transport-loopback round-trips; logging sink emits) under `nros-tests/tests/`.
- **275 W5 (stm32 listener-embassy).** Added `listener-embassy` to `compile-check-fixtures.sh`
  `CARGO_CHECK_EXAMPLES` (id `embassy_main_macro_listener`, cargo-check-only like `talker-embassy`,
  which lacks the board memory layout to link) + a stamp-asserting test fn in
  `stm32f4_embassy_main_macro.rs`. **Verify:** the cross target installs + the check stamps.

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

### 275 W3 — zephyr non-role leaves (partly already de-scoped)
`nros_fixture_roles()` in `scripts/build/fixture-matrix.sh` lists exactly the 6 roles; these leaves
sit outside it. **Finding:** `zephyr/rust/service-client-async` is **already dropped/de-scoped** in
`examples/README.md` (row 81, "Dropped 2026-06-02 per Phase 212.M-F.5") → leave it (W5/H6 territory,
not a fixture to add). For `zephyr/{cpp,rust}/cyclonedds` and `zephyr/cpp/talker-typed`: decide
whether to add a small non-role enumeration to the zephyr driver **or** de-scope in the README —
needs driver understanding + a `native_sim` build to confirm each actually compiles (cyclone on
zephyr especially). Not a mechanical add; do on good hardware.

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
