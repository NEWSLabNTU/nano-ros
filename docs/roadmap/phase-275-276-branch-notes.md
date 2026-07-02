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
- **W3 (zephyr non-role leaves)** — DONE by re-audit: no real gap. The aemv8r cyclone leaves are
  built by the FVP recipes (missed by the 2026-07-01 audit); service-client-async is already
  de-scoped; `zephyr/cpp/talker-typed` is a package.xml-less orphan (W5 cleanup). See W3 below.
- **W6 (silent-gap gate)** — DONE: `examples_fixture_coverage.rs` green (~6s cold, 0.01s warm);
  clippy clean. The 17 remaining `*_entry` demos (W1) are its only tracked exceptions.

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

### 275 W1 — `*_entry` demos — PARTIALLY DONE (2026-07-02)
Re-scoped after investigation. The 18 `*_entry` dirs split three ways:
- **freertos (6) — already covered.** `freertos_run_plan_runtime.rs` boots ALL six roles
  (`boot_and_connect` per role), not just `talker`. NB it still `cargo build`s at test time
  (the compile-in-test antipattern) — a separate cleanup, but the dirs are exercised.
- **threadx-linux (6) — DONE.** Landed as 6 bare `[[fixture]]` rows (each Entry pkg bakes
  board+zenoh via `nros::main!` + the board shim) + `tests/threadx_linux_entry_build.rs`
  (prebuilt-only build-assert, no compile-in-test). Host x86_64 build; all 6 ELFs verified.
- **nuttx (6) — BLOCKED, issue #127.** Adding rows surfaced two bugs: (1) `nros sync` +
  `nuttx-libc-patch.sh` emit a duplicate `[patch.crates-io]` header → invalid TOML (a localized
  awk-insert fix works but is unexercised without (2)); (2) the standalone Entry-pkg `[[bin]]`
  fails to link against NuttX libc (`undefined reference to write/clock_gettime/__errno/exit`) —
  a per-platform link-wiring design gap. Reverted the rows + the libc-patch change; documented in
  #127. Still tracked as W6-gate exceptions (not silent).

### 275 W2 — native C/C++ variants (remaining)
Uncovered: native/c `{custom-msg, custom-platform, custom-transport-loopback, logging}`, native/cpp
`{component-poc, component-node-poc, transform-poc, logging}`. These need the **native C/C++ cmake
row schema** (`cmake_defs`, `codegen_out`, `build_subdir`, `rmw`, per-RMW `build-<rmw>/`), not the
3-line rust row. Mirror an existing native C/C++ row block. Higher risk blind — do on good hardware.

### 275 W3 — zephyr non-role leaves — DONE (2026-07-02): no real coverage gap
Re-checked each; the 2026-07-01 audit had missed the **FVP driver** (same class of miss as the
original #102 undercounting the zephyr role driver):
- `zephyr/rust/cyclonedds/talker-aemv8r` — **covered**: built by `just zephyr
  build-fvp-aemv8r-cyclonedds-rust` and run by `fvp_runtime_rust.rs`. Added to the W6 gate's
  `TEST_DRIVEN_BUILDERS` (was mistakenly allowlisted).
- `zephyr/cpp/cyclonedds/talker-aemv8r` — **covered** by `just zephyr build-fvp-aemv8r-cyclonedds`;
  has no `package.xml`, so it is not a gated leaf (built, not silent).
- `zephyr/rust/service-client-async` — already de-scoped in `examples/README.md` (Phase 212.M-F.5).
- `zephyr/cpp/talker-typed` — no `package.xml` + no build recipe: an orphan dir, not a matrix cell.
  Fix-or-delete belongs to W5 stale-cleanup, not a fixture to add.
No new fixtures needed; no README de-scope needed beyond what already exists.

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

## Phase 276 — blocker found (2026-07-02)

Zephyr provisioned + verified building (native_sim `c/talker` links). But the `nros::main!`
**Zephyr** emit branch wires only register+spin — no param-services / lifecycle / run_tiers (those
emits are `OwnedSpin`-only). So **276 W1/W2/W3 on Zephyr are macro-blocked (issue #128)**; adding a
fixture can't express the capability. **W4 (safety/CRC), W5 (QoS), W6 (multihost)** are node-level
pub/sub and remain achievable on Zephyr (ride the proven `zephyr_entry` register+spin path). Fix
direction for #128: extend the Zephyr arm to `OwnedSpin` parity (emit `param_services_call` +
`lifecycle_call`; add a `ZephyrBoard::run_tiers` for tiers).
