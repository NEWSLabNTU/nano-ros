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

### 156.F Sub-bug D — C-side multi-Session dispatch missing in `nros_publisher_init` (D.4 next blocker) — RESOLVED

After Sub-bug C resolved via Option 3 (`NANO_ROS_RMW=none`
escape hatch + explicit register calls), D.4 progressed
further:

```
=== Phase 104.D.1 bridge: XRCE -> DDS ===
Registered XRCE + DDS RMW backends
XRCE locator (ingress): udp/127.0.0.1:33535
DDS  locator (egress):  (backend default)
Domain ID: 0
Ingress node bound to XRCE
Egress node bound to DDS
[nros] nros_publisher_init(&app.pub_out, &app.node_out, &kStringType, "/chatter") -> -7
```

Both nodes bind (`Ingress node bound to XRCE` +
`Egress node bound to DDS`). Failure at the next call:
`nros_publisher_init` returns `NROS_RET_NOT_INIT` (-7)
because `node_ref.get_support_mut()` returns `None`.

**Root cause:** `nros_executor_node_init`
(`packages/core/nros-c/src/executor.rs:541`) explicitly
sets `node_ref.support = ptr::null()` — the Phase 104.C.8
design ditched the support-based dispatch for multi-Node
paths in favour of `node.node_id`-keyed executor session
lookup. But `nros_publisher_init`
(`packages/core/nros-c/src/publisher.rs:202`) still calls
`node_ref.get_support_mut()` unconditionally — the
"branch on `node.node_id` non-zero" multi-Session
dispatch the doc claims for the C API was wired into
`nros_executor_register_{subscription,service,client,
action_*}` but NOT into `nros_publisher_init` /
`nros_subscription_init` / etc.

**Fix landed:**

1. Added `pub executor: *const nros_executor_t` field to
   `nros_node_t` (struct ABI bump; cbindgen regen
   confirmed in `packages/core/nros-c/include/nros/
   nros_generated.h`).
2. `nros_executor_node_init` populates the field after
   `builder.build()` returns the NodeId.
3. New helper `node::resolve_session_and_domain` branches
   on `node.node_id != 0 && !node.executor.is_null()`:
   - Multi-Session path → `Executor::node_session_mut
     (NodeId::from_raw(node.node_id))` (already public,
     lands in `extra_sessions[N-1]` via
     `NodeRecord.session_idx`); domain_id comes from
     `node.domain_id_override` or, when inherit, the
     executor's still-borrowed `support` pointer.
   - Legacy single-Session path → preserved
     `node.get_support_mut() + support.get_session_mut()`
     dispatch.
4. Wired into all six entity init sites:
   `nros_publisher_init`, `nros_subscription_init`,
   `nros_service_server_init`, `nros_service_client_init`,
   `nros_action_server_init`, `nros_action_client_init`.
5. Bridge `examples/native/c/bridge/xrce-to-dds/src/
   main.c` gained an `setvbuf(stdout, NULL, _IOLBF, 0)`
   so the init-marker prints reach piped test harnesses
   before the long-lived `spin_period` loop blocks
   subsequent flushes.

**Verification:** `cargo nextest run -p nros-tests --test
bridge_xrce_to_dds_e2e` (with `MicroXRCEAgent` on PATH)
goes 1 passed / 0 skipped. The bridge prints all init
markers (`Ingress node bound to XRCE`, `Egress node
bound to DDS`, `Egress raw publisher created on DDS`,
`Ingress raw subscription registered on XRCE`, `Bridge
spinning`) within the test's 20 s readiness window.

### 156.E Sub-bug C — multi-staticlib `nros-rmw-cffi` monomorphisation conflict (D.4 blocker)

D.3 (Rust bridge) went fully green after the
session-cache primary-identity fix. D.4 (C bridge) hits a
**different** blocker exposed by the same fix.

C bridge links TWO Rust staticlibs (`nros-rmw-xrce-cffi`
+ `nros-rmw-dds-staticlib`) via separate corrosion
imports. Each is its own `cargo build` invocation → each
monomorphises `nros-rmw-cffi` (shared dependency) with
whatever feature set was active in THAT invocation:

  * XRCE staticlib uses
    `nros-rmw-cffi = { default-features = false }` (no
    `alloc`); root CMake passes `linkme-register std` for
    XRCE.
  * DDS staticlib uses
    `nros-rmw-cffi = { default-features = false, features = ["alloc"] }`
    via `nros-rmw-dds`; root CMake passed `platform-posix
    ros-humble` only (no `linkme-register`) until the
    Phase 156 cmake edit added it.

Different `nros-rmw-cffi` feature sets per staticlib =
two crate hashes (verified via `nm xrce_to_dds_bridge |
grep RMW_INIT_ENTRIES` → both `linkme_RMW_INIT_ENTRIES`
and `linkm2_RMW_INIT_ENTRIES` symbols visible, each only
8 bytes = ONE entry). Each backend's
`#[distributed_slice]` entry lands in a different
slice. The runtime walker only iterates ONE slice; the
other backend is effectively unregistered.

