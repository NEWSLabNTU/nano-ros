# Zephyr 4.4 baseline build (Phase 180.A Task 3)

**Purpose.** Empirically build the simplest example (`examples/zephyr/c/talker`,
zenoh, `native_sim/native/64`, host toolchain) on the freshly-stood-up
Zephyr 4.4 workspace to surface the real failure set that drives Tasks 4–9.
Companion to the static `zephyr-3.7-to-4.4-divergence-audit.md`.

**Date.** 2026-05-25. **Branch.** `phase-180a-version-spanning-module`.
**Workspace.** `../nano-ros-workspace-4.4` (Zephyr 4.4.0).

## How far the build gets

After clearing the env prerequisites below, configure reaches the Kconfig
stage and stops at the first symbol divergence:

```
-- Found Python3: .../.venv312/bin/python (3.12.12)
-- Zephyr version: 4.4.0
-- Found toolchain: host (gcc/ld)
-- Found BOARD.dts: .../native_sim/native_sim_64.dts   (devicetree OK)
prj-zenoh.conf:7: warning: attempt to assign the value '16' to the undefined symbol MAX_PTHREAD_COUNT
error: Aborting due to Kconfig warnings
```

So on 4.4 the module gets through toolchain selection, devicetree, and into
Kconfig before the first real break. Toolchain (`host` = host gcc) and
devicetree are fine.

## Environment prerequisites (4.4 line)

These are hard prereqs — not code issues — and gate every 4.4 build:

1. **Python ≥ 3.12.** Zephyr 4.4's `find_package(Python3)` requires 3.12
   (host has 3.10.12 → configure aborts before Kconfig). Provisioned **without
   sudo** via the uv-managed 3.12 already on this box:
   ```bash
   uv venv --python 3.12 <ws>/.venv312
   uv pip install --python <ws>/.venv312/bin/python west pyelftools -r zephyr/scripts/requirements.txt
   <ws>/.venv312/bin/python -m west build ...      # run west THROUGH the venv
   ```
   Gotchas hit: (a) `uv venv` creates **no pip** — use `uv pip install` to
   target the venv, not `pip`; (b) bare `west` resolves to the 3.10
   `~/.local/bin/west` and hands cmake `WEST_PYTHON=/usr/bin/python3` — must
   invoke `python -m west` through the venv interpreter so `WEST_PYTHON` is
   3.12.
2. **dtc** (devicetree compiler) not found — emitted as a warning only;
   native_sim proceeded. Install for cleaner builds, non-blocking.

**Feedback to the plan:** Task 2's `just zephyr setup NROS_ZEPHYR_VERSION=4.4`
is incomplete — it does not provision Python 3.12, so it would fail here.
Add a Python-3.12 provisioning step (uv venv) to the 4.4 setup path. New
work item: **180.A Task 2b — Python 3.12 provisioning for the 4.4 line.**

## First Kconfig divergence (resolves audit Task 8)

The audit flagged the POSIX/pthread Kconfig as High-risk "needs a live tree."
Resolved here against 4.4 `lib/posix`:

| Overlay symbol | 4.4 status |
| --- | --- |
| `CONFIG_POSIX_API` | present (`lib/posix/Kconfig.profile`) |
| `CONFIG_MAX_PTHREAD_MUTEX_COUNT` | present |
| `CONFIG_MAX_PTHREAD_COND_COUNT` | present |
| `CONFIG_POSIX_THREAD_THREADS_MAX` | present |
| **`CONFIG_MAX_PTHREAD_COUNT`** | **REMOVED** |

Only one symbol is gone. 4.4 has `MAX_PTHREAD_{BARRIER,COND,MUTEX,RWLOCK,
SPINLOCK}_COUNT` but no bare `MAX_PTHREAD_COUNT`; the live thread-count cap is
now `CONFIG_POSIX_THREAD_THREADS_MAX` (already set to 16 in our overlays). So
**the fix is to drop `CONFIG_MAX_PTHREAD_COUNT` on the 4.4 line** — the count
is already covered. This is a one-symbol removal, not the broad rename the
audit feared; **Task 8 collapses to nearly nothing.**

