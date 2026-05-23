# Phase 161 — C++ FreeRTOS `nros::init -> -100` TransportError triage

**Goal.** Restore green on the 3 C++ FreeRTOS E2E tests (pubsub /
service / action) that fail in `just freertos test-all` after the
Phase 144.5.c migration to `add_subdirectory(<repo-root>)`. The
Rust and C variants pass on the same fixture, so the bug is
C++-specific.

**Status.** **CLOSED 2026-05-19.** Root cause: nros-cpp's
`rmw-zenoh-cffi` feature pulled `nros-rmw-zenoh` as a Rust dep,
which bundled a second copy of the zenoh-pico C build into
`libnros_cpp.a` alongside the one already coming from the
standalone `libnros_rmw_zenoh.a` (linked via
`nano_ros_link_rmw(... RMW zenoh)`). With
`--allow-multiple-definition` the linker reconciled same-named
symbols but each rlib instance kept private trampolines wired to
its private zenoh-pico variant — runtime FFI layout mismatch
surfaced as `nros::init -> -100`. Phase 134.fix landed the
identical fix on nros-c (2026-05-12); nros-cpp was never
migrated.

**Fix:** Drop `dep:nros-rmw-zenoh` from nros-cpp's
`rmw-zenoh-cffi` feature; declare `nros_rmw_zenoh_register` as a
plain `extern "C" { fn nros_rmw_zenoh_register() -> i32; }`
symbol resolved at the C-binary link step from
`libnros_rmw_zenoh.a`. Drop the redundant
`nros_rmw_zenoh::register()` call from `nros_cpp_init` — the
CMake-emitted strong stub at
`cmake/NanoRosLink.cmake:62-117` already calls
`nros_rmw_zenoh_register()` via
`nros_app_register_backends()`.

**Result:** `cargo nextest run -p nros-tests --test rtos_e2e
"platform_1_Platform__Freertos"`: **9/9 PASS** (was 6/9 — all 3
C++ variants previously failed with -100 TransportError).

**Priority.** P1 — blocks the "no FreeRTOS QEMU E2E regression"
acceptance gate of Phase 141 (`docs/roadmap/phase-141-wake-callback-cortex-m3.md`),
and visibly red on every `just freertos test-all` run.

**Depends on.** Phase 144 (add_subdirectory migration, archived),
Phase 155.C (`nros_cpp_init` NodeError decoder, lands the catch-all
mapping that hides the real variant today).

---

## Symptom

```
$ cargo nextest run -p nros-tests --test rtos_e2e \
    test_rtos_pubsub_e2e::platform_1_Platform__Freertos::lang_3_Lang__Cpp

[freertos cpp] pubsub: starting talker/listener...
FAIL — readiness pattern 'Waiting for messages' not observed.
Output so far (truncated):
nros C++ Listener (FreeRTOS)
[nros] examples/qemu-arm-freertos/cpp/zenoh/listener/src/main.cpp:19
       nros::init(NROS_APP_CONFIG.zenoh.locator,
                  NROS_APP_CONFIG.zenoh.domain_id) -> -100
```

Same shape on the C++ service-client and C++ action-client variants
(error at `cpp/zenoh/service-client/src/main.cpp:19` and
`cpp/zenoh/action-client/src/main.cpp:83`). All three return -100
immediately at `nros::init` — before the callback / spin loop runs.

Last full run (2026-05-19, branch `phase-141-a-3`, no code diff vs
main): 6/9 PASS, 3/9 FAIL — all three C++ variants on FreeRTOS.
Rust (3/3) + C (3/3) PASS.

## What `-100` means now

After Phase 155.C (`packages/core/nros-cpp/src/lib.rs:534-554`),
`-100` (`NROS_CPP_RET_TRANSPORT_ERROR`) is no longer the
single-variant connection-failed code — it's the *catch-all* for:

- `NodeError::Transport(t)` where `t` is `ConnectionFailed`,
  `Disconnected`, or any variant not specifically remapped
  (lines 545-551).
- Any `NodeError` not matched by an earlier arm
  (`_ => NROS_CPP_RET_TRANSPORT_ERROR` at 552).

So the current log line **doesn't identify which precondition
the backend rejected**. Surfacing the actual `NodeError` variant
is step 1 of the triage.

## Confirmed NOT the cause

- **Port scheme.** Per-(variant, lang) split via
  `packages/testing/nros-tests/src/platform.rs:142`:
  - Pubsub: 7451 (Rust) | 7551 (C) | 7651 (C++)
  - Service: +10 (7461 | 7561 | 7661)
  - Action: +20 (7471 | 7571 | 7671)

  C++ configs (`examples/qemu-arm-freertos/cpp/zenoh/<dir>/config.toml`)
  match these ports.
- **Fixture binding.** `ZenohRouter::start_slirp(port)` binds
  `0.0.0.0:<port>` (`packages/testing/nros-tests/src/fixtures/zenohd_router.rs:131`)
  — same path Rust and C use.
- **Backend registration.** `cmake/NanoRosLink.cmake:62-117`'s
  `nano_ros_link_rmw(... RMW zenoh)` emits a strong-stub
  `nros_app_register_backends()` that calls
  `nros_rmw_zenoh_register()`. The `#[unsafe(no_mangle)]` symbol
  exists at `packages/zpico/nros-rmw-zenoh/src/lib.rs:140`.
