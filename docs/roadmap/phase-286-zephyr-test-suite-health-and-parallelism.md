# Phase 286 — Zephyr test-suite health & parallelism (#166 speedup + #164 residuals)

Status: **In progress (W1 slice 1) — 2026-07-09** · Drives issue #166 (test parallelism) +
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

#### W1 slice 3 — pubsub-rust group parallelized (DONE 2026-07-09)

Converted the 4 rust pubsub e2e (`zephyr_talker_to_listener`, `zephyr_to_native`,
`native_to_zephyr`, `bidirectional_native_zephyr`) to `ZenohRouter::start_unique()`
+ `start_with_locator`/`NROS_LOCATOR=<ephemeral>`, and flipped
`qemu-zephyr-pubsub-rust` from `max-threads = 1` → `4`. `workspace_entry` split to
its own serial group `qemu-zephyr-ws-entry` (ws-runtime entry path — override not
yet wired). Also fixed a bypass gap: `get_prebuilt_zephyr_example` now honors
`NROS_SKIP_FIXTURE_CHECK` like the sibling guards (the #147-class mtime
false-positive otherwise aborts a content-current image after an inert source edit).

**Speedup measured (retries 0, same 4 tests):** SERIAL 152 s → PARALLEL **54 s
= 2.8×** (wall-clock = slowest test, not the sum; no port collisions). The
mechanism is proven: `zephyr_talker_to_listener` passes inside the parallel group.

**Uncovered (pre-existing, not parallelism):** the 3 cross tests fail on a
**zephyr-publisher → native-subscriber** delivery bug — `zephyr_to_native` fails
identically when run SERIALLY (native listener receives 0 from the Zephyr talker),
and `bidirectional` shows the asymmetry within ONE router (Native→Zephyr = 41 msgs
OK, Zephyr→Native = 0). Independent of the port change. Filed as a #164 residual.

#### W1 design findings (2026-07-09 exploration)

**Where the locator is fixed today.** The Rust entry macro
`nros::zephyr_component_main!` reads `const BAKED_LOCATOR = option_env!("NROS_LOCATOR")`
(`packages/core/nros/src/lib.rs:365`) — a compile-time const, baked by each example
`build.rs` from `CONFIG_NROS_ZENOH_LOCATOR` (and, for XRCE, synthesized
`host:port` from `CONFIG_NROS_XRCE_AGENT_{ADDR,PORT}`). The C/CFFI path consumes the
same Kconfig. One image ⇒ one fixed router endpoint ⇒ shared-port serialization.

**The runtime channel is argv, not host env.** native_sim `zephyr.exe` is a host
process that already takes CLI args — `ZephyrProcess::start` passes `--seed=<n>`
(`nros-tests/src/zephyr.rs:166`), consumed by Zephyr's fake-entropy driver, which
registers it via `native_add_command_line_opts(...)` + `NATIVE_TASK(..., PRE_BOOT_1, 10)`
(`drivers/entropy/fake_entropy_native_posix.c:111`). `PRE_BOOT_1` runs BEFORE the app
`main`, so a value parsed there is available when the entry macro runs. Host env is
**not** a viable channel: `nsi_host_getenv` is absent from this Zephyr 3.7 LTS tree,
and the embedded images are `no_std` (the `std::env::var` reads in `nros-cpp`/
`nros-node` are the *hosted native* fallback, not the native_sim path). So the
original "#166 runtime env" phrasing resolves concretely to a **native
command-line option**, not `getenv`.

**Proposed mechanism.**
1. Register an nros native option `--nros-locator=<loc>` (mirror the `--seed`
   pattern: `native_add_command_line_opts` + `NATIVE_TASK(PRE_BOOT_1)`) in a
   native_sim-only TU under the zephyr platform layer; stash the parsed string in a
   `static`.
2. Add a platform hook `nros_runtime_locator_override() -> Option<&str>` — returns
   the stashed value on native_sim, `None` everywhere else (real QEMU/hw compile it
   out). BOTH read sites honor it: the Rust `zephyr_component_main!` macro (prefer
   over `BAKED_LOCATOR`) and the C CFFI shim (prefer over the Kconfig locator).
