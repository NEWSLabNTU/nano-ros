# Phase 184 — Clean-room fixture / tooling gaps

**Goal:** Close the build/test-infrastructure gaps that make a full clean-room
cycle (`just clean` → `just setup` → `just build-all` → `just build-test-fixtures`
→ `just test-all`) report failing tests even though the product code + build are
green. None of these are product regressions — they are setup/fixture-aggregation
and one codegen-link gap.

**Status:** 184.1–.5 RESOLVED 2026-05-27 (184.1/.2/.3/.5 = clean build-only;
184.4 = build-fixtures-make builds host codegen); 184.6 fixed; 184.7 RE-VERIFIED
2026-05-27 (most reds were fixture-not-built preconditions + one test-timing bug,
now fixed). **184.8 OPEN** — the 3 zephyr cyclonedds action e2e tests DO
reproduce a hard failure here on fresh fixtures (real product gap, 177.2;
contradicts the 184.7 zephyr "GREEN" row); isolated to **post-match reliable
data delivery** (write OK, server RHC empty), frozen-clock lead REJECTED (Cyclone
clock advances). **Pinpointed to the wire:** the goal DATA is transmitted to the
server's data port (`udp/127.0.0.1:20411`) but the server's user-data-socket RX
never receives it (discovery/meta port works both ways; reliable retransmit
never recovers). Select-miss ruled out (a finite-timeout
select re-poll didn't recover); socket layout normal (1 data socket, like the
working fixtures; Recv-Q=0). Down to two sub-cases — **(a)** loopback loss
(datagram never reaches the server socket) vs **(b)** read-then-discard
(reader/RHC drops the action SendGoal request) — split by one Cyclone UDP-read
log (`ddsi_udp_conn_read`). Not Cyclone select/QoS/discovery.
**184.9 FIXED** — Zephyr Cyclone fixture build now force-reconfigures when a
backend source is newer than the linked binary (was: stale binaries broke
iterative diagnosis).
**Priority:** Medium.
**Depends on:** Phase 177 (archived — product fixes incl. the Cyclone idlc
re-resolve harden), Phase 175 (native Cyclone CMake/Corrosion fixtures).

## Resolution (2026-05-27) — one root cause: `clean` nuked the setup stage

All four "infra" symptoms below trace to a single bug: **`just clean` ran
`rm -rf build` (+ depended on `clean-zenohd`)**, deleting the SDK/tool installs
that `just setup` produces under `build/` (`build/{install,cyclonedds,qemu,
xrce-agent,zenohd,zephyr-cache}`). After a clean, the default (**base**-tier)
`just setup` is idempotent and doesn't rebuild them, so Cyclone (`build/install`
→ native/threadx cyclone fixtures skip their `[ -f build/install/lib/libddsc.so
]` gate), the XRCE Agent, and the patched qemu were all gone → the ~76 test-all
fails. (`build/install`, the XRCE Agent, and the patched qemu ARE installed by
`just setup tier=all` — `run xrce` + `run qemu` [→ `setup-qemu`] + `run
cyclonedds`; the clean-room run used base tier *and* clean had erased them.)

**Fix:** `clean` is now **build-stage only** — it removes examples, fixtures,
cargo `target/`, and the build-stage `build/` subdirs (`zephyr-fixtures`,
`esp32-qemu`, `qemu-zenoh-pico`), and **preserves the setup installs**. A new
**`clean-setup`** recipe removes the SDK/tool installs explicitly (full
re-setup). The per-platform `clean`s already only remove build-stage trees
(`build/cmake-*`, `build/rmw_zenoh_ws`), not the expensive installs.

**Setup vs build (the classification this enforces):**

| artifact | stage | `clean` | produced by |
|---|---|---|---|
| `build/{install,cyclonedds,qemu(patched),xrce-agent,zenohd,zephyr-cache}` | **setup** | preserved | `just setup tier=all` (per-module) |
| host `nros-codegen` CLI (`packages/codegen/packages/target/<profile>/nros-codegen`) | **setup** | preserved | `just setup` → `just workspace build-codegen` |
| `build/{zephyr-fixtures,esp32-qemu,qemu-zenoh-pico}`, `cmake-*`, `target/`, example `build-*/` | **build** | removed | `just build-all` / `build-test-fixtures` |

The host `nros-codegen` (the `nros` CLI / message-IDL codegen tool) is a
setup-stage TOOL: `just setup` builds it via `just workspace build-codegen`,
`clean` preserves it (the codegen cargo workspace is excluded), and
`clean-setup` removes it. Build recipes keep `nros_cargo_ensure_codegen_c` as an
idempotent safety net (now usually a no-op since setup built it). This is the
deeper fix behind 184.4 — `build-fixtures-make`'s FFI staging no longer depends
on a build recipe having happened to build codegen.

