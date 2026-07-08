# Phase 286 — Zephyr test-suite health & parallelism (#166 speedup + #164 residuals)

Status: **Draft — 2026-07-09** · Drives issue #166 (test parallelism) +
the live residuals from issue #164 (zephyr family re-triage) to resolution ·
Follows #163 (resolved — pure-Rust backend restored).

> **Goal.** Make the `tests/zephyr.rs` family both **fast** and **honestly
> green**. Two independent tracks surfaced by the 2026-07-09 full-family re-run
> (21 passed / 24 failed / 1 skipped on freshly built native_sim fixtures, after
> #163's fix): (1) the serial-port parallelism ceiling (#166), and (2) real
> delivery/completion debt plus a staleness-guard false-positive masking it.
> Do **W1 (#166) first** — it is the highest wall-clock win and unblocks faster
> iteration on the remaining tracks.

## Context — the 2026-07-09 re-triage (issue #164, step 2)

Provisioned the host (`just zephyr setup`, doctor OK), built all 66 fixtures, ran
`--test zephyr` twice (pre/post #163). Post-#163: **21 / 24 / 1**. Validated the
marker sweep (7 flips fail→pass: six C++ `_boots` → `"Booting Zephyr OS"`, the C
zenoh service → `SERVICE_RESULT_PREFIX`), and fixed a residual server-side marker
(`"Request"` → `SERVICE_INCOMING_REQUEST_MARKER`; the server prints
`"Incoming request"`). The 24 remaining fails partition into the work items below.

## Work items

### W1 — #166 test parallelism (DO FIRST)

The zenoh e2e lanes serialize on a build-time-baked router port. The
`nros_tests::platform::ZEPHYR` scheme already gives unique per-(variant, lang)
ports (xrce parallel at 7, dds at 4), but six groups —
`qemu-zephyr-{pubsub,service,action}-{rust,cpp}` — stay `max-threads = 1` because
**multiple tests reuse one fixture image** whose port is baked
(`-DCONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:$zenoh_port"`, `zephyr-fixture-leaves.sh`).

**Direction (see #166 for the full design):** on native_sim, prefer a **runtime**
`NROS_LOCATOR` (env) over the compile-time `option_env!` bake. Each test then
allocates an ephemeral free port, starts its own zenohd, and passes the locator
via env — unique port per test, zero static coordination — retiring all six
serial groups. The baked value stays the default so real QEMU / hardware images
(which cannot reach a host env) are unaffected.

**Acceptance:** the six `qemu-zephyr-{pubsub,service,action}-{rust,cpp}` groups
run at host-core width; a full `--test zephyr` wall-clock materially below the
current ~292 s; no router-port collisions (the #141 hazard) under parallelism.

### W2 — staleness-guard false-positive (#147 class)

The rust `zenoh` lanes and `workspace_entry_native_sim_e2e` fail with `Zephyr
fixture binary is stale: …/build-rs-*-zenoh/zephyr/zephyr.exe` even right after a
full `build-fixtures`. The images are functional; the source-mtime-vs-linked-image
heuristic (`nros-tests` `binaries/mod.rs`) false-rejects an image the incremental
build did not need to relink. Clean CI does not hit it, but the guard is fragile.

**Acceptance:** the staleness check no longer false-positives on a
correctly-built-but-not-relinked image (compare against the build-manifest / a
content hash, or gate on the actual inputs, not wall-clock mtime); the rust zenoh
lanes report their TRUE runtime verdict.

### W3 — XRCE C/C++ delivery on native_sim (real 0-delivery)

`xrce_{c,cpp}_{talker_listener,service,action}` deliver nothing
(`client OK=0, server requests=0` / `got no reply`) though the agent starts.
#163 fixed the pure-**Rust** xrce images; the `libnros_c` XRCE path is untouched
and does not deliver on zephyr native_sim.

**Acceptance:** C and C++ XRCE pub/sub + service + action deliver end-to-end on
native_sim (parity with the now-green rust xrce lanes).

### W4 — Cyclone action/service completion (native_sim)

`dds_{c,cpp,rs}_action` = `server_received_goal=true, client_completed=false`;
`cpp_service_server_to_client` = `client OK=1` of 3. The Cyclone native_sim
server RECEIVES the goal/request but the result/completion round-trip is lossy.
phase_118 covers only pub/sub, so these action/service lanes have no LKG.

**Acceptance:** Cyclone action goal→feedback→result and multi-call service
complete end-to-end on native_sim across c/cpp/rs; add them to the phase_118-class
coverage so they don't silently rot again.

## Sequencing

W1 (#166) → W2 (staleness — unblocks true rust-zenoh signal) → W3 (XRCE-C/C++)
→ W4 (Cyclone completion). W3/W4 are independent runtime tracks and may proceed
in parallel once W1/W2 land.

## References

Issue #166 (parallelism design), issue #164 (the family re-triage — this phase's
source), issue #163 (resolved — pure-Rust backend), the #147 staleness class,
`packages/testing/nros-tests/{src/platform.rs,tests/zephyr.rs}`,
`.config/nextest.toml`, `scripts/build/zephyr-fixture-leaves.sh`.
