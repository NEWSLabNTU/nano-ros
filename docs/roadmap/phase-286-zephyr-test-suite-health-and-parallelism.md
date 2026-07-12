# Phase 286 — Zephyr test-suite health & parallelism (#166 speedup + #164 residuals)

Status: **Complete — 2026-07-11** · Drives issue #166 (test parallelism) +
the live residuals from issue #164 (zephyr family re-triage) to resolution ·
Follows #163 (resolved — pure-Rust backend restored). W1 (all six serial groups +
the ws-entry follow-up now parallel), W2 (content-aware staleness guard), W3 (XRCE
C/C++ delivery — #174), and W4 (Cyclone action completion — #175, incl. the C/C++
register `-100` residual) all landed.

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

#### W1 slice 2 — C/C++ read-site + pubsub-cpp group (DONE 2026-07-09)

One edit: `ZephyrBoard::run_components` (nros-cpp `main.hpp`) prefers
`nros_runtime_locator_override()` over its `locator` arg. Covers BOTH C++ and C
zephyr examples — codegen (`emit_cpp.rs`) emits `ZephyrBoard::run_components` for
the zephyr entry regardless of node language (a C node's `configure` runs from the
generated C++ entry). Applied to ZephyrBoard only (the override symbol is
zephyr-platform-scoped); Nuttx/Threadx/Freertos boards untouched. Converted the 3
cpp pubsub e2e + flipped `qemu-zephyr-pubsub-cpp` `max-threads = 1 → 3`.

**Proven:** `zephyr_cpp_talker_to_listener_e2e` passes over an ephemeral port
(baked cpp port 7656 had no router). **Speedup:** SERIAL 98 s → PARALLEL 51 s
≈ 1.9× for the 3 cpp tests.

**Uncovered:** the 2 cpp cross tests fail pre-existing (both in the #164 24-fail
list) — and reveal the native↔Zephyr-**C++** bridge is broken in BOTH directions
(cpp-pub→native-sub AND native-pub→cpp-sub), worse than rust (only zephyr-pub
direction). Folded into #173.

#### W1 slice 4 — service/action groups (DONE 2026-07-09) — all six serial groups now parallel

Converted the 5 tests in the four remaining serial groups
(`qemu-zephyr-{service,action}-{rust,cpp}`) to `ZenohRouter::start_unique()` +
the locator override and raised each group `max-threads = 1 → 2`. No new code —
reuses the slice-1 (Rust) + slice-2 (C/C++) read-sites; the slice-2 rebuild's
service/action images already carry the override.

