# Phase 200 — `test-all` runtime-gap triage

**Goal.** Track and close the residual `just test` failures that remain after a
clean `nros setup` + `build-all` on a fully-provisioned host. These are
*runtime / feature* gaps, not setup or fixture-staging gaps — the latter were
all resolved during the 2026-05-29 sweep (see "Already fixed" below).

**Status.** Proposed (2026-05-29). Captured from a full local sweep on `main`
(`663 tests run: 643 passed, 20 failed, 110 skipped`). No new runtime fixes
yet — this doc inventories the 20 remaining failures so they route to the right
owning phase instead of being re-rediscovered each sweep.

**Priority.** P2 — none block setup/build; each is a real but bounded runtime
feature gap or an opt-in SDK shell. The zephyr CycloneDDS cluster (200.1) is the
largest and most product-relevant.

**Depends on.** Phase 177.2 (zephyr CycloneDDS actions + cross-impl), Phase
196.1 (zpico-link / zephyr fixture link), Phase 197 (`just setup` → canonical
`nros setup`). This phase does not duplicate those; it points at them.

---

## Overview

After the sweep, every remaining failure runs against a *built, booting*
fixture (or a deliberately opt-in SDK shell) — i.e. the binary exists and the
test executes; it fails on runtime behaviour or a known build-wiring gap, not
on a missing tool/fixture. The 20 cluster into five groups.

## Already fixed (this sweep — do **not** re-file)

- **Tool discovery** — `nros-tests` resolvers (`xrce_agent`, `zenohd`, `qemu`)
  now fall back to the `nros setup` SDK store via `nros_store_bin(tool, exe)`.
- **Zephyr host idlc** — `zephyr/CMakeLists.txt` resolved the retired
  `build/install/bin/idlc`; now resolves explicit env → nros store → PATH →
  legacy in-tree. Recovered every zephyr c/cpp CycloneDDS fixture *build*.
- **Zephyr fixture path** — `fixtures::binaries::zephyr_build_root()` now
  mirrors the build's `ZEPHYR_WORKSPACE` selection (in-tree → legacy
  `../nano-ros-workspace` sibling → build-tree), so c/cpp CycloneDDS e2e stopped
  false-skipping and now actually run.
- **zenoh-posix archive fixture** — `just build-zenoh-posix-fixture` staged;
  fixes `zenoh_archive_symbols`, `zenoh_header_parity`, `zpico_build_matrix`.

---

## Work Items

### 200.1 — Zephyr CycloneDDS runtime e2e (→ Phase 177.2 / 196.1) — 11

Fixtures build + boot on `native_sim`, but the CycloneDDS data plane does not
complete. Two sub-causes:

- **Data plane / discovery (runtime).** `zephyr_{c,cpp}_cyclonedds_pubsub_e2e`
  exchange no samples; `zephyr_{c,cpp}_cyclonedds_service_e2e` get no `[OK]`
  reply. Suspect embedded Cyclone discovery / multicast under native_sim
  (`<AllowMulticast>` + loopback). `zephyr_dds_{c,cpp,rs}_action_e2e` —
  actions on zephyr CycloneDDS not implemented (Phase 177.2 explicitly defers).
