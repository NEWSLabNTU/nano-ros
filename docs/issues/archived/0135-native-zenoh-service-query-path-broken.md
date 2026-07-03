---
id: 135
title: "Native zenoh service/action query path broken — client get returns Transport(Timeout) instantly, server never receives"
status: resolved
type: bug
area: rmw-zenoh
related: [phase-277, issue-0096]
resolved_in: "phase-277 follow-up (build_c_shim generated-config fix)"
---

## Resolution summary

**Root cause: C ABI mismatch between the zpico shim TU and the zenoh-pico
library TUs, introduced by the issue-0096 loopback fix (`8e6a5cf2a`).**

- `8e6a5cf2a` enabled `Z_FEATURE_LOCAL_SUBSCRIBER` / `Z_FEATURE_LOCAL_QUERYABLE`
  in the **generated** config header (`<OUT_DIR>/zenoh-config/zenoh_generic_config.h`)
  for host targets. The zenoh-pico library sources get that header
  (`build_zenoh_pico_unified` defines `ZENOH_GENERIC` + adds the include dir).
- The legacy `build_c_shim` path (POSIX + bare-metal) compiled `c/zpico/zpico.c`
  **without** `ZENOH_GENERIC` and without the generated include dir, so the shim
  fell back to the in-tree `zenoh-pico/config.h` defaults — `LOCAL_* = 0`.
- `z_get_options_t` **gains a field** (`allowed_destination`) under
  `Z_FEATURE_LOCAL_QUERYABLE == 1`, so the struct layouts diverged: the library's
  `z_get` read the shim's `opts.target` (`Z_QUERY_TARGET_ALL` = 1) as
  `opts.allowed_destination` (`Z_LOCALITY_SESSION_LOCAL` = 1). Every cross-process
  query was silently downgraded to session-local: the local leg found no
  queryable, the pending query finalized instantly with no reply →
  `Transport(Timeout)` in under a second, and the router never saw the query
  (verified with router debug logs + a temporary `_z_query` printf:
  `allow_local=1 allow_remote=0`).
- The 0096 regression guard (in-process round-trip) kept passing because
  session-local was exactly the leg the mismatch preserved; pub/sub was
  unaffected (no publisher-side struct field shifts).

**Fix** (`packages/zpico/nros-zpico-build/src/runner.rs`):
`build_c_shim` now defines `ZENOH_GENERIC` and puts the OUT_DIR generated
config first on the include path (posix additionally mirrors the manifest's
`Z_FEATURE_MULTI_THREAD=1` / `ZENOH_DEBUG=0`); the stale pre-134.3
`c/platform/zenoh_generic_config.h` copy that could shadow the generated
header was deleted; `probe_net_type_sizes` gets the generated include too.
Bisect: `8e6a5cf2a^` good / `8e6a5cf2a` bad; per-entity node identity
(`6601c7e52`) exonerated.

**Verified:** native zenoh service/action nextest suites 11/11
(request/response, sequential, multi-client, xprocess, action e2e, AND the
0096 in-process round-trip); pub/sub + Cyclone service + ros2-interop 7/7.
Note for local checkouts: prebuilt `target-zenoh/<profile>` fixture binaries
embed the broken shim until rebuilt — run `just build-test-fixtures` (tests
resolve the `nros-fast-release` profile dir, not `debug`).