Because `prj-*.conf` overlays are shared by both Zephyr lines and
`MAX_PTHREAD_COUNT` still exists on 3.7, the removal must be **version-aware**
(a 4.4 overlay/snippet, not an edit to the shared file) — that mechanism is
Task 8 / Phase 180.C, so the fix is deferred to there rather than applied in
this baseline.

## Failure set status

- **Resolved/characterized:** env prereqs (Python 3.12, dtc); first Kconfig
  break (`MAX_PTHREAD_COUNT` removed).
- **Not yet observable:** deeper Kconfig / header / API / NSOS failures are
  gated behind the `MAX_PTHREAD_COUNT` fix; they surface on the next rebuild
  once a version-aware 4.4 overlay drops that symbol (Task 8). The baseline is
  iterative by nature — each fix reveals the next layer.

## Conclusion

The module reaches Kconfig on 4.4 after env provisioning; toolchain +
devicetree are clean. The audit's two High-risk unknowns are now bounded:
POSIX Kconfig = a single removed symbol (this doc); Rust module = still
pending its own live-tree check (Task 9). The 4.4 line needs a Python-3.12
provisioning step folded into setup (Task 2b).

## Update — Task 8 applied (2026-05-25)

`CONFIG_MAX_PTHREAD_COUNT` removed/migrated across 42 non-vendored confs
(deleted where `POSIX_THREAD_THREADS_MAX` was already set — redundant since
3.7's deprecated `MAX_PTHREAD_COUNT` defaults to it; migrated to
`POSIX_THREAD_THREADS_MAX` in the one conf lacking it). Behavior-preserving
on 3.7, valid on 4.4. Vendored zenoh-pico doc conf left untouched.

Rebuild result: **Kconfig now completes on 4.4** (autoconf.h generated), and
the build advances through host gcc/ld/asm detection to the **next blocker —
`nros-codegen not found`** (`zephyr/cmake/nros_generate_interfaces.cmake:95`).
That is a generic host-tool prerequisite common to *both* Zephyr lines (the
`build-fixtures` recipe builds the host codegen tool and passes
`_NANO_ROS_CODEGEN_TOOL`), **not** a 4.4 divergence — so it belongs to the
build-orchestration/version-gating work (Tasks 4–10), not the POSIX fix.

No further 4.4-specific Kconfig divergence observed up to the codegen gate.

## Update — orchestration: first green 4.4 build (2026-05-25)

With the host `nros-codegen` built and passed via
`-D_NANO_ROS_CODEGEN_TOOL`, **c/talker zenoh builds end-to-end on Zephyr
4.4** (`native_sim/native/64`, host toolchain): 1303 ninja steps through
nros cargo (Corrosion) + zenoh-pico + std_msgs C codegen → `zephyr.elf`
(27 MB) + `zephyr.exe`. It also **boots and runs**: `*** Booting Zephyr OS
build v4.4.0 ***`, nros C talker initializes and reaches the network-wait
(fails to connect only because no zenohd/NSOS overlay in the smoke run —
expected). 3 non-fatal warnings. The zenoh native_sim line needed **zero
NSOS patches** (TCP transport).

**Tasks 4 + 7 resolved by this build:**
- **Task 4** (drop the upstreamed getsockname patch): confirmed — `getsockname`
  is upstream in 4.4 and zenoh built/booted without the patch.
- **Task 7** (socket/native_sim Kconfig + header renames): **no-op** — none of
  `CONFIG_NET_SOCKETS_POLL_MAX`, `zephyr/net/buf.h`,
  `CONFIG_NATIVE_SIM_NATIVE_POSIX_COMPAT`, `CONFIG_NATIVE_APPLICATION` are used
  anywhere in nano-ros examples / module / in-tree (0 files); the clean build
  corroborates. Nothing to rename.