- **rust+cyclonedds link gap (build).** `zephyr_rust_cyclonedds_{pubsub,service}_e2e`
  and `zephyr_rust_{talker,listener}_cyclonedds_boot` fail to *build*, link-erroring
  on `zpico_open` / `zpico_spin_once` with no zenoh-pico in a CycloneDDS build.
  Zephyr rust **zenoh** builds clean — only the cyclonedds combo is unwired.

  **Root cause (2026-05-29, static — build-verify gated on the zephyr env).** Not
  the example sources (their `register_rmw()` is correctly `#[cfg(feature="rmw-*")]`
  per backend; Cargo deps are `optional`+feature-gated) and not the CMake's RMW
  fan-out (`zephyr/CMakeLists.txt` is `if NROS_RMW_ZENOH … elseif XRCE … elseif
  CYCLONEDDS …`; zenoh-pico sources only under ZENOH). The leak is a **misplaced
  network-wait helper**:
  - `nros` core's `mod zephyr` (`nros/src/lib.rs:289`, gated only on
    `platform-zephyr` — **not** on RMW) exposes `wait_for_network()` →
    `extern zpico_zephyr_wait_network`. Every zephyr rust example calls it (correct
    — the NIC must be up before any RMW init), so it's referenced in *every* RMW
    build.
  - That symbol is defined in `zpico-zephyr/src/zpico_zephyr.c` — **the same TU as
    `zpico_zephyr_init_session`, which calls `zpico_open()`** (zenoh-pico session
    API). The CMake compiles that TU only in the `CONFIG_NROS_RMW_ZENOH` branch.
  - So a cyclonedds build references `zpico_zephyr_wait_network` → drags in the
    zenoh-pico session API → undefined (zenoh-pico not compiled). Exactly the
    reported errors.

  **Why is a network-wait coupled to an RMW at all? It isn't — historical
  artifact.** `zpico_zephyr_wait_network` is pure Zephyr `net_if` / conn_mgr /
  `k_sem` polling (`zpico_zephyr.c:52-115`) — zero zenoh. It lives in the
  *zenoh-pico* support crate (`zpico-zephyr`, `zpico_` prefix) only because zenoh
  was the **first/only** Zephyr backend, so that crate's one TU bundled both the
  platform network-wait and the zenoh session-init. When cyclonedds/xrce landed,
  the platform-level wait stayed mis-filed under the zenoh crate. It is a platform
  primitive wearing a zenoh name.

  **Fix — relocate (chosen, IMPLEMENTED 2026-05-29).** The clean option:
  relocate the RMW-blind network-wait to the platform layer and rename it, so
  the `zpico_`/zenoh name no longer implies RMW coupling. (The minimal split —
  same-named standalone TU still inside `zpico-zephyr` — was rejected: it cures
  the link error but leaves a platform primitive mis-filed under the zenoh
  crate, and keeps the XRCE duplicate.)
  - **New canonical symbol** `nros_platform_zephyr_wait_network(int) -> int32_t`
    in `nros-platform-zephyr/src/net_wait.c`, declared in
    `nros-platform-cffi/include/nros/platform_zephyr.h`. Compiled in the
    **RMW-blind** common block of `zephyr/CMakeLists.txt` (alongside
    `platform.c`/`net.c`/`timer.c`), so zenoh / XRCE / CycloneDDS all link one
    copy.
  - **Folded in the XRCE-only `native_sim` stabilization grace** (2 s TAP/host
    socket settle) — it's a board property, not RMW, so every backend now gets
    it. The shared impl is the union: poll `net_if` up + carrier + preferred
    IPv4, *or* the `NET_EVENT_L4_CONNECTED` sem, then the grace.
  - **Stripped the originals:** `zpico_zephyr_wait_network` + its L4 helpers out
    of `zpico_zephyr.c` (leaves `init_session`/`shutdown`, zenoh-only); the
    duplicate `xrce_zephyr_wait_network` + `zpico_zephyr_wait_network` alias out
    of `xrce_zephyr.c` (leaves the `uxr_millis`/`uxr_nanos` clock symbols).
    Both old headers reduced to relocate stubs.
  - **Callers retargeted:** `nros` core extern (`nros/src/lib.rs`) renamed; all
    12 zephyr C/C++ examples drop the RMW-gated `<zpico_zephyr.h>`/`<xrce_zephyr.h>`
    include + `#if/#elif` wait block for one unconditional
    `nros_platform_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS)` call —
    **this also gives the cyclonedds C/C++ examples a network-wait for free**
    (they previously had no wait branch at all).
  - `cargo check -p nros --features platform-zephyr` passes; no live references
    to the old symbols remain (grep-clean, doc comments aside).
  - **Build-verify still gated on the zephyr env** (3-RMW-branch CMake): confirm
    zenoh still links, cyclonedds + XRCE now link, and the data plane (200.1
    runtime, below) before closing 200.1.