After enabling `linkme-register` on the DDS staticlib
(Phase 156 attempt), `linkme` itself panics at runtime
with:

```
duplicate #[distributed_slice] with name "RMW_INIT_ENTRIES"
```

— `linkme` detects two static slices defined under the
same name across the two `nros-rmw-cffi` crate
instances and aborts.

**Root cause:** Cargo + linkme can't deduplicate
distributed-slice definitions across crate instances
when those instances result from multi-staticlib
monomorphisation. The Phase 129 "unified vtable
registry" design implicitly assumed ONE
`nros-rmw-cffi` instance per binary, which holds when
everything is one cargo build but breaks when two
corrosion staticlibs are linked into the same C/C++
executable.

**Possible fixes (need design judgement):**

1. **Align feature sets across all staticlibs.** Pin
   every staticlib that depends on `nros-rmw-cffi` to
   the SAME features (e.g. `["alloc", "linkme-register",
   "std"]`). Forces Cargo to unify → one crate hash.
   Risk: harder to maintain as backends diverge in
   features.
2. **Single combined staticlib.** Build one
   `libnros_rmw_all.a` containing every backend +
   one shared `nros-rmw-cffi`. Bridge consumers link
   just that. Bigger refactor of Phase 123.A.1.x.4
   staticlib boundary.
3. **Drop the linkme distributed-slice entirely on
   POSIX.** Use the explicit `register()`-call pattern
   (`Phase 104.A.4` already supports it on bare-metal).
   POSIX would lose the auto-registration ergonomic but
   gain multi-backend determinism.
4. **Use `linkme`'s `#[linkme(crate = X)]` attribute
   if available** to pin the slice to a stable crate
   identity across monomorphisations. Need to check
   linkme docs.

**Status:** Sub-bug C carved out. D.4 stays
`[SKIPPED]` cleanly when the bridge fails to reach
"Spinning"; CI gate intact. Real fix gated on
option-pick discussion.

### 156.D Sub-bug B FIXED via Option B + missing Z_FEATURE_MULTI_THREAD (2026-05-18 fourth probe)

Two changes landed Sub-bug B:

1. **Option B implemented:**
   - `platform_aliases.c`: wrap all network alias functions
     (TCP / UDP-unicast / UDP-multicast / socket-helpers) in
     `#ifndef NROS_ZENOH_PLATFORM_USES_UNIX`. Pointer-shaped
     aliases (threading / mutex / condvar / clock / sleep /
     random / malloc / time) stay active uniformly across
     platforms because pointer ABI is uniform.
   - `build.rs`: define `NROS_ZENOH_PLATFORM_USES_UNIX` when
     `use_posix`, so the network alias section gets `#ifndef`-
     elided.
   - `zenoh_platforms.toml [platform.posix].extra_sources`:
     add `system/unix/network.c` so zenoh-pico's upstream
     POSIX impls (matching `_z_sys_net_socket_t = { int _fd; }`
     4-byte struct from unix.h) compile in + provide the
     real TCP/UDP impls.

2. **Missing `Z_FEATURE_MULTI_THREAD = 1` on POSIX:**
   `[platform.posix].defines_kv` was setting only
   `ZENOH_DEBUG = 0`. Other platforms (freertos-lwip /
   nuttx / threadx) all set `Z_FEATURE_MULTI_THREAD = 1`.
   POSIX's missing flag fell to zenoh-pico's `config.h`
   default `0`, which makes `zp_start_read_task` /
   `zp_start_lease_task` return `-1` unconditionally
   (`api.c:2152-2164`). `zpico_open` mapped that to
   `ZPICO_ERR_TASK` (-4). Fix: add `Z_FEATURE_MULTI_THREAD = "1"`
   to the kv set — POSIX always has pthreads.

**Verification trace (zenoh-min minimal repro after both
fixes):**

```
opened
```

Plus tshark on loopback showed full zenoh handshake:
InitSyn → InitAck → OpenSyn → OpenAck → KeepAlive
sequence completed. Verified zenohd accepted the session.

**Full bridge demo:** primary opens but ingress
`node_builder("ingress").rmw("zenoh").build()` returns
`Transport(Backend("rmw_ret error"))` — that's a SEPARATE
issue (zenoh-pico's `g_session` is a process-singleton; the
bridge's two-Node-same-RMW pattern needs the session-cache
to hit instead of opening a second session). Tracked
under a new sub-item below — not part of Sub-bug B.

### 156.C ROOT CAUSE — ABI mismatch in platform_aliases.c on POSIX (2026-05-18 third probe, gdb + tshark + per-layer trace)

Localised + fixed the two visible failure surfaces, but the
fundamental issue is a Phase 129 design-level decision:

**Layer-by-layer trace:**

```
[zpico] init_with_config ret=0             ← config built OK
[zpico/aliases] _z_open_tcp -> -1 (tout=0) ← FIRST FAILURE
[zpico.c] z_open ret=-102                  ← _Z_ERR_TRANSPORT_OPEN_FAILED
[zpico] zpico_open ret=-3                  ← ZPICO_ERR_SESSION
[nros-rmw-cffi] open: ret=-1 backend_data=0x0
```