**Tasks 5–6 (recvmsg reshape, IP-multicast + getifaddrs re-anchor): deferred,
runtime-only.** They do not block compile/link (verified — zenoh builds, and
the symbols are runtime mid-handling, not link deps). They matter only for the
**cyclonedds** native_sim path (RTPS multicast discovery via NSOS). Verifying
them requires the full cyclonedds-on-4.4 bring-up (cyclonedds submodule patches,
host idlc, cmake glue) plus a 2-node runtime discovery test — a separate effort
tracked alongside the cyclonedds-4.4 work, not the zenoh baseline.

## Update — orchestration polish: `just zephyr build-one` (2026-05-25)

Added a focused `build-one <example> <rmw>` recipe (`just/zephyr.just`) that
reproduces the orchestration first-class on either line: it resolves the
versioned workspace, (4.4) prepends the Python 3.12 venv to PATH + selects the
host toolchain, builds host `nros-codegen`, and `west build`s the example with
`-D_NANO_ROS_CODEGEN_TOOL`. **Verified on 4.4:**
`NROS_ZEPHYR_VERSION=4.4 just zephyr build-one c/talker zenoh` →
`zephyr.elf`. The 3.7 path uses the established ambient toolchain (same,
simpler branch — not re-run here). The whole `build-fixtures` matrix is *not*
version-gated yet (it inlines 3.7 patches + builds C++/cyclonedds that aren't
4.4-ready); `build-one` is the reproducible 4.4 entry point meanwhile.

### First version-aware-overlay case: ETH_NATIVE_POSIX → ETH_NATIVE_TAP

Adding the NSOS board overlay (`boards/native_sim_native_64.conf`) to the 4.4
build surfaced a genuine version-divergent symbol: the overlay sets
`CONFIG_ETH_NATIVE_POSIX=n` (disable the native eth driver so NSOS offload is
used), but 4.4 renamed it `CONFIG_ETH_NATIVE_TAP` (driver `eth_native_posix`
→ `eth_native_tap`). Unlike `MAX_PTHREAD_COUNT` (deprecated/redundant →
deletable), this `=n` is **meaningful on both lines**, so it cannot just be
dropped — it needs a per-line value in a SHARED overlay. **This is the first
concrete case that requires the version-aware overlay mechanism** Task 8
deferred (a 4.x snippet / per-line conf selection — Phase 180.C). **RESOLVED (version-aware overlay mechanism, 2026-05-25).** Added per-line
native_sim overlays `cmake/zephyr/native-sim-line-{3.7,4.4}.conf` (identical
NSOS settings; only the eth-disable symbol differs —
`CONFIG_ETH_NATIVE_POSIX=n` vs `CONFIG_ETH_NATIVE_TAP=n`). `build-one` selects
the file by `NROS_ZEPHYR_VERSION` and appends it to `CONF_FILE` for native_sim
boards, superseding the legacy per-example `boards/native_sim_*.conf`.
**Verified:** `NROS_ZEPHYR_VERSION=4.4 just zephyr build-one c/talker zenoh`
builds clean and boots with **`Network ready (NSOS — host kernel sockets)`**
— NSOS is now active on 4.4 (previously the tap driver failed on
`eth_tap: Cannot create zeth`). This is the reusable hook for any future
per-line config divergence (extend the two line confs); the broader
snippet-based form remains Phase 180.C. The legacy per-example board overlays
are untouched, awaiting the not-yet-version-gated `build-fixtures` path.

## E2E proof — 4.4 zenoh pub/sub over NSOS (2026-05-25)

Ran c/talker → c/listener on Zephyr 4.4 (`native_sim/native/64`, NSOS host
loopback) through `build/zenohd/zenohd` on `tcp/127.0.0.1:7456` (the default
zephyr locator). Subscriber-first, 6 s stabilization. **Result: PASS** —
talker published 0..15, listener received 0..15 (16/16). Confirms the 4.4
zenoh native_sim line does real pub/sub at runtime, not just build/boot:
the full chain (version selector → Python 3.12 → POSIX Kconfig migration →
NSOS line overlay → nros cargo + zenoh-pico) holds together end-to-end.

Run via `tmp/zephyr-44-e2e.sh` (ad-hoc). Promoting it to a proper
nextest/`nros-tests` case for both Zephyr lines is part of Task 10 (dual CI).

## Update — build-fixtures version-gated (2026-05-26)