**Update (2026-05-29, after 200.6 unblocked the rust build):** the zpico-link
gap is **broader than rust+cyclonedds and broader than `wait_network` alone.**
Once 200.6 let rust zephyr compile, `just zephyr build-one rust/service-server
xrce` link-fails on the *full* `nros_rmw_zenoh` zpico shim —
`zpico_open`, `zpico_spin_once`, `zpico_declare_publisher`,
`zpico_publish_streamed`, `zpico_send_keep_alive`, `zpico_query_reply`,
`zpico_liveliness_get_{check,count}`, `zpico_init_with_config`, … (refs from
`nros-rmw-zenoh/src/{zpico.rs,shim/*.rs}`) — i.e. the entire `nros_rmw_zenoh`
rlib's object code is pulled into a **non-zenoh (xrce)** build, not just the one
`zpico_zephyr_wait_network` symbol. So removing the wait helper from the zenoh TU
is necessary but **not sufficient**: something still references
`nros_rmw_zenoh::zpico::Context` methods unconditionally. Re-confirm what keeps
the zenoh rlib live under xrce/cyclonedds (an un-`cfg`'d `use`/registration in
`nros` core or `nros-cpp`, or a non-feature-gated dep edge) before landing the
TU split. Rust zephyr **zenoh** builds clean — only the non-zenoh RMWs leak.

**Build-verified (2026-05-29, native_sim / zephyr 3.7) — relocate landed.** All
nine `talker` combos link: rust × {zenoh, xrce, cyclonedds} and c/cpp × {zenoh,
xrce, cyclonedds}. `net_wait.c.obj` exports
`T nros_platform_zephyr_wait_network`; the cyclonedds ELF has zero `zpico_open`
references — **the original rust+cyclonedds `wait_network` link failure is
fixed**, and cyclonedds c/cpp (previously no wait branch at all) now build.
**Leak claim RETRACTED (2026-05-29) — the relocate was *sufficient*, not just
necessary.** The "broader `nros_rmw_zenoh` shim leak" note above was the
*pre-relocate* symptom: `wait_network` lived in `zpico_zephyr.c`, the same TU as
`zpico_zephyr_init_session` → `zpico_open` → the whole zenoh session/shim path,
so referencing the wait helper dragged the entire chain into every RMW. Once the
wait helper moved to `net_wait.c` (RMW-blind, no zenoh), nothing in a non-zenoh
build references `nros_rmw_zenoh::zpico::Context` anymore. Confirmed on
`native_sim`: after fixing each example's `static mut LOCATOR` lint (below),
**`rust/{service-server,service-client,listener,action-server,action-client} xrce`
all link clean — zero `zpico_*` undefined.** No un-`cfg`'d edge remains; the TU
split contemplated by 200.6 is unnecessary.

**Remaining rust-example fix.** The only thing blocking the non-zenoh rust
builds after the relocate was a per-example Rust-2024 hard error — `&`/`&mut`
references to the `#[cfg(rmw-xrce)] make_config`'s `static mut LOCATOR`. Routed
through `core::ptr::addr_of_mut!` in all six rust examples (talker,
service-server, service-client, listener, action-server, action-client).

**Files (landed).** `packages/core/nros-platform-zephyr/src/net_wait.c` (new),
`packages/core/nros-platform-cffi/include/nros/platform_zephyr.h` (new),
`zephyr/CMakeLists.txt` (RMW-blind source list),
`packages/zpico/zpico-zephyr/src/zpico_zephyr.c` + `include/zpico_zephyr.h`
(stripped), `packages/xrce/xrce-zephyr/src/xrce_zephyr.c` +
`include/xrce_zephyr.h` (stripped), `packages/core/nros/src/lib.rs` (extern
rename), `examples/zephyr/{c,cpp}/*/src/main.{c,cpp}` (12 callers),
`examples/zephyr/rust/*/src/lib.rs` (static-mut + safety comments, all 6).
**Still open:** zephyr CycloneDDS *runtime* data-plane (the 200.1 e2e, separate
from the build gap) in `packages/testing/nros-tests/tests/zephyr.rs`,
`packages/dds/nros-rmw-cyclonedds/`.

### 200.2 — XRCE action/service runtime e2e — FIXED ✅