- **Phase 141.A.3.** The 141.A.3 wake-cb wire-up landed already
  (mirrored into spin.rs:806-842 / 1516-1583 / 3614-3652). The
  branch that probed 141.A.3 had 0 code commits vs `main`, so
  this regression is independent.

## Suspect surface

1. **Phase 144.5.c migration (6588f6b87, 2026-05-18).** Last
   change to qemu-arm-freertos cpp examples — switched from
   `find_package(NanoRos CONFIG)` + `include(freertos-support)`
   to `add_subdirectory(<repo-root>)`. C examples migrated in the
   same commit and pass; C++ may have a subtle linkage drift
   (weak-stub override order, missing TU, or `nros-cpp` feature
   flag not propagating through the `add_subdirectory` graph).
2. **`nros-cpp` rmw-zenoh-cffi feature gate.** `nros_cpp_init`
   only calls `nros_rmw_zenoh::register()` under
   `#[cfg(feature = "rmw-zenoh-cffi")]`
   (`packages/core/nros-cpp/src/lib.rs:474-477`). The CMake-emitted
   strong stub at `cmake/NanoRosLink.cmake:88-117` is the
   redundant safety net; if both paths fail the listener
   prints -100 silently because `Executor::open` later trips
   on a missing-backend in the registry.

## Work items

- [x] **161.1 — Surface real NodeError variant.** Patch
      `node_error_to_cpp_ret` (`nros-cpp/src/lib.rs:535-554`)
      to `eprintln!("[nros_cpp_init] {err:?}")` once before
      returning the mapped code, gated on `feature = "std"`
      OR a dedicated `feature = "diagnose-init"` so the no_std
      embedded build doesn't drag `format!` in.

- [x] **161.2 — Re-run C++ FreeRTOS pubsub with the probe.**
      Capture the exact NodeError variant. Expected:
      `Transport(ConnectionFailed)` — but `Transport(Other(_))`,
      `NotInitialized`, or `Serialization` would each point at
      a different root cause.

- [x] **161.3 — Linkage diff.** Compare `nm
      target/.../freertos_c_listener.elf` vs `freertos_cpp_listener.elf`
      for:
      - `nros_rmw_zenoh_register` symbol presence
      - which TU defines `nros_app_register_backends` (the
        CMake-emitted strong stub vs the Rust weak fallback)
      - `__nros_rmw_register_*` linkme distributed-slice entries

      Phase 128.B.1 / 128.H.2 (per `nros-rmw-zenoh/src/lib.rs:170-180`)
      established linkme as the canonical auto-registration path
      with a macro fallback for unsupported targets; if the C++
      build is missing the strong stub AND linkme isn't running,
      the registry is empty.

- [x] **161.4 — Compare init paths.** `nros_c_init`
      (`packages/core/nros-c/src/lib.rs`) vs `nros_cpp_init`
      (`packages/core/nros-cpp/src/lib.rs:453-530`) — the C path
      works on FreeRTOS, the C++ path doesn't. The diff in the
      `register()` cfg-feature scaffold (lines 470-481) is the
      obvious suspect once 161.2 confirms which backend is missing.

- [x] **161.5 — Land the fix.** Probably one of:
      - Add the missing feature flag to `nros-cpp` at the FreeRTOS
        consumer side (CMakeLists snippet or `Cargo.toml` of the
        cpp shim crate).
      - Fix `add_subdirectory` propagation of the rmw-zenoh-cffi
        feature into the cpp-linked rlib.
      - Add an explicit `nros_rmw_zenoh_register()` call from
        the strong stub if linkme isn't running on the target.

- [x] **161.6 — Verify the Phase 141 regression gate.**
      `just freertos test-all` returns 9/9 PASS.

## Files (when 161.5 lands)

- `packages/core/nros-cpp/src/lib.rs` (probe in 161.1, possibly
  feature scaffold fix in 161.5)
- `cmake/platform/nano-ros-freertos.cmake` or
  `cmake/NanoRosLink.cmake` (if the fix is on the link path)
- `examples/qemu-arm-freertos/cpp/zenoh/*/CMakeLists.txt` (if
  a per-example feature flag is missing)

## Acceptance criteria

- [x] `cargo nextest run -p nros-tests --test rtos_e2e
      "test_rtos_*_e2e::platform_1_Platform__Freertos::lang_3_Lang__Cpp"`
      passes all 3 C++ FreeRTOS variants.
- [x] `just freertos test-all` returns 9/9 PASS.
- [x] No regression on Rust + C variants (already 6/6 PASS).
- [x] Probe `eprintln!` from 161.1 either reverted before
      landing OR moved behind an explicit diagnostic feature
      that isn't enabled by default.

## Notes

- The Phase 141 doc explicitly carved this triage out of 141's
  scope ("Triage 3 C++ FreeRTOS TransportError failures —
  separate bug, not 141 scope") on 2026-05-19. This phase doc
  is the place to land that work.
- Don't conflate with Phase 160.B / 160.K (also Transport-class
  errors but on NuttX + ThreadX, both closed). The FreeRTOS C++
  failure pattern is distinct from those — `nros::init` returns
  immediately, no readiness banner, no subsequent backend
  activity in the log.