`just zephyr build-fixtures` is now version-aware. On the 4.4 line it: runs
west via the 3.12 venv + host toolchain, skips the 3.7-only patch set, uses
the version-aware NSOS overlay (`cmake/zephyr/native-sim-line-4.4.conf`), and
restricts the matrix to the proven **zenoh native_sim** subset (xrce +
cyclonedds gated off pending Tasks 5–6 / cyclonedds-on-4.4). 3.7 is unchanged.
Verified: `NROS_ZEPHYR_VERSION=4.4 NROS_ZEPHYR_FIXTURE_FILTER='build-c-(talker|listener)-zenoh'
just zephyr build-fixtures` builds both ELFs via the full recipe path
("Zephyr test fixtures built successfully"). The 4.4 matrix expands to rust/cpp
+ other RMWs as they become 4.4-ready.

## cyclonedds-on-4.4 — progress + first blocker (2026-05-26)

Prep done: host `idlc` built (`build/install/bin/idlc`); dropped the inline
`CONFIG_ETH_NATIVE_POSIX=n` from all 18 `prj-cyclonedds.conf` (the eth-disable
now comes from the appended version-aware NSOS line overlay, same as zenoh —
3.7-safe because build-fixtures/build-one always append an NSOS overlay).

`build-one c/talker cyclonedds` on 4.4 then gets **far** — past configure,
957/1346 build steps (cyclonedds core + nros-c cargo) — and stops at the
**first real 4.4 cyclonedds blocker:**

```
zephyr/include/zephyr/kernel.h:6248: fatal error: zephyr/heap_constants.h: No such file or directory
  (via the force-included zephyr_ipv4_compat.h -> <zephyr/net/socket.h> -> kernel.h)
```