All 15 `xrce` + `c_xrce_api` tests pass (C and Rust, pub/sub + service +
action). Two distinct bugs:

**Bug 1 — registration ABI (fixed).** `xrce_service_{client,server}_create`
(service.c) were
missing the `const nros_rmw_qos_t *qos` parameter that the
`nros_rmw_vtable_t` `create_service_{client,server}` typedef grew in the Phase
193.5 QoS work. The cffi caller passed 7 args (`…, domain_id, &qos, &out`); the
C impls declared 6, so the impl read `&qos` as its `out` and wrote
`backend_data` into the QoS struct — the real `out->backend_data` stayed null,
and the cffi wrapper returned `ServiceClientCreationFailed` *after* the
requester/replier had actually been created successfully on the agent. Adding
the `qos` param (and honoring it, falling back to services-default when null)
restored registration. **Recovered:** `c_xrce_action_fibonacci` (+ the XRCE
service/action *registration* path for C / C++ / Rust, all of which share
`service.c`).

**Bug 2 — discovery race (fixed).** With registration fixed, the client sent
the request ~100ms after creating its requester — before RTPS discovery
matched the agent's request DataWriter to the server's reader. A reliable +
**volatile** request published pre-match is dropped, the reply never comes, and
`nros_client_call` (which sent once, then only spun for the reply) hung until
timeout. Confirmed via tshark: the request reached the agent and was ACK'd but
never forwarded to the server (`Total requests handled: 0`). The action
roundtrip survived because its longer lifecycle outlasts discovery.
**Fix:** resend the request every 500ms within the blocking call's spin loop
until the reply arrives or it times out. (Also corrected `xrce_dds_reply_type`
to `_Response_` for ROS interop — not required for nano↔nano routing.)

This same race is the documented NuttX cold-boot "call [1] times out" flake —
the resend should harden Phase 200.3's `rtos_e2e` service path too (verify).

**Files.** `packages/xrce/nros-rmw-xrce/src/{service.c,internal.h}` (ABI),
`packages/core/nros-c/src/service.rs` (resend),
`packages/xrce/nros-rmw-xrce/src/session.c` (reply type),
`packages/testing/nros-tests/tests/{c_xrce_api,xrce}.rs`.

### 200.3 — NuttX runtime e2e — DONE (verified passing)

`rtos_e2e::…Nuttx…Lang__C` service e2e and
`nuttx_make_e2e::nuttx_external_apps_link_into_kernel_binary`. Cross-ref Phase
194 (nuttx provisioning) — confirm whether these are provisioning-residual or
genuine runtime.

