# Phase 156 — Bridge Runtime Blockers (104.D.3 / .D.4 E2E gate)

**Goal.** Resolve the two runtime blockers that prevent
Phase 104.D.3 (`bridge_zenoh_to_dds_e2e`) + 104.D.4
(`bridge_xrce_to_dds_e2e`) from running fully green. Both
tests already exist + skip cleanly via `[SKIPPED]` when the
underlying bridge can't reach its `Spinning` marker; this
phase closes the actual session-open paths so the assertions
fire end-to-end instead.

**Status.** Open — investigation paused 2026-05-18 after
clearing the four shallower blockers (link-tcp feature
removal, zpico-sys POSIX shim include path, bridge
`ExecutorConfig::default` vs `from_env`, missing `std`
feature on `nros-rmw-zenoh`). The remaining two failures
both surface inside dual-RMW-backend binaries and need
focused debugging.

**Priority.** P2 — bridge plumbing already works structurally
(D.1 / .D.2 examples build clean, both `register` symbols
land in the final binary per `nm`); the failure is the
backend's session-open path, not the multi-RMW link itself.

**Depends on.** None blocking.

**Related.** Phase 104.D.3 + 104.D.4 (the E2E tests that
gate on these fixes), Phase 117 (Cyclone DDS — same dual-
session shape may surface there too), Phase 124.B (wake-cb
plumbing — shares some session-init code paths).

---

## Blocker #5 — `Executor::open_with_rmw("zenoh", ...)` returns `Transport(ConnectionFailed)` in dual-backend bridge binary

**Symptom.** `examples/bridges/native-rust-zenoh-to-dds/`
panics at `src/main.rs:60` with
`Transport(ConnectionFailed)` even though:

- `zenohd` is up on the same locator the bridge is configured
  for (verified `ss -lnt 'sport = :7451'` shows LISTEN).
- The single-backend `examples/native/rust/zenoh/talker/`
  binary using the same locator + `Executor::open` succeeds.
- Both backends' `_register` symbols are in the bridge
  binary (`nm` shows `nros_rmw_zenoh_register` +
  `nros_rmw_dds_register`).
- `Executor::open(&cfg)` (no name pin) also fails with
  `ConnectionFailed` — not a name-lookup miss.

**Investigation steps (suggested):**

1. **Confirm registry contents at runtime.** Dump the
   `nros_rmw_cffi_registered_names` list right before the
   `open_with_rmw` call (bridge crate doesn't directly
   depend on `nros-rmw-cffi`, so route this through a tiny
   helper export on the `nros` umbrella crate). Want to
   verify both `"zenoh"` + `"dds"` names are present, not
   one or zero.
2. **Bisect at link-feature layer.** Strip the bridge to
   only `nros-rmw-zenoh` (no DDS dep, no whole-archive
   wrap). If zenoh-only bridge works → DDS link is
   clobbering zenoh-pico state. If still fails → bug is
   in the umbrella's session-open path triggered by
   `Executor::open_with_rmw`.
3. **Check `--allow-multiple-definition` impact.** The
   per-target whole-archive wrap uses `-Wl,--allow-multiple-definition`
   to suppress platform-cffi symbol collisions. This can
   silently pick the wrong copy of a function. Inspect
   `nm xrce_to_dds_bridge | sort | uniq -c | sort -rn` for
   duplicated `nros_platform_*` symbols + verify the active
   copy is the one the running session-open expects.
4. **Symbol resolution under whole-archive.** Confirm
   `nros_rmw_zenoh_register` actually gets *called* — its
   `.init_array` ctor should fire before `main`. Set a
   breakpoint or add eprintln! in the ctor path
   (`packages/zpico/nros-rmw-zenoh/src/lib.rs` `_register`).
5. **Compare cargo metadata between talker + bridge.**
   Same `nros-rmw-zenoh` features? Same `nros` features?
   The bridge currently sets `nros = [std, rmw-cffi,
   platform-posix]` + `nros-rmw-zenoh = [std, platform-posix,
   ros-humble]` (matches talker). Any divergence in
   transitive feature resolution between the two crates?

**Hypotheses to falsify:**

- **H1 (link order):** DDS staticlib being whole-archived
  drags in a global static or platform-cffi symbol that
  zenoh-pico's session-init reads, getting the wrong copy.
  → Check via #3.
- **H2 (registry name miss):** Linkme registry is empty
  for `"zenoh"` because DDS's ctor ran first + somehow
  reset the slot. → Check via #1.
