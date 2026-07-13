---
id: 181
title: "build-test-fixtures exits 0 with whole lanes unbuilt (esp32, px4, freertos/threadx-linux rust) — tests then fail 'not prebuilt'"
status: resolved
resolved_in: "phase-287 follow-up (2026-07-13)"
type: bug
area: build
related: [issue-0164, phase-287]
---

## Summary

A full `just build-test-fixtures` run (2026-07-12, post phase-287 W6) printed
its lane markers and **exited 0**, yet `test-all` then failed a dozen tests
with `Test fixture binary not prebuilt` / `Binary not found after build`:

- `esp32_emulator` ×5 + `logging_smoke_esp32`: `examples/qemu-esp32-baremetal/
  rust/talker/target/riscv32imc.../esp32-qemu-talker` absent ("Binary not
  found after build" — the esp32 lane's cargo build produced nothing and the
  sweep still exited 0).
- `px4_xrce` ×2: `examples/px4/rust/xrce/px4-stub/...` never built.
- `rtos_e2e` rust lanes (freertos pubsub/service, threadx-linux
  pubsub/service): `examples/qemu-arm-freertos/rust/talker/target-zenoh/...`
  absent — the freertos lane's `build-examples` (rust) half didn't run in the
  sweep even though `build-fixture-extras` (C/C++) did.
- `native/rust/{listener,service-client-callback}` STALE (generated source
  newer than binary) — the native rust example rebuild didn't cover them.

## Why it matters

The sweep is the staleness gate's ground truth: "exit 0" is read as "every
lane fresh", so these tests fail red in `test-all` and look like runtime
bugs. The mtime-treadmill pitfall (CLAUDE.md) makes this recurrent.

## Fix direction

Per-lane build steps must fail loudly (or emit an explicit `[SKIP <lane>:
<reason>]` that the fixture-staleness gate understands) instead of exiting 0
with nothing built. Audit: esp32 lane (espup toolchain probe), px4 lane
(PX4-Autopilot checkout probe), the rust half of freertos/threadx-linux
`build-examples`, and the native rust example set.

## Progress

**W1 — the rust-lane silent skip (freertos / threadx-linux / native) — FIXED
2026-07-13.** Root cause: `nros_require_ws_sync` (`scripts/build/cargo.sh`) — the
prereq guard every rust fixture build calls before `nros ws sync` (which
materialises each example's `generated/` msg crates) — printed `[PREREQ]` and
**`exit 0`** when the verb was unavailable. That aborted the ENTIRE rust half of
`build-examples` before a single binary was built, and the make graph read the
0 as success. Changed it to **`exit 1`** (fail loud): a fixture build genuinely
cannot proceed without `ws sync`, so a stale/wrong in-tree CLI is an actionable
setup error (`just setup-cli`), not a skippable lane. The verb is present on the
normal in-tree CLI, so the sweep's happy path is unchanged; the guard now only
fires — loudly — when the CLI is stale, exactly the case that previously produced
a green sweep + a dozen `fixture not prebuilt` reds. `scripts/build/fixtures-build.sh`
+ `workspace-fixtures-build.sh` + the per-platform recipes all inherit the fix.

**Remaining (own follow-ups):**
- **esp32 / px4** are legitimately toolchain-gated (best-effort). Fail-loud is
  wrong for them; they need an explicit `[SKIP <lane>: <toolchain> absent]` that
  the TEST side reads to `skip!` (not hard-fail on `get_prebuilt_*`). Today the
  esp32 flash-image step already `WARNING`-skips on absent `espflash`, but the
  build's cargo step producing no ELF still exits 0 and the test hard-fails.
- **native rust codegen coverage** — PARTIAL FIX 2026-07-13: `service-client-callback`
  (a fixtures.toml row consumed by `native_api.rs::build_native_service_client_callback`)
  was absent from the native rust codegen loop (`just native build-fixture-rust` /
  `-core`), so its `generated/` msg crates were never synced and the binary went
  stale. Added it to both loops. Still open: the plain-`target/` vs `target-<rmw>/`
  variant matrix — some tests read `target/` (`build_native_listener`) while the
  loop stages per-RMW dirs; a full coverage audit against what each test consumes
  is a separate follow-up.

## Resolution (2026-07-13)

Upstream `5411bd49d` fixed the loudest mechanism (missing `ws sync` verb
exit-0-skipped the whole rust half; now fails loud + the native codegen loop
covers service-client-callback). This follow-up closes the rest:

- **esp32 + px4 lanes added to BOTH sweep drivers** (`build-test-fixtures-leaves`
  fallback make-graph + jobserver loop in the justfile; `build-all.mk`
  PLATFORMS/INDEPENDENT_FIXTURE_PLATFORMS — esp32 was already in the mk, px4
  in neither; the fallback had neither).
- **esp32 name drift**: the crates' `[[bin]]` names are underscored
  (`esp32_qemu_talker`) while the recipe + harness probed hyphenated names —
  espflash opened a nonexistent ELF and the lane "passed". Recipe packs the
  ELF under its real name (flash `.bin` keeps the historical hyphenated name
  every consumer references); the harness now CONSUMES the prebuilt ELF
  (`require_prebuilt_binary`) instead of `cargo build`-ing in-test.
- **freertos rust roles**: the role crates are lib-only (212.L) — the harness
  probed a `qemu-freertos-<role>` bin they can't produce. Builders now consume
  the `<role>-entry` release images (the NuttX pattern); the six fixture rows
  gained `target = "thumbv7m-none-eabi"` (they built for the HOST before —
  exit 0, no board artifact); the six entries bake per-variant ports
  (7451/7461/7471) instead of a uniform 7451, pair members carry distinct IPs
  (10.0.2.15/.16 — the ZID lesson), and rtos_e2e gained a freertos-rust
  launcher arm (default-slirp plan) + entry-banner readiness gates.
- threadx-linux rust + the stale native rust leaves were sweep-ordering
  staleness, not gaps.

Verified: px4 2/2, esp32 boots + logging green, freertos rust boots + opens
its session on the right port. Residual runtime reds (0-delivery) filed as
**#191** (freertos rust entries) and **#190** (esp32 cross-delivery).