**Corrected workflow:** `just setup tier=all` **once** → then `clean → build-all
→ build-test-fixtures → test-all` repeatedly; the setup installs now survive
`clean`, so native/threadx Cyclone fixtures build (gate satisfied) and the XRCE
Agent + patched qemu stay present. `just clean-setup` only when you want a full
SDK re-install. **Per-platform setup-undo** (uninstall one platform's SDKs) is
deferred pending design discussion — `clean-setup` is a blanket nuke for now.

Only **184.4** (NuttX-cpp make-fixture link gap) remains — a real codegen/link
bug, unrelated to clean/setup.

## Overview

A full clean-room run on `main` (HEAD `59f47cef6`) was **build-green** —
`just clean` rc=0, `just setup` rc=0, `just build-all` rc=0 (**0 idlc-127**,
0 failures, vs exit 2 before the 177 idlc harden), `just build-test-fixtures`
rc=0 — but `just test-all` reported **760 run, 684 passed (5 flaky), 76 failed,
8 skipped**. Triaging the 76 (18 distinct test fns × params) showed **every
failure is one of four infra gaps below**, not a code regression. After applying
the per-issue workarounds, the 76 collapse to **2 residual hard fails**
(`nuttx_make_e2e` link gap + `threadx_linux_cyclonedds`, both below).

| Original test-all fail group | Count | Cause | After workaround |
|---|---|---|---|
| `c_xrce_api` | 10 | XRCE Agent not installed (184.3) | **pass** |
| `qemu_patched_binary` | 6 | patched qemu not built (184.3) | **pass** |
| `native_api` Cyclone | 9 | `build/install` nuked + native fixtures not built (184.1/184.2) | **8 pass**, 1 = threadx-linux below |
| `nuttx_make_e2e` | 1 | NuttX-cpp make-fixture link gap (184.4) | **still fails** |

## Work Items

### 184.1 — [RESOLVED]  `just clean` removes the Cyclone install; `just setup` doesn't restore it
- [x] `just clean` (→ `clean-examples clean-fixtures clean-zenohd`) removes
  `build/install/` (the Cyclone DDS install: `build/install/bin/idlc` +
  `build/install/lib/libddsc.so`). The umbrella `just setup` (default tier) is
  idempotent and, on a tree where `build/install` is gone, **does not detect or
  restore it** — its prior run was instant (rc=0) yet left `build/install`
  absent.
- Downstream: the native/ThreadX-Linux Cyclone fixture builds gate on
  `[ -f build/install/lib/libddsc.so ] && [ -x build/install/bin/idlc ]`
  (`just/native.just:169`), so with the install gone they **silently skip**
  ("native c/cpp cyclonedds skipped (run: just cyclonedds setup)"), leaving the
  fixtures unbuilt → `[SKIPPED]` test panics (counted as failures).
- **Workaround:** `just cyclonedds setup` restores `build/install` in ~4 s
  (the submodule build is cached; only the install step was missing).
- **Fix options:** (a) `just clean` should not delete `build/install` (it is an
  SDK install, not an example/fixture artifact); or (b) the umbrella `just setup`
  / `just cyclonedds setup` should re-run the install step whenever
  `build/install/{bin/idlc,lib/libddsc.so}` is missing, not skip on a stale
  "already set up" marker.

### 184.2 — [RESOLVED via 184.1]  `just build-test-fixtures` does not build the native / ThreadX-Linux Cyclone fixtures
- [x] The native Cyclone fixtures (`examples/native/<lang>/<case>/build-cyclonedds/`)
  are the Phase-175 CMake/Corrosion path built by **`just native build-fixtures`**
  (gated on 184.1's Cyclone install), and the ThreadX-Linux Cyclone fixtures by
  the threadx-linux path — **neither is invoked by the root
  `just build-test-fixtures`**. So a clean-room `build-all` + `build-test-fixtures`
  leaves them absent and the `native_api` Cyclone tests `[SKIPPED]`-fail.
- **Workaround:** `just cyclonedds setup && just native build-fixtures` →
  **8/9 native_api Cyclone pass** (the 9th is 184.5). Verified 2026-05-27.
- **Fix:** fold the native + threadx-linux Cyclone fixture builds into the
  fixture aggregation that `test-all` depends on (or document them as a required
  separate step in the clean-room runbook + the test skip messages).

### 184.3 — [RESOLVED via 184.1]  Default `just setup` omits the XRCE Agent and the patched qemu
- [x] On a fresh clean, `test-all` reports ~16 false fails because the default
  tier does not produce two tools the tests require:
  - **XRCE Agent** — `build/xrce-agent/MicroXRCEAgent`, built by `just xrce setup`
    (~1.5 min). Without it the 10 `c_xrce_api` tests fail.
  - **Patched qemu** — `build/qemu/bin/qemu-system-arm`, built by
    `just qemu setup-qemu`. Without it the 6 `qemu_patched_binary` tests fail.
- **Workaround (verified 2026-05-27):** `just xrce setup` + `just qemu setup-qemu`
  → all 16 pass.
- **Fix:** either include both in the default-tier `just setup` (note the
  SDK-tier policy in `docs/development/sdk-tiers.md` — patched qemu is a build,
  not a download), or have the tests' skip/remedy messages name the exact recipe
  (some already do).
- Note: a recursive `git pull --recurse-submodules` here hit transient GnuTLS
  errors fetching `third-party/xrce/agent` + `rmw_zenoh`; the Agent build later
  succeeded, so the earlier `build/xrce` absence was a not-yet-built state, not a
  hard network block.

### 184.4 — [RESOLVED] `nuttx_make_e2e` C++ make-fixture link failure (real codegen/link gap)
- [x] `nuttx_make_e2e::nuttx_external_apps_link_into_kernel_binary` fails because
  `just nuttx build-fixtures-make` (the NuttX external-apps → kernel link path,
  not the cmake `build-fixtures`) **fails to link the C++ examples**:
  ```
  .../cpp/service-server/generated/.../add_two_ints.hpp: undefined reference to
    `nros_cpp_deserialize_example_interfaces_srv_add_two_ints_request'
    `nros_cpp_serialize_example_interfaces_srv_add_two_ints_response'
  .../cpp/talker/generated/.../int32.hpp: undefined reference to
    `nros_cpp_publish_std_msgs_msg_int32'
  make[1]: *** [Makefile:217: nuttx] Error 1
  ```
  The generated C++ bindings reference per-type FFI symbols
  (`nros_cpp_{serialize,deserialize,publish}_<pkg>_<msg>`) that are **not provided
  in the make/kernel link** (the cmake `build-fixtures` path links them, but the
  apps/external make path's `libapps.a` does not pull the codegen FFI archive).
- **Actual root cause:** the link hook already exists — each cpp example
  Makefile `-include`s `generated/ffi/extra_libs.mk`, populated by
  `scripts/nuttx/gen-cpp-ffi-crates.py` (builds the per-package
  `nano_ros_cpp_ffi_<pkg>` staticlibs). But that script needs the host
  `nros-codegen` binary, and **`build-fixtures-make` never built it** (the cmake
  `build-fixtures` does, via `nros_cargo_ensure_codegen_c`). After `just clean`
  removed the codegen build, `gen-cpp-ffi` printed "nros-codegen not at …;
  skipping" → empty `extra_libs.mk` → undefined `nros_cpp_*` at kernel link.
- **Fix (RESOLVED, `just/nuttx.just`):** `build-fixtures-make` now sources
  `cargo.sh`, runs `nros_cargo_ensure_codegen_c`, and exports `NROS_CODEGEN` at
  the active profile dir before `stage-external-apps.sh` (mirroring the cmake
  path; `gen-cpp-ffi`/`gen-interfaces` read it). **Verified:** `gen-cpp-ffi`
  emits all 4 FFI staticlibs (incl. `example_interfaces`); a full
  `just nuttx build-fixtures-make` links the kernel ELF clean
  (`NMAKE_EXIT=0`, no `undefined reference`, `extra_libs.mk` = 4 lines).

### 184.5 — [RESOLVED via 184.1]  `test_threadx_linux_cyclonedds_talker_to_native_listener` (missing fixture)
- [x] Fast-fails (0.046 s) the same way the native Cyclone tests did — the
  ThreadX-Linux Cyclone fixture is not built by the clean-room aggregation
  (184.2). Should pass once that fixture is built (Cyclone install present after
  184.1). Folded into 184.2; tracked separately only to confirm after the
  fixture build lands.

### 184.6 — `graph.cpp` (177.36 `ros_discovery_info`) breaks the **zephyr cyclonedds** build (`build-all` RED)
- [x] **FIXED + landed on main** (`4c6ce2520`, 2026-05-27). Before this a fresh
  `build-all` on plain main was **RED** (36 zephyr cyclonedds fixtures); green now.
- 177.36 (`00bc53ef3`) added `packages/dds/nros-rmw-cyclonedds/src/graph.cpp`,
  which `#include "rmw_dds_common_graph.h"` — an **idlc-generated** header. The
  standalone `nros-rmw-cyclonedds/CMakeLists.txt` generates it via
  `NrosRmwCycloneddsTypeSupport` (idlc on `src/idl/rmw_dds_common_graph.idl`).
  The **Zephyr** path (`zephyr/CMakeLists.txt`) compiles the backend by
  `file(GLOB src/*.cpp)` — so it picked up `graph.cpp` but **never ran the
  graph-types codegen nor added its include dir** → all **36** zephyr cyclonedds
  fixtures (rust+c talker/listener/service/action) fail:
  `fatal error: rmw_dds_common_graph.h: No such file or directory`.
- **Fix (landed on the feature branch):** mirror the standalone codegen in the
  zephyr cyclonedds block — pre-set `IDLC_EXECUTABLE` (host idlc; no imported
  `CycloneDDS::ddsc` target on embedded), `include(NrosRmwCycloneddsTypeSupport)`,
  `nros_rmw_cyclonedds_idlc_compile(... rmw_dds_common_graph.idl ...)`, add the
  generated descriptor + register TU to the library sources and the output dir to
  the include path. `build-all` → exit 0 after the fix.
- Note: the earlier `CONFIG_NROS_DOMAIN_ID`-not-found error on the same fixtures
  was a **stale build dir** (cached `kconfig.rs` predating 177.38's symbol) — a
  clean rebuild cleared it and unmasked this graph.cpp gap.

### 184.7 — [RE-VERIFIED 2026-05-27] runtime `test-all` reds were NOT live product bugs
The baseline `test-all` (post-184.6) showed **7 hard failures** + 1 `[SKIPPED]`
(`nuttx_make_e2e`, see 184.4). Re-verification by **building each platform's
release/Cyclone fixtures and re-running the specific test** found that none of
the reproducible reds is a live product bug — they were stale-baseline /
fixture-not-built preconditions (the product fixes had already landed under
177.x), plus one genuine **test**-timing bug:

| Test | Owning phase | Re-verified status (2026-05-27) |
|------|--------------|----------------------------------|
| `large_msg test_zenoh_overflow_detection` | — | **FIXED (test bug, not flake).** The listener only prints `RECV_DONE:` at its internal timeout (every 2048B payload correctly overflows the 1024B shim buffer ⇒ `received=0`, `EXPECTED_COUNT` never reached); the test's `wait_for_output_pattern` raced that exact 15s deadline ⇒ missed the line ⇒ parsed `overflow_drops=0`. Product was correct (`overflow_drops=5`). Fix in `large_msg.rs`: listener `TIMEOUT_SECS` 15→8, test wait 15→25s, `unwrap_or_default`→`expect`. **PASS** (8.2s). |
| `rtos_e2e test_rtos_pubsub_e2e::Nuttx::Rust` | **177.30** | **already fixed; was fixture-missing.** Cold run fell back to `nros-fast-release` (177.8.c reboot loop). After `just nuttx build-fixtures`: **PASS** (45s). |
| `threadx_riscv64_qemu test_threadx_riscv64_cyclonedds_two_qemu_pubsub` | **177.26** | **already fixed; was Cyclone-fixture-missing.** After `just cyclonedds threadx-cross-probe` + `NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=1 just threadx_riscv64 build-fixtures`: **PASS** (5.2s) — the "executor register" blocker is resolved. |
| `native_api test_threadx_linux_cyclonedds_talker_to_native_listener` | **177.26** | **already fixed; was fixture-missing.** After `just cyclonedds setup` + `just threadx_linux build-fixture-extras`: **PASS** (10.1s). |
| `zephyr test_zephyr_dds_{c,cpp,rs}_action_e2e` (3) | **177.2** | **GREEN (not a product gap).** 177.2 closed 2026-05-23; the `k_thread`-ddsrt-port theory was debunked 2026-05-25 (diagnosed against stale `eth_posix` fixtures — Cyclone workers ARE `k_thread`s, `tid in use!` benign). On fresh NSOS fixtures the full `binary(zephyr) & test(dds)` group is **15/15 PASS** incl. all `*_action_e2e`. Clean-room reds were stale-fixture / `zephyr-native-cyclonedds` domain cross-talk (177.33/177.35). Not re-run in this SDK-less env. **⚠️ CONTRADICTED — see 184.8:** with a full `just zephyr setup` + fresh fixtures in this env, all 3 `*_action_e2e` deterministically FAIL (`server_received_goal=false`), solo + distinct domains. Treat 177.2 as OPEN. |

**Takeaway:** the clean-room `test-all` reds are dominated by *fixtures not built
for the specific platform* (the per-test `[SKIPPED]`/fallback messages name the
exact `just … build-fixtures` step). Only `test_zenoh_overflow_detection` was a
real defect — a test-side timing race, now fixed. None are Phase-172 (Group-1)
regressions — Group-1 touches only additive board `from_toml` parsers + the mps2
baremetal example's `nros.toml` + the 184.6 zephyr build fix; none of these test
paths.

### 184.8 — Zephyr CycloneDDS actions hard-fail (forwarded owner; Phase 177 archived)
*Phase 177 (incl. 177.2) is archived/closed. This live action-goal-delivery gap
is now owned here under 184.8; "177.2" below is the historical reference only.*

**Re-verified 2026-05-27 with a full `just zephyr setup` + `just zephyr
build-fixtures` in this env.** Contrary to the 184.7 zephyr row above (which
records 177.2 as GREEN / 15-15 PASS, "not re-run in this SDK-less env"), all
three `test_zephyr_dds_{c,cpp,rs}_action_e2e` **deterministically FAIL** here —
solo *and* grouped, on freshly-built fixtures, with distinct baked Cyclone
domains (c=52 / cpp=55 / rs=58, so not 177.33/177.35 cross-talk):

```
Action server ready: /fibonacci          (server, at boot)
Action client ready: /fibonacci          (client, +0.3s)
Sending goal: order=10                    (+3.3s)
Goal acceptance failed: Timeout           (client gives up)
Rust Cyclone action E2E failed (server_received_goal=false, client_completed=false)
```

**Not a timeout / discovery-wait issue.** Bumping the Rust client's goal-accept
wait 10 s → 45 s only moves the give-up to +49 s — still `server_received_goal=
false`. The goal request never reaches the server no matter how long the client
spins.

**Not transport-wide.** On the *same* Zephyr Cyclone fixtures,
`test_zephyr_rust_cyclonedds_pubsub_e2e` **PASS** (2 s) and
`test_zephyr_rust_cyclonedds_service_e2e` **PASS** (32 s — plain service match
is slow but converges). Only the **action goal service** fails to deliver.

**Root cause (CONFIRMED 2026-05-27 via instrumented `[cyc-dbg]` trace, since
reverted).** Action goals route through the *same* gated service-client path as
plain services: `ActionClientCore::send_goal` → `send_request_raw`/`call_raw` →
Cyclone `service_send_request_raw` → `maybe_flush_request` (`service.cpp`),
which buffers the request until `request_writer_matched()`
(`dds_get_publication_matched_status.current_count > 0`, the Phase 171.0.a
gate). The trace ruled out the earlier candidates:
- **descriptor OK / reader created** — no `NO-DESCRIPTOR` log; the server's
  send_goal request reader is created fine.
- **the gate opens + the request IS written** — `flush writer=… MATCHED,
  writing 44 bytes` fires (client-side `publication_matched.current_count > 0`).
- **but the server reader never matches** — server-side `POLL reader=…
  matched=0` (its `subscription_matched.current_count` stays 0); `server TOOK
  request` never fires.

> **CORRECTION (2026-05-28) — the "asymmetric match" reading above was a
> sampling artifact; the match actually SUCCEEDS.** Enabling Cyclone's own
> `<Tracing>Verbosity=finer` on the embedded config (since reverted) shows the
> send_goal request endpoints **fully connect, both directions**, at ~16–66 ms —
> well before the 3.3 s goal send:
> ```
> new_reader(… rq/fibonacci/_action/send_goalRequest/…Fibonacci_SendGoal_Request_)   # server
> new_writer(… rq/fibonacci/_action/send_goalRequest/…)                              # client
> reader_add_connection(pwr <client-wr>:203 rd <server-rd>:204)                      # server side matched
> writer_add_connection(wr <client-wr>:203 prd <server-rd>:204) - ack seq 0          # client side matched
> ```
> No `incompatible qos` / reject lines; client + server both on **domain 52**;
> topic + type **identical** on both sides (logged:
> `svc='/fibonacci/_action/send_goal' type='…Fibonacci_'
> req_topic='rq/fibonacci/_action/send_goalRequest'`). The earlier
> `subscription_matched=0` was the throttled poll catching the *pre*-match
> instant. So **discovery / matching / QoS / naming are all fine.**
>
> **The real failure is POST-MATCH reliable data delivery.** With the channel
> matched and the goal written *after* the match, the server's reader still never
> surfaces the sample: the action-server example spins correctly
> (`spin_once(100 ms)` + `try_accept_goal` every loop, `examples/zephyr/rust/
> action-server/src/lib.rs:82-98`) yet never logs `Goal request: order=` →
> `service_try_recv_request`/`dds_take` returns nothing. Cyclone delivers data via
> its own RX/dq `k_thread`s (the backend's `session_drive_io` only sleeps —
> `session.cpp:197`), so the goal DATA submessage either never leaves the client
> writer, never reaches the server via NSOS unicast loopback, or is dropped before
> the reader cache — **on this one action channel, while pubsub + plain service
> deliver fine over the same transport.**

**Fix attempts — ruled out (2026-05-27/28):**
- **Request QoS → TRANSIENT_LOCAL: REJECTED.** Stock ROS 2's goal/cancel/result
  services are RELIABLE+**VOLATILE** (`QOS_PROFILE_SERVICES_DEFAULT`,
  `nros-rmw/src/traits.rs:460`; only the *status* topic is TRANSIENT_LOCAL,
  :492). A TRANSIENT_LOCAL reader needs a TRANSIENT_LOCAL+ writer, so this is a
  durability mismatch that **breaks stock-ROS 2 action interop** (the goal of
  Phase 117). Off the table.
- **Client goal-accept timeout bump (10 s → 45 s): NO EFFECT.** Still
  `server_received_goal=false`; the request is written once at `send_goal` and
  dropped — spinning longer can't resend it.
- **Backend bidirectional flush gate (`maybe_flush_request` requires request-
  writer `publication_matched` AND reply-reader `subscription_matched`, the rmw
  `service_server_is_available` semantics) + 40 s client spin: STILL FAILS.**
  With the stricter gate the client waits the full 40 s and never gets accept —
  i.e. the action `send_goal` service's **reply-reader never matches either**, so
  the gate never opens. Reverted (it also risks regressing the *working* plain-
  service path, which only ever needed the unidirectional gate + example retry).

**What's ruled out (evidence, not speculation):** topic/type-name mismatch
(logged identical), QoS incompatibility (no reject in finer trace), discovery /
endpoint matching (trace shows full bidirectional `*_add_connection` at ~66 ms),
client-side gate/timeout (goal written after match; 45 s spin no help),
action-server spin (server polls `try_accept_goal` every 100 ms),
TRANSIENT_LOCAL (breaks stock interop). The bug is **narrower than any of
these**: a matched RELIABLE+VOLATILE write on the action `send_goalRequest`
channel does not surface at the server reader, **specifically** — pubsub + plain
service on the same participant pair / transport deliver fine.

**Data-path trace done (2026-05-28, `category=data,whc,rhc,traffic,radmin`,
reverted).** New facts:
- **The goal `dds_write` SUCCEEDS:** `flush WRITE writer=… len=44 ret=0
  pub_matched=1` — the client writes the 44-byte goal to a matched writer, no
  error.
- **The server reader's RHC receives nothing** — no `rhc`/store/deliver line for
  the server's `send_goalRequest` reader; the server-app never logs `Goal
  request: order=`. The sample is lost **between the client whc and the server
  rhc**, despite a successful matched write.
- **Two strong leads in the trace:**
  1. **Every Cyclone trace line is timestamped `00:00:00.000,002`** (frozen)
     while the app clock advances (3.3 s, 14.3 s). If the Cyclone threads see a
     **frozen ddsrt monotonic clock** on native_sim, the reliable
     heartbeat/NACK/retransmit **timers never fire** — a sample that isn't
     delivered by the single inline DATA write is never retransmitted. (Why
     plain RELIABLE service survives but the action doesn't is the open
     question — possibly the action's larger endpoint set shifts the inline-vs-
     timed delivery, or a per-channel inline-send difference.)
  2. **`<err> os: tid 0x… is in use!`** repeated at Cyclone startup (the k_thread
     tid-reuse 184.7 called "benign") — worth re-checking whether the action's
     extra threads (more endpoints ⇒ more Cyclone workers) push a tid collision
     that drops the **timed-event / xmit** thread.

**Clock check attempted (2026-05-28) — INVALIDATED + surfaced a build blocker.**
Tried to log `dds_time()` vs the app clock in `session_drive_io`, but:
1. The probe used an invalid `extern "C"` declaration *inside a function body* (a
   compile error), and
2. **`just zephyr build-fixtures` did not recompile the changed backend** — the
   built `zephyr.exe` still contained the *previous* round's (reverted)
   `[cyc-dbg]` strings and lacked the new one (verified with `strings`). The
   incremental fixture build silently skipped the cyclone-backend recompile, so
   the probe ran against a **stale binary** — and the staleness also masked the
   compile error until a forced `rm -rf build-*-cyclonedds && just zephyr
   build-fixtures` exposed it.
So the clock lead is **UNVERIFIED**. (The earlier *match-succeeds* and
*write-succeeds / RHC-empty* findings remain trustworthy — those binaries
provably contained their diagnostics: the Cyclone trace + the `flush WRITE`
string were present in the artifacts that produced them.)

### 184.9 — [FIXED 2026-05-28] Zephyr Cyclone fixture incremental build could reuse a stale backend binary
Observed: after editing `packages/dds/nros-rmw-cyclonedds/src/*.cpp`, `just
zephyr build-fixtures` (exit 0) sometimes did **not** relink the affected
`zephyr.exe` — the per-fixture incremental path runs bare `ninja -C <dir>`, and
a churned / half-failed build dir reused a stale backend object → **stale
binary**. This silently broke 184.8 diagnosis: a probe ran against an old binary
(verified via `strings`: the binary still held a *previous, reverted* `[cyc-dbg]`
marker), and the staleness even **hid a compile error** in the probe until a
forced `rm -rf build-*-cyclonedds` exposed it. (Intermittent — on fresh dirs
incremental works; the failure came from accumulated edit/rm churn.)

**Fix (`just/zephyr.just`, the per-fixture build-arg selection):** for
`*cyclonedds*` fixtures, if any `nros-rmw-cyclonedds/src` source is **newer than
the fixture's linked `zephyr.exe`**, force `needs_west=1` + `pristine=always` —
a clean west reconfigure that always picks up the backend edit. Fires **only**
right after a backend edit (`find … -newer …/zephyr.exe`), so a normal
`build-test-fixtures` (backend unchanged ⇒ exe newer than src) is unaffected,
and non-cyclone fixtures never trigger it. **Validated:** a marker added to
`session.cpp` then `just zephyr build-fixtures` (no `rm`) now lands in the
cyclone `zephyr.exe` (was `0`, now `1`).

**Now-unblocked clock check (184.8) — DONE, hypothesis REJECTED.** With reliable
rebuilds, logged `dds_time()` vs the app clock in `session_drive_io`: Cyclone's
clock **advances in lockstep** (`dds_time_ns == app_ms·1e6`, e.g.
`3524000000 ns` at `app_ms=3524`). **Not frozen** — the earlier "frozen
`00:00:00.000,002`" trace prefixes were a Zephyr LOG artifact, not Cyclone's
time source. So the frozen-clock / dead-timer lead is **wrong**; the reliable
heartbeat/retransmit timers do tick.

Owner: **184.8** (forwarded from archived **177.2**). Isolated to **post-match
reliable data delivery on the action send_goal channel** — match succeeds, goal
`dds_write` succeeds (`ret=0`), Cyclone clock advances, server RHC stays empty.
Discovery / QoS / naming / gate / timeout / frozen-clock **all ruled out by
evidence**.

**Wire-level pinpoint (2026-05-28, `category=trace,radmin` + `dds_wait_for_acks`
probe, reverted) — the writer transmits; the server's user-data RX never
receives.** Per the RTPS trace, at the 3.304 s goal write:
- the writer **does** emit the sample: `write_sample …:203 #1` (payload
  `{…,{10}}` = order 10) → `data(…:203:#1/1)` → `nn_xpack_send 128` to
  **`udp/127.0.0.1:20411`** → `traffic-xmit (1) 128`, plus a piggyback
  HEARTBEAT. `20411` is confirmed the **server**'s `default_unicast_locator`
  (data port; disc=20410).
- `dds_wait_for_acks(writer, 3 s)` = **`DDS_RETCODE_TIMEOUT (-10)`** — no
  matched reader acked the sample.
- the server **never receives any client→server user-data after discovery**:
  no `traffic-recv`, and the only `acknack 204 -> 203` is at 0.079 s
  (`F#1:1/0` = "expecting seq 1, have nothing"); **none after the 3.3 s write**,
  so the reader never even sees a heartbeat to NACK ⇒ reliable retransmit never
  recovers.
- all Cyclone RX threads start on both processes (`recvUC`, `recv`, `dq.user`,
  …); the 12× `tid in use` is the benign native_sim k_thread id-reuse, **not** a
  failed thread.

So: **discovery (meta port 20410) works both ways, but client→server
*user-data* on the data port 20411 never arrives at the server** — the writer
sends to the right port, the server's data-socket RX doesn't surface it, and
nothing retransmits. pubsub + plain service deliver over the same transport
(continuous flow / example retry masks any single-datagram loss; the action's
**one** goal datagram has no second chance).

**NSOS sockwaitset audit done — select-miss RULED OUT.** Cyclone's
`os_sockWaitsetWait` (`q_sockwaitset.c`) uses `ddsrt_select(..., DDS_INFINITY)`
(the `MODE_SELECT` POSIX path; native_sim is not LWIP/THREADX/WIN32). Hypothesis:
NSOS select drops a single readability event for the data socket → the recv
thread blocks forever on the one-shot goal datagram (continuous pub/sub traffic
self-recovers). **Tested:** patched the timeout to `DDS_MSECS(50)` under
`__ZEPHYR__` (level-triggered re-poll; verified compiled in — obj newer than the
edit, the select string present in the exe). Result: **still
`server_received=false`** — re-polling never surfaced the goal. So it is **not a
select-miss** (a buffered datagram would have been drained on re-poll). The
datagram either never arrives or is read-and-discarded — split below. (Reverted
the patch.)

**Host-socket forensics (`ss -ulnp`, running the fixtures standalone).** The
server **does** bind its data port: stable `0.0.0.0:20411` (data) + `:20410`
(meta) + an ephemeral — **one** socket per port, with `Recv-Q = 0` on 20411 (no
datagram sits buffered-unread). Socket layout is identical to the *working*
talker / listener / service-server fixtures (each binds 1 data socket). [A first
snapshot caught a startup **transient** of 2 fds on 20411 → an SO_REUSEPORT-split
theory; the stable state is 1 socket, so that theory is **rejected**.]

**Two remaining sub-cases (need to be split):**
- **(a) loopback loss** — the client's datagram never reaches the server's 20411
  host socket (NSOS `sendto`/loopback). Consistent with: writer-xmit confirmed,
  Recv-Q=0, finite-timeout re-poll found nothing.
- **(b) read-then-discard** — the datagram arrives + Cyclone's recvUC reads it
  (hence Recv-Q=0) but the reader/RHC drops it (action `SendGoal_Request_`
  type/QoS/dedup) so it never reaches the goal callback.

**`tshark` on lo (no sudo — `dumpcap` has `cap_net_raw`) — the goal IS on the
wire.** Captured `udp portrange 20400-20500` during the run; the **goal DATA
submessage is physically transmitted to the server's data port**:
`3.353 s  <client> → 127.0.0.1:20411  rtps DATA  wrEntityId=0x00000203` (the
send_goal request writer), followed by the writer's periodic HEARTBEATs (3.45,
3.55, …) to 20411. So **case (a) "never reaches the wire" is ruled out** — the
datagram reaches the loopback addressed to the server's data port.

**recvmsg works on the server's data socket.** A recv-path probe (later reverted,
see crash note) showed the server's `ddsi_udp_conn_read`/recvmsg returning
discovery datagrams on its data socket (sock=3, 176–900 B) — so recvmsg
functions; the data socket is being read during discovery.

**⚠️ `fprintf` from the recvUC k_thread crashes native_sim** — adding a raw
`fprintf(stderr, …)` in `ddsi_udp_conn_read` aborted the server (`ZEPHYR FATAL
ERROR 4: Kernel panic`, ~0.25 s) before the goal. The recvUC k_thread can't
safely call libc stdio on native_sim. **Any recv-path instrument must use
Cyclone's own `GVTRACE`/`category=trace` logging (thread-safe, already routed to
stderr by the log-flush patch), or gdb — not `fprintf`.**

**Refined split (still open):** the goal is on the wire to 20411 + recvmsg works
for discovery, yet the server's Cyclone shows **no goal processing after ~0.25 s**
(no traffic-recv/acknack/RHC; `server_received=false`), and the finite-timeout
select re-poll did **not** recover it. So either:
- the recvUC `select(DDS_INFINITY)` never wakes for the data socket once
  discovery traffic stops (NSOS select doesn't signal that fd's readiness for the
  one-shot goal), leaving the datagram unread in the kernel buffer; or
- the datagram is read but dropped at the reader/RHC.

**Next step (decisive, SAFE instrument):** add a `GVTRACE(gv, …)` (NOT `fprintf`)
in `ddsi_udp_conn_read` logging recv size + src after 3.3 s, OR `gdb` break on
`ddsi_udp_conn_read` in the server process; if the server recvmsg's the goal
datagram ⇒ reader-drop, else ⇒ select/kernel-delivery. (`tcpdump -i lo` already
confirmed the wire side.)

**Ruled out by evidence (cumulative):** discovery, writer↔reader match, QoS,
topic/type naming, the 171.0.a gate, client timeout, frozen clock, RX-thread
startup, select-miss (tested fix), SO_REUSEPORT socket-split (transient
mis-read), send-side / wire loss (tshark: goal DATA on the wire to 20411). ~40
trace/build cycles spent. Web refs: native_sim offloaded-sockets driver
[zephyr#65116], offloaded poll/recvfrom history [zephyr#94161].

> **Reconcile with the 184.7 row:** the prior "15/15 PASS" run delivered the
> goal data on this channel; here it deterministically doesn't (post-match).
> Treat as **open**, owned by 184.8 (177.2 archived).

## Acceptance
- A clean-room `clean → setup → build-all → build-test-fixtures → test-all` on a
  capacity-adequate host is green with no manual per-tool steps (184.1–184.3),
  OR the required steps are documented in a clean-room runbook and the skip
  messages name them.
- `nuttx_make_e2e` links (184.4).

## Notes
- ~~`build-all` is green~~ **Correction (2026-05-27, see 184.6):** the 177 idlc
  re-resolve harden fixed the idlc-127, but 177.36's `graph.cpp` then broke the
  **zephyr cyclonedds** build (36 fixtures) — `build-all` on plain main is RED
  until the 184.6 fix merges. The other 177 fixes (idlc re-resolve, zenoh fflush
  deadlock, service round-trip retry, `Executor::open` connect-retry) are in
  archived Phase 177 and hold.
- Cyclone test-runner domain isolation (concurrent native_sim nodes sharing DDS
  domains → SPDP cross-talk) is a *separate* concern tracked under 177.33/177.35;
  the 184 native_sim Cyclone fails here were missing-fixture, not concurrency
  (they pass `--test-threads=1` once the fixtures exist).