`heap_constants.h` is **4.x-new** (3.7 kernel.h doesn't include it) and is a
build-time-generated header (`heap_constants` cmake target). The cyclonedds
sub-build's TUs lack the dependency/generated-include path for it on 4.4 —
a cyclonedds-Zephyr-4.4 build-integration gap (the cyclonedds module needs
Zephyr's generated-include dir + ordering against the `heap_constants` target).
Likely the first of several integration blockers; cyclonedds-on-4.4 is a
focused multi-iteration effort. Tasks 5–6 (NSOS recvmsg/IP-multicast) are
runtime-after-build, still pending behind this.

## cyclonedds-on-4.4 — BUILDS + cyclone init works (2026-05-26)

`build-one c/talker cyclonedds` on 4.4 now builds to `zephyr.elf` (39 MB)
after fixing 4 blockers (all version-safe; 3.7 unaffected):

1. **heap_constants force-include** (`zephyr/CMakeLists.txt`): the cyclonedds
   `-include zephyr_ipv4_compat.h` was a GLOBAL `zephyr_compile_options`, so it
   hit Zephyr's own `heap_constants.c` bootstrap TU, whose 4.x kernel.h pulls
   the not-yet-generated `<zephyr/heap_constants.h>`. Scoped to the `nros`
   library on Zephyr ≥4.0 (global kept on 3.7).
2. **net_ip_mreq redefinition** (`zephyr_ipv4_compat.h`): 4.x net_compat.h does
   `#define ip_mreq net_ip_mreq`, so our `struct ip_mreq` macro-expanded to a
   redefinition. Guarded with `!defined(ip_mreq)` (version-agnostic feature
   detect).
3. **venv shadows ROS codegen python** (`just/zephyr.just` build-one): the 4.4
   PATH-prepend shadowed `python3`, breaking the cyclonedds descriptor codegen
   (ROS `msg2idl` needs catkin_pkg/rosidl_adapter from system python 3.10).
   Now west is invoked via the venv interpreter explicitly (`WEST_PYTHON`=3.12
   for cmake) without prepending the venv to PATH.
4. **cbprintf extern-C** (`nros-rmw-cyclonedds/src/vtable.cpp`): `<zephyr/
   logging/log.h>` was wrapped in `extern "C"`; on 4.x log.h pulls cbprintf_cxx.h
   (C++ overloads) which break under extern "C". Removed the wrap (log.h is
   C++-safe). Harmless on 3.7, fatal on 4.4.

**Boot smoke:** boots, `dds_create_participant` succeeds, cyclone app thread
starts. Two RUNTIME items remain (build is done): (a) **multicast join fails**
(`join conn (udp/239.255.0.1) ... continuing unicast-only`) — the NSOS
IP-multicast re-anchor, **Task 6**, runtime not build; (b) **`os: tid ... is in
use!`** — a 4.x cyclone-threads issue (the cyclonedds-zephyr threads patch /
dynamic-thread reuse needs 4.4 re-verification). Tasks 5–6 + threads are the
runtime follow-up; the cyclonedds-on-4.4 BUILD + participant init are proven.

## Runtime drill — Task 5 (NSOS recvmsg) fixed on 4.4 (2026-05-26)

Longer run of the cyclonedds 4.4 talker: cyclone **publishes** (data plane up),
but **receive busy-spun** with `UDP recvmsg sock N: ret 0 retcode -1` — the 4.x
NSOS `nsos_recvmsg` is an ENOTSUP stub. Ported the 3.7 fill to 4.4
(`scripts/zephyr/nsos-recvmsg-patch-4.4.sh`): delegate the single-iovec form to
`nsos_recvfrom`, adapted to `struct net_msghdr` + `net_sockaddr`/`net_socklen_t`.
**Verified:** recvmsg `-1` flood drops from hundreds to **0** in a 5 s run;
publish still works. Wired into `just zephyr setup` (4.4 branch). The patch is
idempotent + reproducible (revert→apply→re-apply checked).

Remaining runtime: **Task 6** (NSOS IP-multicast — `IP_ADD_MEMBERSHIP`, for SPDP
discovery; currently `multicast join failed … unicast-only`) and the
`os: tid … is in use!` cyclone-threads noise (non-fatal — publish/receive work).

## Runtime drill — Task 6 (NSOS IP-multicast) fixed on 4.4 (2026-05-26)

Ported the 3 legacy NSOS IPv4-multicast patches to 4.4 as two scripts
(`native-sim-ipproto-ip-patch-4.4.sh` = guest half: NSOS_MID_IP_* + struct
nsos_mid_ip_mreq + nsos_setsockopt/getsockopt `case NET_IPPROTO_IP` marshalling,
folding in the dual-mreq-size logic; `nsos-adapt-ipproto-ip-patch-4.4.sh` = host
half: unmarshal + real host setsockopt(IP_ADD_MEMBERSHIP)). Re-anchored to 4.4's
`net_`-typed shapes; reuse 4.4 net_compat.h's IP_*/ip_mreq constants (only the
NSOS wire-format MIDs+struct are added). Wired into the 4.4 setup branch.
**Verified:** build ok; the `multicast join failed … unicast-only` line is GONE
(`IP_ADD_MEMBERSHIP` reaches the host); `dds_create_participant` ok; publishes.

**Remaining (final e2e blocker): `os: tid … is in use!` → abort() (FATAL ERROR
4).** With recvmsg + multicast fixed, cyclone now spawns its discovery/receive
threads, which trip a 4.x thread-table clash and kernel-panic right after
`Published: 1`. This is a cyclone-zephyr **thread-model** issue on 4.4 (ddsrt
thread registration / kEmbeddedCycloneConfig, cf. Phase 177.22), distinct from
the socket-layer NSOS work. It is the last blocker before a 2-node cyclonedds
e2e on 4.4.

## Runtime drill — final blocker root-caused (gdb): cyclone mutex/thread semantics on 4.4 (2026-05-26)

After Tasks 5+6, cyclonedds 4.4 builds, publishes, and joins multicast, but
`abort()`s right after `Published: 1`. gdb backtrace pins it precisely:

```
abort() <- ddsrt_mutex_unlock (sync/posix/sync.c:63)
       <- handle_individual_xevent (q_xevent.c:1126, unlock xevq->lock)
       <- xevent_thread
```

`ddsrt_mutex_unlock` aborts because `pthread_mutex_unlock` -> `k_mutex_unlock`
returns **`-EPERM`**: Zephyr's `k_mutex` enforces **owner-only unlock**, but
cyclone's ddsrt assumes POSIX `PTHREAD_MUTEX_NORMAL` semantics (unlock not tied
to the locking thread). Linux / 3.7 tolerated it; 4.4 does not.