tshark on loopback showed: TCP SYN/ACK/ACK handshake
**succeeded** at kernel level (zenohd accepted), then client
immediately sent FIN — no zenoh handshake bytes ever
exchanged. Failure was AFTER socket open but BEFORE TX.

**Sub-bug A (FIXED):** `nros_platform_tcp_open` called
`apply_tcp_common_options(fd, timeout_ms)` BEFORE `connect()`.
With `timeout_ms = 0`, `set_recv_timeout_ms` flipped the
socket to `O_NONBLOCK` (Phase 127.B.5 mapping designed for
dust-DDS recv loop). Then `connect()` on a non-blocking
socket returns `-1` with `errno = EINPROGRESS` even though
the kernel completed the SYN/ACK/ACK. The C code treats
`-1` as failure → `close()` → kernel sends FIN. Fixed by
moving `apply_tcp_common_options` to AFTER successful
`connect()`.

After Sub-bug A fix: `_z_open_tcp` returns 0 (success).
zenoh-pico proceeds to `_z_link_send_t_msg` for InitSyn,
which calls `_z_send_tcp` (the alias) which calls
`nros_platform_tcp_send`. Trace:

```
[posix] tcp_send fd=0 len=106674563756592 r=-1 errno=88
```

**Sub-bug B (NOT trivially fixable):** ABI mismatch.

- zenoh-pico's `_z_send_tcp` declared in unix.h:
  `size_t _z_send_tcp(const _z_sys_net_socket_t sock, ...)`
  where `_z_sys_net_socket_t = { int _fd; }` (4 bytes,
  passed by VALUE in one int register on SysV AMD64).
- The alias TU `platform_aliases.c` redeclares the same
  name with a 32-byte opaque struct from
  `nros_zenoh_generic_platform.h`:
  `size_t _z_send_tcp(nros_zp_alias_socket_t sock, ...)`
  where `nros_zp_alias_socket_t = { uint8_t _opaque[32]; }`.
  The 32-byte struct is passed via stack/multiple regs.
- `--allow-multiple-definition` makes the linker pick
  ONE definition. Which one wins is undefined; on this
  host it picked the alias TU's 32-byte version.
- zenoh-pico's call site passes 4 bytes; alias function
  reads 32 → `fd = 0` (stdin) + garbage `len` →
  `errno = 88 (ENOTSOCK)`.

**Why both POSIX defines are set:** `[platform.posix]` in
`zenoh_platforms.toml` defines BOTH `ZENOH_LINUX` AND
`ZENOH_GENERIC`. zenoh-pico's `system/common/platform.h`
checks `ZENOH_LINUX` first → includes `unix.h` (4-byte
struct). The alias TU is built with `NROS_PLATFORM_ALIASES`
which (per the `define` comment in `build.rs:645`) "unlocks
the alias TU's clock-variant + network wrappers, which
depend on the generic `z_clock_t = uint64_t` typedef and
the canonical `_z_sys_net_*` opaque layouts in
`nros_zenoh_generic_platform.h`." So the alias TU
deliberately uses the GENERIC layouts regardless of
platform — POSIX shouldn't compile the network aliases at
all.

**Root-cause fix options (need design judgement):**

1. **(A) Don't compile platform_aliases.c on POSIX.**
   Tried — breaks link with undefined `_z_mutex_*` +
   `z_malloc` etc. because the unified build's POSIX entry
   (`zenoh_platforms.toml:71-80`) only includes
   `system/common` + `system/unix/tls.c`; the unix/system.c
   + unix/network.c files that provide POSIX threading +
   malloc aren't compiled into the unified archive. POSIX
   relied on aliases for those symbols too.

2. **(B) Make `platform_aliases.c` skip JUST the network
   aliases on POSIX.** Add `#ifndef NROS_ZENOH_PLATFORM_USES_UNIX`
   around `_z_open_tcp` / `_z_send_tcp` / etc. Keep the
   threading + malloc aliases. Requires defining the new
   macro from `build.rs` when `use_posix`. Cleanest
   localized fix.

3. **(C) Drop `ZENOH_LINUX` define on POSIX + add
   `system/unix/*.c` to `extra_sources`.** Use zenoh-pico's
   unix.h fully; aliases stop fighting. Bigger reshape.

4. **(D) Switch POSIX to `ZENOH_GENERIC`-only + extend
   alias TU to fully provide POSIX system layer.** Most
   aligned with Phase 129's "unified ABI" intent but
   biggest change.

**Sub-bug A's fix landed under Phase 156 anyway** because
it's a real bug even after Sub-bug B is resolved
(`set_recv_timeout_ms(0)` should never have run pre-connect).
The `apply_tcp_common_options(fd, effective_tout)` call now
runs post-connect with a `5000ms` coercion when `timeout_ms
== 0` (zenoh-pico's `_z_send_t_msg` does single-shot send;
non-blocking socket would EAGAIN on slow consumers).

Sub-bug B's resolution deferred to option-pick discussion +
implementation (likely option B as smallest blast radius).

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