- **H3 (zenoh-pico transport singleton):** Phase 129 dropped
  `link-tcp` / `link-udp-unicast` features — vendor always
  compiles those transports + the locator picks at runtime.
  If the locator parser fails silently in dual-backend
  builds (compile-feature divergence?), zenoh-pico can't
  open a TCP socket. → Check via #2 (zenoh-only build).

---

## Blocker #6 — `nros_executor_node_init(rmw="xrce")` returns -1 against live agent in C bridge

**Symptom.**
`examples/native/c/bridge/xrce-to-dds/build/xrce_to_dds_bridge`
panics with `nros_executor_node_init(...) -> -1` for the
**ingress** (XRCE) node even when:

- `build/xrce-agent/MicroXRCEAgent udp4 -p <port>` is
  running on the supplied `NROS_XRCE_LOCATOR` (verified
  `ss` shows LISTEN, `MicroXRCEAgent` log shows agent up).
- `nros_support_init` + `nros_executor_init` already
  succeeded (so the primary session opened against the
  same locator without error).
- `nros_rmw_xrce_register` is in the bridge binary
  (`nm` confirms).
- The C bridge's options struct
  (`nros_node_options_t.rmw_name = "xrce"`) is built via
  the documented `nros_node_get_default_options()` +
  `memcpy` path (matches what `nros_executor_node_init`'s
  rustdoc shows).

**Investigation steps (suggested):**

1. **Determine the failing layer.** `nros_executor_node_init`
   wraps `Executor::node_builder(name).rmw(rmw_name).build()`
   on the Rust side. -1 is the generic
   `NROS_RET_ERROR`; trace from
   `packages/core/nros-c/src/executor.rs::nros_executor_node_init`
   down to find which step returns Err. Likely candidates:
   `extra_sessions.push` (capacity exceeded — unlikely),
   the actual `CffiRmw::open_with_rmw("xrce", ...)`
   call (matches the wake-up of agent ping handshake — see
   #2), or `node_id` allocation.
2. **XRCE dual-session vs singleton.** Micro-XRCE-DDS-Client
   uses one `uxrSession` + one `uxrUDPTransport` per process
   in its default build. When the primary
   `nros_support_init` opens session #1 and the bridge
   then asks `nros_executor_node_init(rmw="xrce")` for
   another XRCE session via the same agent, the client
   side may reject the second open (some XRCE configs
   refuse multiple sessions per agent address). Look at
   `packages/xrce/nros-rmw-xrce-cffi/src/lib.rs`'s open
   path + the underlying `uxr_create_*_session` calls
   for "already open" guards.
3. **Try same-session reuse.** If the bridge's ingress
   node could *reuse* the primary's XRCE session (because
   primary was opened with the XRCE locator too), the
   problem reduces to a session-cache miss in the Rust
   side. Inspect
   `Executor::create_node_with_rmw`'s session-cache key
   logic — `(rmw, locator, domain_id)` should hit the
   cache, but maybe the locator string normalisation
   doesn't match between `nros_support_init`'s call site
   and `nros_executor_node_init`'s.
4. **Check whether dds-egress opens before xrce-ingress.**
   If the bridge swaps node creation order (`node_out`
   first, then `node_in`), the XRCE-side failure mode
   might shift — which would point at session-cache or
   resource-ordering issues.

**Hypotheses to falsify:**

- **H4 (XRCE client singleton):** uxrSession allows only
  one session per process; the bridge's primary support
  init already grabbed it. → Check via #2 + check the
  uxr config in `nros-rmw-xrce-cffi/build.rs`.
- **H5 (session-cache key mismatch):** Rust side's
  `(rmw, locator, domain_id)` cache key doesn't match
  what `nros_support_init` registered, so `node_init`
  tries to open a fresh session + fails. → Check via #3.
- **H6 (NROS_XRCE_LOCATOR format quirk):** Bridge expects
  `udp/host:port` but XRCE backend's locator parser wants
  `udp:host:port` or a different shape. → Single-backend
  XRCE talker / listener works today, so this is
  unlikely, but worth a 30-second comparison.

---

## Work Items

### 156.B diagnostic — failure localised to zpico_open (2026-05-18 second probe)

Added `NROS_RMW_TRACE_OPEN` env-gated `eprintln!` at three
points along the open path:

  * `packages/core/nros-rmw-cffi/src/lib.rs:1558`
    (`open_with_vtable` — outer)
  * `packages/zpico/nros-rmw-zenoh/src/zpico.rs:404`
    (after `zpico_init_with_config`)
  * `packages/zpico/nros-rmw-zenoh/src/zpico.rs:417`
    (after `zpico_open`)