Linked symptom during `dds_create_participant`: `os: tid 0x… is in use!` from
`z_impl_k_thread_stack_free` (kernel/dynamic.c:121) — 4.x refuses to free a
dynamic thread's stack unless the thread is `_THREAD_DEAD`/`_THREAD_DUMMY`;
cyclone's pthreads free stacks before the threads terminate. The mutex-owner
EPERM is likely downstream of this thread-identity churn.

**Root:** cyclone's POSIX thread/mutex assumptions vs Zephyr 4.4's stricter
`k_mutex` ownership + dynamic-thread stack-free lifecycle. The 3.7
`cyclonedds-zephyr-threads` patch needs a 4.4 adaptation (stable thread
identity + join-before-stack-free, and either owner-preserving unlock or a
ddsrt sync shim that tolerates the Zephyr `k_mutex` ownership model). This is a
deep, near-research-grade concurrency fix — distinct from the socket-layer NSOS
work (Tasks 5/6, done). It is the sole remaining blocker before a stable
cyclonedds run + 2-node e2e on 4.4.

**cyclonedds-on-4.4 net:** BUILD ✓ · publish ✓ · recvmsg ✓ · multicast join ✓ ·
stable run ✗ (cyclone↔k_mutex ownership). Zenoh line is fully e2e-proven on
both 3.7 + 4.4; the cyclonedds runtime concurrency adaptation is the tracked
follow-up.

### Refinement (deeper gdb drill, 2026-05-26)

Two findings correct the earlier note:

1. **`tid in use` is benign noise**, not the root. gdb backtrace shows it fires
   from `ddsrt_thread_create` → `pthread_attr_destroy` (pthread.c) →
   `k_thread_stack_free`: cyclone destroys the pthread_attr immediately after
   `pthread_create` (standard POSIX), but Zephyr 4.4's `pthread_attr_destroy`
   tries to free `attr->stack` — which is the just-created thread's live stack.
   `k_thread_stack_free` sees the thread is not `_THREAD_DEAD` and **refuses
   (`-EBUSY`)**, so the stack is intact; only a `LOG_ERR` is emitted. (4.4's
   pthread model has the attr own the stack and expects the caller to destroy
   the attr *after* join — incompatible with cyclone's immediate destroy, but
   harmless because the free is refused.)

2. **The actual `abort()` is the `k_mutex` owner-only-unlock EPERM** in the
   xevent thread (`ddsrt_mutex_unlock(xevq->lock)`), independent of #1. gdb
   can't introspect the owner (`pthread_mutex_t` is an opaque pool index, not a
   struct), so cross-thread-unlock vs stale-owner (from dynamic-thread k_thread
   reuse) is unresolved without deeper instrumentation.

**Fix directions (research-grade, pick during the focused follow-up):**
- *Zephyr pthread/k_mutex*: make `pthread_mutex_unlock` for
  `PTHREAD_MUTEX_NORMAL/DEFAULT` not abort on non-owner unlock (POSIX leaves it
  undefined; Linux/3.7 tolerated it) — e.g. an owner-agnostic unlock path.
- *cyclone ddsrt sync*: map ddsrt mutex onto a non-ownership primitive, or make
  `ddsrt_mutex_unlock` tolerate the Zephyr EPERM.
- *cyclone threads patch*: if the EPERM is stale-owner from dynamic-thread
  reuse, stabilise thread identity (static pthread pool: drop DYNAMIC_THREAD,
  size POSIX_THREAD_THREADS_MAX with static stacks) and/or defer
  `pthread_attr_destroy` to join.

The minimal experiment to disambiguate is to disable CONFIG_DYNAMIC_THREAD for
the cyclonedds 4.4 build and see whether the mutex EPERM disappears (stable
k_thread identity). Deferred to the focused concurrency follow-up.

