---
id: 164
title: "tests/zephyr.rs family: 29/45 fail on freshly built images — stale markers, the #163 backend gap, and untriaged xrce/action/entry lanes"
status: open
type: bug
area: testing
related: [issue-0157, issue-0163, phase-277, phase-286]
---

## Summary

The first FULL `just zephyr build-fixtures` sweep in a long time (during #161)
plus a `tests/zephyr.rs` family run: **16 passed / 29 failed / 1 skipped**.
The family had been "passing" on museum binaries that predate several reworks;
fresh images expose accumulated rot. Categorized:

### (a) Stale test markers — proven, mechanical to fix (the #157 class)

- `test_zephyr_c_service_server_to_client_e2e` (zenoh): the client PRINTS
  `Result of add_two_ints: 5` (delivery works); the test waits/asserts
  `"Result:"` which matches nothing. Same for the cpp variant and the other
  `"Result:"`/`"[OK]"` sites — ~6 occurrences in `zephyr.rs`
  (`nros_tests::output::SERVICE_RESULT_PREFIX` is the canonical marker).
- `test_zephyr_dds_cpp_{talker,listener,service_server,service_client}_boots`:
  the images boot and publish immediately (`Publishing: 'Hello World: 1'`,
  `session_open: domain=56` — the #161 bake visibly working); the tests grep
  pre-277 banners (`"nros Zephyr C++ Talker"` …) that phase-277 slimmed out.

### (b) Pure-Rust zenoh/xrce images have no backend — issue #163

`test_zephyr_talker_to_listener_e2e`, `test_zephyr_rust_service_e2e`,
`test_zephyr_action_e2e`, `test_zephyr_to_native_e2e`,
`test_zephyr_server_native_client`, `test_zephyr_xrce_rust_*` — all consume
`rs-*-{zenoh,xrce}` images, which since phase-248/249 contain no
`nros_rmw_{zenoh,xrce}_register` (see 0163). Freshly built images fail loudly
at `Executor::open`. Blocked on 0163's decision; not separately debuggable.

### (c) Untriaged — need their own look

- `test_zephyr_xrce_{c,cpp}_*` (talker/listener/service/action + cpp boots):
  C/C++ xrce images DO carry the backend (libnros_c.a) — could be more stale
  markers, the documented xrce runtime-lane debt, or real. Triage first
  against the marker list.
- `test_zephyr_dds_{c,cpp}_action_e2e`, `test_zephyr_dds_rs_action_e2e`:
  cyclone action lanes; phase_118 does not cover actions, so these have no
  recent LKG on fresh images.
- `test_zephyr_workspace_entry_native_sim_e2e`: west-lane zenoh entry image;
  listener side times out (`Listener timed out: Timeout` at 40 s). The entry
  images use the `nros_ws_runtime` umbrella (not the #163-affected app shape),
  so this is a distinct signal.

## Suggested order

1. Marker sweep (a) — mechanical, unlocks true signal for the rest.
2. Re-run family; re-categorize (c) with markers fixed.
3. (b) resolves with #163.

## Progress

**Step 1 (marker sweep) — DONE.** All stale markers in `zephyr.rs` fixed against
ground-truth example sources (compile-verified; runtime re-run still owed on a
built zephyr fixture host):

- **`"Result:"` → `SERVICE_RESULT_PREFIX`** ("Result of add_two_ints:", what
  `AddTwoIntsClient.cpp` actually prints): `test_zephyr_c_service_server_to_client_e2e`
  (zenoh) + `test_zephyr_xrce_c_service_e2e`.
- **`"[OK]"` / `"sum="` → `SERVICE_RESULT_PREFIX`**:
  `test_zephyr_xrce_cpp_service_e2e` + `test_zephyr_cpp_service_server_to_client_e2e`
  (the bogus `sum=` fallback dropped — no example prints it).
- **pre-277 `"nros Zephyr C++ <Role>"` boot banner → `"Booting Zephyr OS"`**
  (the kernel banner, the marker the passing `dds_c` `_boots` tests already use;
  phase-277 W5 removed the C++ banner — no source prints it anymore): all four
  `test_zephyr_dds_cpp_{talker,listener,service_server,service_client}_boots`, both
  `test_zephyr_xrce_cpp_{talker,listener}_boots`, and the stale-banner else-arm in
  `test_zephyr_cpp_talker_to_listener_e2e`.

**Step 2 (re-run + re-categorize) — DONE 2026-07-09.** Provisioned the host
(`just zephyr setup`, doctor OK), built all 66 fixtures, and ran the full
`--test zephyr` family TWICE — once before and once after #163's fix landed
upstream (the pre-#163 run's 15 `Executor::open ConnectionFailed` on the rust
lanes is obsolete; #163 is now resolved). Post-#163 result: **21 passed / 24
failed / 1 skipped**.

Marker sweep **validated**: 7 flips fail→pass (six C++ `_boots` now on
`"Booting Zephyr OS"`; `c_service_server_to_client_e2e` on `SERVICE_RESULT_PREFIX`).

Re-categorized the 24 remaining fails:

- **#163 (resolved) — rust `xrce` lanes now PASS** (gone from the fail list):
  `xrce_rust_{talker_listener,service,action}` green (the XRCE `host:port`
  locator bake + force-link register).
- **Staleness-guard false-positive (#147 class) — the rust `zenoh` lanes +
  workspace-entry**: `test_zephyr_{talker,listener,service_*}_smoke`,
  `rust_service_e2e`, `talker_to_listener_e2e`, `to_native`, the native↔zephyr
  crosses, and `workspace_entry_native_sim_e2e` all fail with `Zephyr fixture
  binary is stale: …/build-rs-*-zenoh/zephyr/zephyr.exe`. The images EXIST and
  are functional — the incremental rebuild relinked only #163's xrce images, so
  the untouched zenoh images predate a bumped source and the source-mtime-vs-image
  heuristic false-rejects them. A `--force` (clean) rebuild clears it; clean CI
  never hits it. Not an RMW failure — a resolver-heuristic fragility.
- **(c-XRCE C/C++) — real 0-delivery**: `xrce_{c,cpp}_{talker_listener,service,
  action}` (`client OK=0, server requests=0` / `got no reply`). The agent starts;
  the `libnros_c` XRCE path does not deliver on native_sim. Untouched by #163
  (that fix was the pure-Rust images). **Actionable residual.**
- **(c-cyclone completion) — `dds_{c,cpp,rs}_action` + `cpp_service_server_to_client`**:
  action = `server_received_goal=true, client_completed=false`; cpp service =
  `client OK=1` of 3 expected. Cyclone native_sim server RECEIVES but the
  result/completion round-trip is lossy. phase_118 covers only pub/sub, so no LKG.
  **Actionable residual.**
- **Residual (a) folded in**: the cpp service tests' server-side
  `count_pattern(server_output, "Request")` was itself a stale marker (the server
  prints `"Incoming request"` = `SERVICE_INCOMING_REQUEST_MARKER`); fixed here so
  the cyclone-completion diagnostic reads true instead of "server requests=0".

**Net:** (a) done + validated; (b) resolved via #163 (rust lanes green, modulo
the local staleness artifact); the live residuals are now their own issues —
**XRCE-C/C++ delivery → #174** and **cyclone action/service completion → #175**
(and the zephyr-pub→native-sub delivery bug below → **#173**).

**Update (phase-286 W1 slice 3, 2026-07-09) — zephyr-pub → native-sub delivery.**
Converting the rust pubsub cross tests to per-test ephemeral routers (for
parallelism) let them actually RUN past the staleness guard, and exposed a
distinct delivery bug: a **Zephyr publisher's samples never reach a native
subscriber** through the shared zenoh router. `zephyr_to_native_e2e` fails the
same way SERIALLY (native listener logs 0 `Received:` from the Zephyr talker), and
`bidirectional_native_zephyr_e2e` pins the asymmetry inside ONE router —
Native→Zephyr delivered 41 samples while Zephyr→Native delivered 0. Zephyr↔Zephyr
(`zephyr_talker_to_listener_e2e`) and Native→Zephyr both work, so it is
specifically the zephyr-pico **publisher → host-zenohd → native-subscriber** path.
Not a port/parallelism artifact (fails serial). Own follow-up.

## References

`packages/testing/nros-tests/tests/zephyr.rs`, archived issue 0157 (the
marker-fix pattern + `SERVICE_RESULT_PREFIX`), issue 0163, phase-277 W5
(banner slimming).