Trace output from the bridge run:

```
[zpico] init_with_config ret=0
[zpico] zpico_open ret=-3
[nros-rmw-cffi] open: locator="tcp/127.0.0.1:7451" mode=0 ret=-1 backend_data=0x0
```

`init_with_config` succeeds (config built correctly); the
failure is `zpico_open` returning `ZPICO_ERR_SESSION` (-3,
defined in `packages/zpico/zpico-sys/c/include/zpico.h`),
which is set when zenoh-pico's `z_open` returns negative
(see `packages/zpico/zpico-sys/c/zpico/zpico.c:880-883`).
The outer cffi layer correctly maps to
`Transport(ConnectionFailed)` and the bridge surfaces it.

**Crucial finding (falsifies bridge-specific hypothesis):**
the same trace appears when running the **single-backend
native talker** (`examples/native/rust/zenoh/talker/`)
against the same zenohd — both bridge AND talker fail at
`zpico_open` with -3. zenohd shows zero incoming TCP
accepts in either case. So the failure isn't dual-backend
related — it's a zenoh-pico session-open env / runtime
issue affecting every zenoh-pico consumer in this
sandbox.

Falsified hypotheses:
- **H1 (DDS clobber)** — zenoh-only bridge fails the same way.
- **H2 (registry name miss)** — diag shows registry lookup
  succeeded (vtable.open ran).
- **H3 (zenoh-pico transport singleton)** — broken in
  single-backend talker too.
- **H7 (multicast scout blocking)** — `ZENOH_MULTICAST_SCOUTING=false`
  doesn't unblock.

New hypotheses to test (parked):
- **H8 (zenoh-pico vs zenohd version mismatch despite
  matching version.txt):** Both report 1.7.2 but ABI / wire
  format may differ. Stock `zenohd` binary picked up from
  PATH — check if `build/zenohd/zenohd` is actually 1.7.2
  built from the project's pinned source vs a system
  install.
- **H9 (Z_FEATURE_MULTI_THREAD timing):** `zpico_open`
  sets `auto_start_read_task = false` /
  `auto_start_lease_task = false` (`zpico.c:876-879`).
  `z_open` may rely on those tasks for its own handshake
  completion → returns prematurely. Check whether
  zenoh-pico's `z_open` semantics require those tasks
  to be auto-started, or whether `zpico_open` should
  manually pump after.
- **H10 (build-time config drift):** my Phase 156 fix to
  `[platform.posix].include_paths` in
  `zenoh_platforms.toml` is data-only and shouldn't
  affect runtime. The `build_c_shim` include addition
  is the active code path for POSIX zpico.c builds.
  Confirm neither change altered zenoh-pico
  compile-time defines.

Next concrete probe (156.5):

1. Set `RUST_LOG=trace` + run `zenohd` with
   `--cfg 'transport/log_level:"trace"'` to see if any TCP
   handshake attempt reaches the router.
2. Bypass nros-rmw-zenoh entirely — call zenoh-pico's
   `z_open` directly from a 30-line C test program with
   the same config. If THAT works, the bug is in
   zpico.c's config / task wiring. If it fails too,
   zenoh-pico itself is the issue in this env.

### 156.A diagnostic log (2026-05-18 investigation pause)

Partial findings from the first investigation session:

- **Bisect #2 result:** Zenoh-only bridge (DDS dep removed)
  STILL panics with `Transport(ConnectionFailed)` —
  falsifies H1 (DDS clobber) + H3 (zenoh-pico transport
  singleton). Bug is in the bridge crate's session-open
  path itself, not the dual-backend link interaction.
- **`register()` call added:** Per Phase 128.B.1's note in
  `examples/native/rust/zenoh/talker/src/main.rs`, stable
  Rust requires an explicit symbol reference from the
  binary to a backend crate before the backend's
  `RMW_INIT_ENTRIES` linkme section gets pulled into the
  link line. Bridge now calls
  `nros_rmw_zenoh::register().expect(...)` +
  `nros_rmw_dds::register().expect(...)` before
  `Executor::open_with_rmw`. Neither `register()` panics
  — registration succeeds. ConnectionFailed still
  surfaces from `open_with_rmw`.