## Phase 180.C — module-shipped snippets (`west build -S nros-<rmw>`)

Zephyr 4.x can ship snippets from a module via `zephyr/module.yml`
`build.settings.snippet_root`. nano-ros now contributes three RMW snippets so a
downstream user selects the nano-ros RMW the 4.x-native way:

```sh
west build -b native_sim/native/64 -S nros-cyclonedds <app>
# RMW alternatives: -S nros-zenoh  /  -S nros-xrce
```

instead of the legacy overlay form:

```sh
west build -b native_sim/native/64 <app> -- -DCONF_FILE="prj.conf;prj-cyclonedds.conf"
```

**This is 4.x-only and purely additive.** Zephyr 3.7 has no snippet support, so
the `prj-<rmw>.conf` overlays and the `-DCONF_FILE=...` build path stay in place
and unchanged; `just zephyr build-one` still uses CONF_FILE for both lines.

Layout:

- `zephyr/module.yml` → `build.settings.snippet_root: zephyr`. The snippet root
  is resolved **relative to the module root** (the dir containing
  `zephyr/module.yml` = the repo root), and Zephyr's `snippets.py` then scans
  `<snippet_root>/snippets/`. Pointing it at `zephyr` makes Zephyr discover
  `zephyr/snippets/*/snippet.yml`. (`.` would scan `<repo-root>/snippets/`,
  which is the wrong place.)
- `zephyr/snippets/nros-{zenoh,cyclonedds,xrce}/snippet.yml` — each appends a
  sibling `<rmw>.conf` to `EXTRA_CONF_FILE`.
- `zephyr/snippets/nros-{zenoh,cyclonedds,xrce}/<rmw>.conf` — the RMW-common
  Kconfig (RMW select + transport + POSIX/threads + resource sizing), derived
  from the canonical `examples/zephyr/c/talker/prj-<rmw>.conf` overlay and kept
  in lockstep with it.

The snippets are **RMW-only**. The native_sim NSOS / line overlay
(`cmake/zephyr/native-sim-line-<v>.conf`, which sets `ETH_NATIVE_TAP=n` +
`NET_SOCKETS_OFFLOAD` etc.) is board+line specific and is deliberately *not*
baked into the snippet — it continues to be supplied via the `-D` / board
overlay path.

**Verified on 4.4** (2026-05-26): a `native_sim/native/64` build of
`examples/zephyr/c/talker` with `-S nros-cyclonedds` (and the NSOS line overlay
still via `-DCONF_FILE`) reports `Snippet(s): nros-cyclonedds`, merges
`zephyr/snippets/nros-cyclonedds/cyclonedds.conf`, applies
`CONFIG_NROS_RMW_CYCLONEDDS=y` + `CONFIG_CPP=y` + `CONFIG_NET_IPV4_IGMP=y` +
the heavy resource sizing into the merged `.config`, and links `zephyr.elf`.
`zephyr_settings.txt` carries `"SNIPPET_ROOT":".../nano-ros/zephyr"`.

## RESOLVED — cyclonedds-4.4 runtime: k_mutex fix (2026-05-26)

The `ddsrt_mutex_unlock` → `k_mutex` owner-only-unlock `-EPERM` → `abort()`
(root-caused above) is **fixed**. Zephyr maps pthread mutexes onto `k_mutex`,
which rejects unlock by a non-owner *and* unlock of an unlocked mutex with
`-EPERM`; POSIX leaves both UNDEFINED for `NORMAL`/`DEFAULT` and Linux/glibc
just perform the unlock — which cyclonedds' ddsrt relies on. The fix relaxes
`pthread_mutex_unlock` to force a releasable state (`owner=caller`,
`lock_count>=1`) so the unlock always succeeds (the first, narrower
non-owner-only attempt missed the `owner==NULL`/`lock_count==0` case).

Delivered as `scripts/zephyr/pthread-mutex-unlock-patch-4.4.sh` (wired into the
4.4 `just zephyr setup` branch) + `zephyr/patches/pthread-mutex-unlock-4.4.patch`
in `patches.yml` (BYO `west patch`; `upstreamable: false` — it changes Zephyr's
deliberate k_mutex strictness).