3. Harness: a `ZephyrProcess::start_with_locator` (or extend `start`) that binds
   `TcpListener::bind("127.0.0.1:0")` for a free port, starts a per-test zenohd on
   it, and passes `--nros-locator=tcp/127.0.0.1:<port>` to both fixture processes.
4. nextest: delete the six `qemu-zephyr-{pubsub,service,action}-{rust,cpp}`
   `max-threads = 1` groups (fall through to the parallel `qemu-zephyr` group).

**Scope / edges.** native_sim only — the baked value stays the default so QEMU/hw
are untouched (they have no host arg channel and no host zenohd). **XRCE: a single
`--nros-locator` carries both** (decided 2026-07-09) — `tcp/host:port` for zenoh, a
bare `host:port` for xrce, exactly as the example `build.rs` already unifies the two
RMW shapes into `NROS_LOCATOR`. One option, one static, one hook. Two read sites
(Rust macro + C shim) both need the hook — miss one and that lane silently keeps the
baked port. The existing `--seed` already de-conflicts client *source* ports; this
override de-conflicts the *router* port, orthogonal and complementary.

**Effort.** ~1 small native-C TU (option registration + static) + the
`Option<&str>` hook wired into two read sites + the harness port-alloc/zenohd
plumbing + the nextest group deletion. No fixture rebuild contract change (the bake
remains the default).

**Mechanism confirmed (host-env ruled out).** `nsi_host_trampolines.h` (Zephyr
3.7 native-simulator) exposes only `nsi_host_{malloc,free,calloc,realloc,open,close,
read,write,random,srandom,strdup,getcwd,isatty}` — **no `nsi_host_getenv`**. So a
host-env read is impossible from the native_sim image; the **native command-line
option is the sole channel**, registered exactly like `--seed`:
`native_add_command_line_opts(&opts)` (decl in `boards/native/native_sim/cmdline.h`,
`struct args_struct_t` with `.type = 's'` for a string `.dest`) under a
`NATIVE_TASK(fn, PRE_BOOT_1, 10)` hook, all guarded `#if defined(CONFIG_ARCH_POSIX)`
so real embedded builds compile it out.

**Implementation plan (vertical slices).**
1. **Infra + Rust slice (first, provable alone) — DONE 2026-07-09.** Added
   `nros_runtime_locator_override()` to `nros-platform-zephyr/src/platform.c`
   (`CONFIG_ARCH_POSIX`-guarded; reads `-testargs --nros-locator=<loc>` via
   `nsi_get_test_cmd_line_args`) + its decl in `nros/platform.h`;
   `zephyr_component_main!` (`nros/src/lib.rs`) prefers it over `BAKED_LOCATOR`;
   harness `ZephyrProcess::start_with_locator` passes `-testargs …`, and
   `test_zephyr_talker_to_listener_e2e` now spins its own `ZenohRouter::start_unique()`
   (ephemeral) + override. **Proven:** the test passes over ephemeral port 42391
   with NO router on the baked port 7456 (delivery is impossible unless the images
   dialed the override). `nros` still compiles for non-zephyr (macro is
   `rmw-cffi`-gated). Group-flip deferred to slice 3 — `qemu-zephyr-pubsub-rust`
   has 4 other members (`zephyr_to_native_e2e`, `native_to_zephyr_e2e`,
   `bidirectional_native_zephyr_e2e`, `workspace_entry_native_sim_e2e`) still on
   fixed ports; flipping before they convert would collide.
2. **C/C++ read sites:** honor the override where `app_config.h` sets
   `.locator = CONFIG_NROS_ZENOH_LOCATOR` (C) and `main.hpp`'s `NROS_ENTRY_LOCATOR`
   (C++) — a runtime `if (override) use it` at entry, not the header const.
3. **Extend to service/action + xrce** (unified `host:port` form) and delete the
   remaining five serial groups.
4. **Rebuild + re-run** the family; confirm wall-clock drop and no #141 collisions.

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