- **zenohd reachability verified:** `ss -lnt 'sport = :7451'`
  shows zenohd listening; bridge supplies matching
  `NROS_LOCATOR=tcp/127.0.0.1:7451`. zenohd's accept
  log shows zero incoming TCP attempts when the bridge
  runs — i.e., the bridge fails BEFORE making any
  network call, NOT during the TCP handshake. Suggests
  the failure is upstream of zenoh-pico's transport
  layer (vtable.open returning a non-OK ret code
  immediately, or a parse/validation failure on the
  locator / mode struct).
- **`open_with_vtable` path (`packages/core/nros-rmw-cffi/src/lib.rs:1534`)**
  either returns `error_from_ret(non_ok_ret)` or
  explicitly `ConnectionFailed` when
  `view.backend_data.is_null()` post-open. Next probe:
  log the exact `ret` value out of `(vtable.open)(...)`
  + whether `backend_data` is null.

Investigation suspended after these four data points;
156.4 + 156.5 work items below carry the next steps.

- [ ] **156.1 — Add registry-name + cargo-feature debug
      dump.** One-shot diagnostic on the bridge crate that
      prints `nros_rmw_cffi_registered_names` + each
      backend's resolved Cargo feature set before the
      first `Executor::open`. Lands behind a
      `debug-registry` Cargo feature so production
      bridge binaries stay clean. **Files:**
      `examples/bridges/native-rust-zenoh-to-dds/src/main.rs`,
      `examples/bridges/native-rust-zenoh-to-dds/Cargo.toml`,
      `packages/core/nros/src/lib.rs` (if a small
      re-export of `nros_rmw_cffi_registered_names`
      from the umbrella is cleanest).
- [ ] **156.2 — Bisect blocker #5 by stripping DDS.**
      Build zenoh-only bridge (no `nros-rmw-dds` dep).
      If it works, the DDS staticlib is clobbering
      zenoh-pico's session state. If it fails, the bug
      is in the umbrella's `open_with_rmw` path or in
      the zenoh-pico shim's session-init when reached
      from a multi-backend binary. **Files:** scratch
      branch, no commit needed.
- [ ] **156.3 — Audit `nm` for duplicated symbols.**
      `nm zenoh-to-dds | sort | uniq -d | head`. Each
      duplicate is a candidate for the
      `--allow-multiple-definition` "wrong copy"
      hypothesis. Cross-reference with bare-metal
      single-backend binaries to confirm which symbols
      are expected to be unique. **Files:** none.
- [ ] **156.4 — Trace XRCE dual-session open.** Add
      `eprintln!` instrumentation in
      `packages/xrce/nros-rmw-xrce-cffi/src/lib.rs`'s
      open path to log every step from
      `nros_rmw_cffi_lookup("xrce")` through the
      underlying `uxr_create_session` call. Run the
      bridge with the local agent; capture the failing
      step. **Files:** `packages/xrce/nros-rmw-xrce-cffi/src/lib.rs`
      (temporary instrumentation, revert after).
- [ ] **156.5 — Session-cache key audit.** Read
      `Executor::node_builder.build` + the underlying
      session-cache code (Phase 104.C.2). Confirm
      `(rmw, locator, domain_id)` key normalisation
      matches between `support_init`-opened primary +
      `node_init`-opened extras. **Files:**
      `packages/core/nros-node/src/executor/spin.rs`,
      `packages/core/nros-node/src/node.rs`.

---

## Acceptance

- [ ] `cargo nextest run -p nros-tests --test
      bridge_zenoh_to_dds_e2e` runs to completion (not
      `[SKIPPED]`) with all four init markers
      (primary-zenoh-open, ingress/egress session_idx,
      raw publisher/subscriber) asserted green.
- [ ] `cargo nextest run -p nros-tests --test
      bridge_xrce_to_dds_e2e` runs to completion (not
      `[SKIPPED]`) with all four init markers
      (XRCE ingress, DDS egress, raw publisher/subscription)
      asserted green.
- [ ] No new symbol-collision warnings in the bridge
      binary's link line; the
      `-Wl,--allow-multiple-definition` whole-archive
      workaround is documented (or replaced).

## Notes

- Both blockers surface ONLY in dual-RMW-backend binaries.
  Single-backend builds (talker, listener) work fine.
- The 4 shallower blockers fixed during the 104.D.3
  investigation (commits `246bbf8b` link-tcp,
  `1f9ce6dd` zpico.c include) landed on the bridge
  branch + improve other consumers too — those fixes
  stay merged regardless of how 156 resolves.
- Phase 104.D.3 / 104.D.4 keep their `[x]` checkbox in the
  104 doc because the *tests* are correct + green-on-skip;
  the bridges' runtime correctness is THIS phase's scope.