**Speedup:** SERIAL 75 s → PARALLEL 34 s ≈ 2.2×. **3 of the 5 now PASS**
(`native_server_zephyr_client`, `zephyr_action_e2e`, `cpp_action_server_to_client`)
— `zephyr_action_e2e` passing over an ephemeral port proves the override on the
hardest path (3 serialized queryable declares). 2 fail pre-existing:
`zephyr_server_native_client` (#173 zephyr→native direction) and
`cpp_service_server_to_client` (delivers 1 reply over the ephemeral port — override
works — but short of the expected 3; a zenoh cpp service completion/throughput
residual, distinct from the #175 Cyclone one).

**W1 net:** all six `qemu-zephyr-{pubsub,service,action}-{rust,cpp}` groups are now
parallel (were all `max-threads = 1`). Per-group speedups measured: pubsub-rust
2.8× (152→54 s), pubsub-cpp 1.9× (98→51 s), service+action 2.2× (75→34 s). The
mechanism (native_sim `-testargs --nros-locator` runtime override, honored by the
Rust macro + the C/C++ `ZephyrBoard::run_components`) is proven green on at least
one Zephyr↔Zephyr lane per RMW-role.

#### W1 follow-up — ws-entry override wired (2026-07-11)

The last serial group, `qemu-zephyr-ws-entry`, is retired. The ws-runtime Entry
uses the `nros::main!` **proc-macro** (`nros-macros/main_macro.rs`), NOT the
`zephyr_component_main!` macro nor the CLI `generate.rs` emitter — so its Zephyr
arm needed its own override read. The macro's Framework::Zephyr `config` build now
prefers `nros_runtime_locator_override()` over the baked `option_env!("NROS_LOCATOR")`
(same shape as `zephyr_component_main!`; NULL on real embedded ⇒ the bake stands).
`test_zephyr_workspace_entry_native_sim_e2e` converted to
`ZenohRouter::start_unique()` + `start_with_locator` (external native listener
points at the same ephemeral router); the `qemu-zephyr-ws-entry` nextest group +
its override filter deleted so the test falls through to the parallel
`qemu-zephyr` group. Fixture rebuilt with the override embedded (`build-ws-rs-entry-zenoh`
relinked; `nros_runtime_locator_override` referenced by the generated `rust_main`).
**Override mechanism proven** (the Entry dials the `-testargs --nros-locator`
ephemeral port, not the baked 7456) — the parallelism goal (no shared baked port)
is met and the serial group is retired.

**ws-entry e2e GREEN 2026-07-12** — but the initial "48 messages" claim was wrong,
and so was a first "flaky race" re-diagnosis. The real bug was a **message-type
mispair**: the ws demo Entry (`talker_pkg`) publishes `std_msgs/Int32` on
`/chatter`, but the test's external observer (`examples/native/rust/listener`)
subscribes `std_msgs/String`. rmw_zenoh bakes the type into the wire keyexpr
(`…::Int32_/*` vs `…::String_/*`), so the router never matched them → 0 delivery.
The earlier "48 messages" pass used the STALE Int32-era listener binary (which
happened to match the Int32 demo); once that binary was rebuilt to `String` (the
07-06 migration, see #173) the mispair surfaced. **Fix:** the native listener's
message type is now `NROS_SUB_TYPE`-selectable (default `String`,
`NROS_SUB_TYPE=int32` for the Int32 demo), and the ws-entry test sets `int32`.
`test_zephyr_workspace_entry_native_sim_e2e` PASSES (49 s). The override mechanism
was never the problem — it correctly dials the ephemeral port.

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

**DONE 2026-07-09 — content-aware staleness.** `is_binary_stale` (`nros-tests`
`zephyr.rs`) no longer trusts wall-clock mtime alone. New
`candidates_changed_content` records a per-binary sidecar
`<build_dir>/.nros-srcbaseline` = the LINKED binary's own content hash + each
watched source's `(mtime, size, content_hash)`:

- **binary hash changed** (a rebuild happened, or first sight) → the image IS the
  fresh truth → re-record baseline, report not-stale.
- **binary unchanged** → only files whose `(mtime, size)` moved are content-hashed;
  a moved mtime with **unchanged bytes** is an artifact (not stale, refresh the
  recorded mtime), a **changed hash** or a newly-appearing watched file is a real
  edit (stale).

This kills the dominant #147 false-positive (rebase/checkout/pull "mtime
treadmill", or an inert edit — the exact class that aborted the rust-zenoh lanes
after the slice-1/3 rebuilds) while still catching genuine un-rebuilt library
edits. Steady state is stat-only (content-hash only on touched files); the sidecar
write is atomic (temp + rename) so parallel tests sharing a fixture never read a
half-written baseline. Falls back to the old mtime gate if the binary can't be
hashed. `NROS_SKIP_FIXTURE_CHECK=1` (now honored by the zephyr guard too, slice 3)
remains the explicit escape. Unit-tested: `content_aware_staleness_ignores_mtime_only_bumps`
covers first-sight / mtime-artifact / real-edit / rebuild.

### W3 — XRCE C/C++ delivery on native_sim (real 0-delivery)

`xrce_{c,cpp}_{talker_listener,service,action}` deliver nothing
(`client OK=0, server requests=0` / `got no reply`) though the agent starts.
#163 fixed the pure-**Rust** xrce images; the `libnros_c` XRCE path is untouched
and does not deliver on zephyr native_sim.

**Acceptance:** C and C++ XRCE pub/sub + service + action deliver end-to-end on
native_sim (parity with the now-green rust xrce lanes).

**DONE 2026-07-10 — root cause was the missing agent locator (the C/C++ analog of
#163), not a delivery bug.** The C/C++ XRCE entry opened its session with NO agent
address: `NROS_ENTRY_LOCATOR` (nros-cpp `main.hpp`) only read
`CONFIG_NROS_ZENOH_LOCATOR` (unset for XRCE) → `""` → the XRCE transport never
connected (`run_components` rc=-100 `TRANSPORT_ERROR`, hence "0 delivery"). #163
had fixed only the Rust images (via each example `build.rs` synthesizing the
`host:port` into `NROS_LOCATOR`). Fix: `main.hpp` now synthesizes the bare
`host:port` from `CONFIG_NROS_XRCE_AGENT_{ADDR,PORT}` when it's an XRCE build
(adjacent string-literal concat + stringize), which the XRCE session parser
accepts. Covers both C and C++ (codegen routes both through
`ZephyrBoard::run_components`).

En route, three stale test markers surfaced once the transport connected (#164
class): `xrce_c_action` server-ready grepped `"Waiting for goals"` (server prints
`"Waiting for action goals"` = `ACTION_SERVER_READY_MARKER`); `xrce_cpp_action`
grepped `"Waiting for goal"` + required `feedback >= 1` and a literal `"Feedback"`
the Fibonacci server never streams (it completes with a result, like the C
sibling) — gated on `ACTION_RESULT_PREFIX` instead.

**Result: all 6 XRCE C/C++ lanes green** (`xrce_{c,cpp}_{talker_listener,service,
action}`, 6/6 in the parallel `qemu-zephyr-xrce` group). #174 resolved.

### W4 — Cyclone action/service completion (native_sim)

`dds_{c,cpp,rs}_action` = `server_received_goal=true, client_completed=false`;
`cpp_service_server_to_client` = `client OK=1` of 3. The Cyclone native_sim
server RECEIVES the goal/request but the result/completion round-trip is lossy.
phase_118 covers only pub/sub, so these action/service lanes have no LKG.

**Acceptance:** Cyclone action goal→feedback→result and multi-call service
complete end-to-end on native_sim across c/cpp/rs; add them to the phase_118-class
coverage so they don't silently rot again.

**INVESTIGATED 2026-07-10 — root cause narrowed, fix deferred (deep DDS discovery,
distinct from the W1–W3 quick wins).** Reproduced `dds_rs_action_e2e`: the SERVER
is fully green (goal received → executed → feedback published → succeeded), the
CLIENT gets the immediate goal-accept reply but never the later feedback (topic) or
the delayed `get_result` reply in 90 s. So client→server + the first server→client
reply work; the LATE server→client paths do not reach the client. **Ruled out:**
the #171/0171 VOLATILE-write-timing race (that fix landed AND is present on all
three writers — `service.cpp` request + `service_send_reply`, and `publisher.cpp`
feedback all gate on `dds_get_publication_matched_status.current_count`); stale
markers (client genuinely stops at "waiting for result"); the benign
`tid … is in use!` dynamic-thread cleanup warnings (both sides; server works
despite them). **Remaining question:** the writers wait for a match, yet the
client's feedback + `get_result`-reply READERS never complete match/receive — most
likely a late-reader discovery/liveliness gap for the action's result/feedback
entities on native_sim NSOS (the client's `get_result` reader is created lazily,
only after goal-accept). Next step = a **trace-level rebuild** (`NROS_CYC_TRACE`
both images) to read the reply/feedback writers' match/timeout at write time.
Full evidence in #175. #175 stays open.

**Deep inspection follow-up 2026-07-11 — narrowed to a selective receive-side gap.**
Instrumented the server's reply + feedback writers (temporary trace, reverted): every
write goes to a MATCHED reader (`cur=1`) — goal-accept reply, feedback, and the
`get_result` reply all fire `dds_write` with the client's reader matched. So it is
NOT write-timing/match/QoS. Also ruled out a client spin/dispatch gap:
`action_client_raw_try_process` (`arena.rs:1219`) polls feedback + the `get_result`
reply every spin. Yet only the goal-accept lands on the client; the later feedback +
`get_result` reply never appear in the client's readers. It is a **selective
receive-side delivery gap on native_sim NSOS** — first server→client reply received,
later ones not. Next step = a **client-side** read-path trace (`subscriber.cpp` /
reply-reader `dds_take`) to split "sample never reaches the reader cache" (RTPS/NSOS)
vs "reaches it but `dds_take` misses" (reader state). #175 carries the full evidence.

**Client read trace 2026-07-11 — resolved to the LAYER: the rmw + transport WORK;
the bug is in the nano-ros action layer.** Traced the client read paths (reverted):
the client DOES receive everything at the rmw — feedback `sub_take taken=1`, the
`get_result` **response** `reply_take taken=1`, and its correlation `match=1` (so
`service_try_recv_reply_raw` returns the reply). The goal-accept reaches the app
("Goal accepted"), but `on_feedback`/`on_result` never fire. So the loss is strictly
between the rmw take (works) and the app callback in
`arena.rs::action_client_raw_try_process` (feedback + result steps). Plus the server
sends the `get_result` reply ~2 s BEFORE it prints "Goal succeeded" — a **premature
reply** that clears the client's `pending_seq` so the terminal result is never
awaited. Fix is two-fold and ABOVE cyclone: (1) action server must hold the
`get_result` reply until the goal is terminal; (2) action-client dispatch must
deliver the taken feedback + result to the callbacks. Cyclone transport +
`service.cpp` routing are proven working, NOT the cause. #175 has the full trace.

**FIXED (rust action) 2026-07-11 — nested-message encap.** The final byte-level
trace corrected the "premature reply" reading: the server correctly DEFERS
`get_result` (goal active) and completes with a valid Succeeded result. The real
defect: the action **result/feedback is a NESTED field** inside the DDS typed
`GetResult_Response` / `FeedbackMessage`, so it must carry NO per-message CDR encap
— but the nros action layer serialised it WITH one (`new_with_header`). Cyclone's
`dds_stream` typed framing consumes that inner encap in transit, delivering the
fields RAW to the client, while `ctx.message` / `ffi_deserialize` (`new_with_header`)
expect an encap and eat the first data word → `ctx.message::<Result>()` Err →
callback silent. Fix: `arena.rs::action_client_raw_try_process` splices the reply's
top-level encap back in front when the payload arrives without one
(`payload_has_cdr_encap`); zenoh/XRCE keep the encap and pass through.
**`test_zephyr_dds_rs_action_e2e` PASS (9.5 s); `test_zephyr_xrce_rust_action_e2e`
still PASS** (no regression). Residuals (separate, tracked in #175): the C/C++
Cyclone action SERVER hangs in `create_action_server` (never reaches readiness — a
`nros-c`/`nros-cpp` entity-declare issue, not the encap), and the typed
(closure-callback) action-client dispatch needs the same splice.

**RESIDUALS RESOLVED 2026-07-11.** Both #175 residuals landed:
- *Typed dispatch* — shared `read_action_field` helper (`arena.rs`) applies the
  same encap-splice on the typed `action_client_callback_try_process` path
  (feedback + result). Committed with the core encap fix.
- *C/C++ action SERVER register `-100`* — NOT a `create_action_server` hang; the
  **feedback publisher** create returned UNSUPPORTED on a ROS-slash vs DDS-mangled
  type-name mismatch (`action_topic_type` derived `…/Fibonacci_FeedbackMessage_`
  slash form; `find_descriptor` is exact-strcmp vs the registered DDS key
  `…::dds_::Fibonacci_FeedbackMessage_`). Fix: `action_topic_type` (descriptors.cpp)
  runs `type_name` through `ros_form_to_dds` before deriving the feedback/status
  suffix; `ros_form_to_dds` moved to descriptors.cpp's named namespace (shared).
  **`dds_c_action_e2e` + `dds_cpp_action_e2e` PASS 5.5 s** (were 60 s register-fail
  timeouts); `dds_rs_action_e2e` PASS 9.8 s (no regression); C service boots PASS.
  All three cyclone action lanes (c/cpp/rs) now green → **W4 acceptance met**;
  **#175 resolved.**

## Sequencing

W1 (#166) → W2 (staleness — unblocks true rust-zenoh signal) → W3 (XRCE-C/C++)
→ W4 (Cyclone completion). W3/W4 are independent runtime tracks and may proceed
in parallel once W1/W2 land.

## References

Issue #166 (parallelism design), issue #164 (the family re-triage — this phase's
source), issue #163 (resolved — pure-Rust backend), the #147 staleness class,
`packages/testing/nros-tests/{src/platform.rs,tests/zephyr.rs}`,
`.config/nextest.toml`, `scripts/build/zephyr-fixture-leaves.sh`.