**Triage (2026-05-29, structural — NuttX-QEMU not runnable in this env).
Verdict: genuine, NOT provisioning-residual.** Both tests are correctly
precondition-gated — they `skip!` when `NUTTX_DIR` / `arm-none-eabi-gcc` / the
nightly `rust-src` / the built kernel are absent (`rtos_e2e::Platform::Nuttx::
require_e2e`, `nuttx_make_e2e` top-of-test guards). NuttX is in the **default
`just setup` tier**, so `test-all` provisions it and the tests *run* — a failure
therefore means the env was provisioned and it broke downstream:
- **`rtos_e2e` NuttX C service:** past `require_e2e`, `build_pair`
  (`rtos_e2e.rs:495/501`) **panics** on a fixture build failure, then the body
  asserts the service goal→reply completes over NuttX-QEMU. Either path = a
  genuine **build or runtime** gap (the C service fixture fails to build with the
  provisioned toolchain, or boots but the service exchange doesn't complete).
- **`nuttx_make_e2e` link:** skips on missing toolchain/kernel **and** skips when
  the kernel has *zero* nano-ros app symbols (make fixture unstaged → run
  `just nuttx build-fixtures-make`). Its only hard-fail paths are `nm` missing
  (env `panic!`) or **partial** `<prog>_main` linkage (the `assert!` — a genuine
  `Application.mk` `-Dmain=<prog>_main` rename gap where only some apps link).

**Action.** No precondition fix needed (tests are correctly gated). Reproduced
on a NuttX-provisioned host (cross-ref Phase 194) to confirm the actual mode.

**Resolution (2026-05-29 — verified on a provisioned host: `NUTTX_DIR` set,
`arm-none-eabi-gcc` + nightly `rust-src` present, kernel elf built, make
fixtures staged). Both tests PASS:**
- [x] `nuttx_make_e2e::nuttx_external_apps_link_into_kernel_binary` →
      `[PASS] all 12 nano-ros example PROGNAMEs linked into …/nuttx` (0.08s). No
      partial-`<prog>_main`-linkage; the `Application.mk` rename is correct on a
      freshly-staged kernel.
- [x] `test_rtos_service_e2e::…Nuttx::…C` → builds the C service fixture, boots
      NuttX-QEMU, runs AddTwoInts: `[PASS] 4 responses (completed=true)` (30.7s).
      build_pair did not panic; the goal→reply exchange completes. The service
      path already uses `wait_for_output_pattern` + a 45s `(Nuttx, C)` window
      (`rtos_e2e.rs:634`), so it's not the fixed-window fragility 200.4 had — no
      hardening needed.

So these are **not** runtime/build/linkage bugs — they pass when the env is fully
provisioned + the kernel/make-fixtures are freshly built. Both are correctly
precondition-gated (the linkage test `skip!`s on an unstaged kernel; the service
test gates on `require_e2e`). A red in `test-all` therefore means provisioning
was incomplete (kernel not built / `just nuttx build-fixtures-make` not run
before the tests) — a Phase 194 / CI-ordering concern, not a code fix here.

**Files.** `packages/testing/nros-tests/tests/rtos_e2e.rs`,
`packages/testing/nros-tests/tests/nuttx_make_e2e.rs`.

### 200.4 — ESP32 logging smoke — DONE

`logging_smoke_esp32_qemu_emits_every_severity`. **Triage result: the fixture is
correct** — a fresh `just esp32 build-logging-smoke` + a direct `qemu-system-riscv32
-machine esp32c3` run emits all six severities in order (trace→fatal); nros-log's
compile-time ceiling defaults to `Trace` even with `default-features = false`, and
the fixture sets the runtime level to `Trace` + flushes, so nothing is dropped.
The failure was the **test's fixed 30s window**: `wait_for_output(30s)` always ran
the full 30s and, under CI load, could expire mid-boot before every severity
flushed (and stale-fixture sweeps saw old output).
- [x] Switched the test to `wait_for_output_pattern("[FATAL] smoke: fatal
      payload", 90s)` — the last severity, so it returns as soon as all six are
      present (early-return), with a generous ceiling for slow esp32-qemu boots.
      A real backend regression now fails loudly (no `[FATAL]`). Verified: 3/3
      green on a fresh build, test time 30.01s → 0.08s; direct qemu run shows all
      six lines.

**Files.** `packages/testing/nros-tests/tests/logging_smoke.rs`.

### 200.5 — External opt-in SDK shells — 3 (expected-skip candidates) — DONE

`integration_{esp_idf,platformio,zephyr}_integration_shell_smoke` are static
shell smokes (assert the `integrations/<rtos>/` component files exist + carry the
expected markers — they do **not** build), gated by `nros_tests::skip!` when the
SDK env is absent (`IDF_PATH`/`idf.py`, `pio`, `ZEPHYR_BASE`/`west`).

**Decision (2026-05-29): `skip!` — keep as-is (NOT fail-loud + excluded).**
`skip!` panics with the `[SKIPPED]` marker, and the project's
`scripts/test/failed-filterset.py` / `just _count-real-failures` already treat a
`[SKIPPED]` failure as **not a real failure**. So a default-tier `just test`
shows these as precondition-skips, not failures — the "hard-fail" was a *raw*
`cargo nextest` artifact (a skip!-panic looks like a failure until the
`[SKIPPED]` reclassification). Excluding them from the filterset would instead
lose coverage when the SDK *is* present, so `skip!` is preferred per the
"check existence + skip, don't build" principle.

**Verified (2026-05-29):** ran the three on a default-tier host — `zephyr` →
`[SKIPPED]` (no `ZEPHYR_BASE`), `esp_idf`/`platformio` → pass (env/tool present,
file-asserts hold); `failed-filterset.py` on the JUnit returns **empty** (zero
real failures). No code change needed — the tests already implement the chosen
policy.

**Files.** `packages/testing/nros-tests/tests/integration_{esp_idf,platformio,zephyr}.rs`
(unchanged); `scripts/test/failed-filterset.py` (the `[SKIPPED]` reclassifier).

### 200.6 — Rust Zephyr build break (`export_kconfig_bool_options` + clippy gate) — FIXED ✅

Surfaced 2026-05-29 while rebuilding zephyr fixtures: every **Rust** zephyr
example failed its build script with
`error[E0425]: cannot find function export_kconfig_bool_options in crate zephyr_build`.

**Root cause — stale local workspace, not a code bug.** Phase 199.2 pinned
zephyr-lang-rust to a specific SHA (`404fcefd…`, west.yml) whose `zephyr-build`
both (a) renamed `export_bool_kconfig` → `export_kconfig_bool_options` (the
example `build.rs` was already updated to match in `b2492024d`) and (b) added a
**mandatory clippy build step** (`cargo clippy -- -D warnings
-D clippy::undocumented_unsafe_blocks`). The local zephyr workspace module was
stale at `248e23e` (pre-rename), so the new call name didn't resolve. C/C++
zephyr fixtures were unaffected (no rust build script).

**Resolution.**
1. **Workspace sync (setup, not repo):** `git -C modules/lang/rust checkout
   404fcefd…` (or `just zephyr setup` / `west update`) — the pin *does* export
   `export_kconfig_bool_options`.
2. **Clippy component (setup):** `rustup component add clippy --toolchain
   nightly-…` — the pin's build step shells out to `cargo-clippy`.
3. **Repo fix (`fab706056`):** the pin's clippy gate then failed on the
   examples' undocumented `unsafe` blocks (the `set_logger()` install + the
   `LOCATOR` static formatting). Added `// SAFETY:` comments to all unsafe
   blocks across the 7 rust examples.