**Verified single-node:** with all NSOS patches + this fix, c/talker cyclonedds
on 4.4 runs the full window (no abort, exit 137 not 134), recvmsg flood = 0,
multicast join = OK, and **publishes 1→6** (one per second). A 2-node e2e (talker→listener over NSOS multicast) was attempted:
**both nodes run stably (no abort — the k_mutex fix holds for talker AND
listener), the talker publishes 1→11, but the listener Receives 0.** cyclone
is peer-to-peer (no router), so it relies on **multicast SPDP discovery between
two separate native_sim NSOS processes on host loopback** — which does not
bridge here (single-process multicast join succeeds, but cross-process
multicast RX on `lo` between two native_sim instances doesn't close discovery).
This is a **discovery-transport gap, distinct from the (resolved) k_mutex
runtime abort**; the likely fix is a unicast-peer cyclone config for native_sim
(`<Peers><Peer address="localhost"/>`, like ThreadX's AllowMulticast=false
path) rather than relying on loopback multicast. Tracked as a follow-on; the
cyclonedds-4.4 *runtime* (stable participant + publish/receive machinery, no
abort) is proven per-node.

## RESOLVED — cyclonedds-4.4 2-node discovery on native_sim (2026-05-26)

**c/talker → c/listener now exchanges data on native_sim: Published 11,
Received 10** (msgs 2–11; the first is lost during discovery, as expected).
Two compounding root causes, neither of which was the k_mutex runtime:

1. **Identical GUID prefix (the real blocker).** native_sim's fake entropy
   driver seeds from a *fixed* seed by default (`Using a test - not safe -
   entropy source` at boot). Two native_sim processes therefore produced the
   **same** cyclone participant GUID prefix (e.g. both
   `110462d:1f23f4a1:af20f854`). DDSI requires per-participant unique GUIDs;
   with identical GUIDs each node treats the other's SPDP as *its own* and
   drops it → no proxy participant → no match (the `finest` trace showed
   `spdp_write` + `nn_xpack_send 320` going out, but `match_writer_with_proxy_readers
   … tgt=0`). **Fix: distinct `--seed` per process** (native_sim accepts
   `--seed=<n>` / `--seed-random`). The e2e launches listener `--seed=11`,
   talker `--seed=22`; GUIDs then differ (`110595b:…` vs `110e5b4:…`) and
   discovery closes (16 proxy-participant SPDP events). **Any two native_sim
   cyclone nodes must be given different seeds.**

2. **Multicast breaks cyclone's select-based waitset on native_sim.** Enabling
   multicast SPDP (`AllowMulticast=spdp` + an `INADDR_ANY` join, ported from the
   ThreadX path) made `os_sockWaitsetWait: select failed` spam and stalled the
   talker at Published 1. native_sim's NSOS multicast RX fd does not survive
   cyclone's `select()` waitset. So native_sim uses **unicast SPDP** instead:
   `<AllowMulticast>false</AllowMulticast>` + `<Discovery><Peers><Peer
   Address="127.0.0.1"/></Peers><MaxAutoParticipantIndex>20</…></Discovery>`.
   Numeric `127.0.0.1` is required — NSOS `getaddrinfo` rejects the name
   `localhost` (`add_peer_addresses: localhost: not a valid address`).

**Delivery:** `packages/dds/nros-rmw-cyclonedds/src/session.cpp` —
`kEmbeddedCycloneConfig` is now also defined and passed to `dds_create_domain`
for `CONFIG_BOARD_NATIVE_SIM` (previously FreeRTOS/ThreadX only; native_sim
fell through to `dds_create_participant(domain, nullptr)` with the default
multicast config), with the unicast-discovery block above. The `ddsi_udp.c`
multicast-join experiment was reverted (multicast is a dead-end here per #2).

**Net (native_sim, 4.4):** BUILD ✓ · publish ✓ · recvmsg ✓ · k_mutex ✓ ·
**2-node unicast discovery ✓ · pub→sub data ✓.** Caveat: the `--seed`
requirement is native_sim-specific (real hardware/host has real entropy).