Rust zephyr **zenoh** now builds clean (talker + listener verified, EXIT=0).
Rust zephyr **xrce/cyclonedds** still fail — but at *link*, on the full
`nros_rmw_zenoh` zpico shim (`zpico_open`/`spin_once`/`declare_publisher`/…),
which is the **200.1 zpico-link gap** (see note added there), not 200.6.

**Files.** `examples/zephyr/rust/*/src/lib.rs` (SAFETY comments, fixed);
setup-only: zephyr-lang-rust workspace pin sync + `clippy` rustup component.

---

## Acceptance

- [ ] 200.1 zephyr CycloneDDS c/cpp pubsub+service exchange data on native_sim
- [ ] 200.1 rust+cyclonedds zephyr links (zpico provider wired or backend-gated)
- [ ] 200.1 zephyr CycloneDDS actions implemented (or explicitly skip! pending 177.2)
- [x] 200.2 XRCE action/service e2e complete goal→result over the agent
- [x] 200.3 nuttx C service e2e + external-apps link pass (both verified green
      on a provisioned host; no code fix — tests correct + already robust)
- [x] 200.4 esp32 logging smoke emits every severity (fixture correct; test
      hardened to a pattern-wait — early-return + slow-boot tolerant)
- [x] 200.5 opt-in SDK shells gated as precondition-skip when SDK absent
      (`skip!` + `[SKIPPED]` reclassification — verified zero real failures)
- [x] 200.6 rust zephyr build (export_kconfig sync + clippy SAFETY comments) — zenoh green; xrce/cyclonedds remain on 200.1

## Notes

- Baseline sweep (2026-05-29, `main` @ post-codegen-retirement, nros 0.3.0):
  `663 run, 643 passed, 20 failed, 110 skipped`. Failure list is the union of
  200.1–200.5.
- The progression across the sweep was 53 → 39 → 24 → 20 failures as the
  setup/fixture-staging fixes landed; the floor of 20 is the runtime/external
  set tracked here.
- 200.5 is the only item with a *testing-policy* decision (skip vs. fail-loud);
  the rest are feature/runtime work owned by 177.2 / 196.1 / 194.
