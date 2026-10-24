---
id: 910
title: "migrating to zenoh-pico 1.10: the serial layer moved, `config.h` is no
  longer shipped, and our config generator is 54 knobs behind"
status: open
type: task
area: rmw, build
related: [phase-415, issue-0852, issue-0882, phase-508]
---

**Home: [phase-508](../roadmap/phase-508-zenoh-pico-1-10-migration.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)". It had no home after phase-355 was archived on 2026-10-12, because no active phase fit.

## Why

Our pin is 1863 upstream commits behind. The gap matters for serial specifically:
upstream **restructured the link transports** and now ships its own Zephyr UART
backend, so most of our Zephyr serial code has an upstream equivalent that did
not exist when we wrote it.

## What upstream changed

| | our pin (`dac320e3`) | upstream 1.10 |
| --- | --- | --- |
| Zephyr serial | ours, in `src/system/zephyr/network.c` | theirs, `src/link/transport/serial/uart_zephyr.c` |
| serial protocol | `src/system/common/serial.c` (ours) | `src/link/transport/upper/serial_protocol.c` |
| lwIP / sockets | `src/system/freertos/lwip` | `src/link/transport/**`, `src/system/socket` |
| `include/zenoh-pico/config.h` | checked in | **generated** from `config.h.in` by their CMake |
| system ABI (`_z_mutex_*`, `_z_task_*`) | `src/system/zephyr/` | unchanged — nano-ros's shims still fit |

The last row is the good news: `src/system/zephyr/system.c` still exists, so
`nros_zenoh_zephyr_system.c` continues to override the symbols it always did.

## Progress so far

Two branches on the fork (`jerry73204/zenoh-pico`), both off `upstream/main`:

- **`serial-fixes`** — one commit, for the upstream PR. Fixes
  `_z_zephyr_uart_read`, which busy-spins on `uart_poll_in` with no timeout, no
  yield and no `uart_err_check`, and returns `len` regardless. All three are
  defects we hit and fixed on our own port; the third is
  [issue 0852](archived/0852-zephyr-serial-rx-is-polled-and-overruns.md), where a silent overrun cost a long investigation that
  ended in a bug report against an innocent peer.
- **`nros-integration`** — `serial-fixes` plus a passthrough
  `include/zenoh-pico/config.h`, which upstream no longer ships. Not for
  upstream. A passthrough suffices because every platform in
  `config/*/nros-platform.toml` defines `ZENOH_GENERIC`; the 48 `@TOKEN@`
  substitutions are all in the dead `#else` branch, which is now an `#error` so
  a platform that forgets the define fails loudly.

Build progress against `nros-integration`, each item found by building and
fixed in turn:

1. `nros-platform.toml` named `system/freertos/lwip`, which no longer exists →
   `system/freertos`. (Reverted for now; correct only against the new pin.)
2. `zenoh-pico/config.h` missing → the passthrough above.
3. **Current blocker**, below.

## Current blocker — the config generator is 54 knobs behind

`nros-zpico-build`'s `config_header()` emits 49 of the 95 `Z_*` knobs upstream's
template defines. The build stops on the first missing ones:
`Z_CONFIG_SESSION_ZID_KEY`, `Z_GET_TIMEOUT_DEFAULT`, `Z_RUNTIME_MAX_TASKS`,
`Z_ZID_LENGTH`.

Of the 54 missing: **27 are `*_KEY`** (fixed protocol strings, not tunables),
**12 are `*_DEFAULT`**, and the rest are numeric knobs whose values are the 54
`set(Z_… CACHE STRING …)` lines in upstream's `CMakeLists.txt`.

**Do not transcribe them by hand.** That is what leaves us 54 behind again at
the next bump. `config_header()` should emit only the knobs nano-ros actually
*tunes* — buffer sizes, lease, feature flags — and derive the rest from
upstream's `CMakeLists.txt` defaults at build time. Then a bump costs nothing.

## Also outstanding

- One TU compiles without `ZENOH_GENERIC` and trips the new `#error`. Which one
  is not yet identified; the `#error` exists so it cannot be missed.
- Once it builds, our Zephyr serial code in `src/system/zephyr/network.c`
  should be **deleted** in favour of upstream's `uart_zephyr.c` plus the
  `serial-fixes` commit. That is the point of the migration.
- The INIT-retry scoping (fork commit `67ee0224`) is **not** needed upstream:
  upstream's `_z_connect_serial` never retries, so it never had the flood. That
  commit fixed a regression of ours and should not be carried forward.

## State

The submodule pin is back on `dac320e3` and the board builds. Nothing here is
half-applied; the migration lives on the two fork branches.


## 2026-09-03 — the target changes to 1.8.0, and the work moves to phase-415

This issue is written around **1.10**. Measured in the `ros2` distrobox with the
apt index refreshed: ROS Humble ships zenoh **1.8.0**
(`ZENOH_C "1.8.0"` in `zenoh_configure.h`; `ros-humble-rmw-zenoh-cpp` 0.1.9,
installed == candidate, built 2026-07-23 and unchanged since). We track what ROS
adopts, so 1.10 — two minors AHEAD of ROS — is the wrong target and
`nros-integration-1.10` is further from ROS than our 1.7.2 line is.

None of that branch's three nano-ros commits ports to 1.7.2 either: one is
1.10-only by construction (`config.h` becomes CMake-generated there), one
targets files that do not exist at 1.7.2, and one would REGRESS us — our serial
read already has the deadline, the `k_yield()` and the issue-0852 interrupt-driven
RX that commit lacks.

The 1.7.2 -> 1.8.0 port is surveyed in
[phase 415](../roadmap/archived/phase-415-zenoh-pico-1-8-0-patch-line.md): 85 files apply
clean, **42 conflict**, 2 moved, and one genuine name collision — our added
`session/keyexpr.h` versus upstream's `protocol/keyexpr.h` moved to the same
path. That is a decision, not a merge.

This issue stays open for the parts that are not the version bump: the config
generator being 54 knobs behind, the `ZENOH_GENERIC` TU, and the note that fork
commit `67ee0224`'s INIT-retry must NOT be carried forward because upstream
never had the flood it fixed.

## Update 2026-09-04 — 1.8.0 landed; this issue stays open for 1.10

Phase 415 moved the patch line to **1.8.0**, the zenoh ROS Humble actually
ships (`rmw_zenoh_cpp` 0.1.9 → `ZENOH_C "1.8.0"`). That is the version the
tracking rule names, so it closes the *drift* half of this issue. 1.10 is still
two minors AHEAD of ROS and remains the wrong target under that rule — this
issue stays open as the record of what a later move would cost, not as work
that is queued.

**One row of the table above is wrong and matters, because it is used as an
argument for 1.10 being special.** `include/zenoh-pico/config.h` is not
"checked in at our pin, generated at 1.10": **1.7.2 and 1.8.0 both generate it**,
via `configure_file(config.h.in -> config.h)` writing back into the SOURCE tree
(`CMakeLists.txt:345` at 1.7.2, `:379` at 1.8.0). It only *looks* checked in
because the generated file is committed and because `zpico-sys` compiles the
sources directly and never runs that CMakeLists — so for nano-ros the committed
`config.h` is the effective config and the generator never fires.

The practical consequence, found by building 1.8.0 the upstream way during the
port: anything we write into `config.h` alone is erased by the first `cmake`
configure. Two knobs were in that position and are now in `config.h.in`; the
`#ifndef` wrapping from `49012370` still is not (118 lines). Details and the
sweep command are in [phase-415](../roadmap/archived/phase-415-zenoh-pico-1-8-0-patch-line.md).

## Update 2026-10-24 — ROS Humble now ships 1.10.1, so 1.10 is the target again

The 2026-09-04 update parked this issue because 1.10 was two minors ahead of the
zenoh ROS ships. That stopped being true on 2026-10-11, when
`ros-humble-zenoh-cpp-vendor` **0.1.11** reached the Humble apt repository
(`1jammy.20261011`). Its `zenoh_cpp_vendor/CMakeLists.txt` reads "corresponds
to zenoh-c 1.10.1 plus few fixes"; 0.1.9's read "zenoh-c 1.8.0 plus few fixes".
The jazzy, kilted and rolling branches of `ros2/rmw_zenoh` are on 1.10.1 too,
though jazzy (0.2.11) and kilted (0.6.8) have not released it yet. The full
table and its sources are in
[phase-508](../roadmap/phase-508-zenoh-pico-1-10-migration.md).

Under the same tracking rule that made 1.8.0 the target, the target is now
**1.10.1**, and this issue is queued work again rather than a cost record.

Which of the measurements above still apply:

- The **1863 commits behind** and **54 knobs behind** counts are from the
  `dac320e3` (1.7.2) pin and overstate the gap from the 1.8.0 line phase-415
  landed. phase-508 re-measures both against 1.10.1.
- The structural points stand: upstream ships its own Zephyr UART backend and
  serial protocol, so the migration is mostly deleting ours; derive the knobs
  from upstream rather than transcribing them; find the TU that compiles
  without `ZENOH_GENERIC`; and do not carry fork commit `67ee0224` forward.
- The `nros-integration-1.10` branch's serial commit still lacks the deadline,
  `k_yield()` and the issue-0852 interrupt-driven RX our port has, so it is a
  reference, not something to cherry-pick.

Not yet measured: whether a 1.8.0 image interoperates with a 1.10.1 router.
Issue 0291 expects it to, since the wire protocol is stable across 1.x, and
phase-508's first work item checks it.

## Survey 2026-10-24 — the 1.8.0 patch line against upstream 1.10.1

Measured on a scratch clone of the fork, nothing pushed. Superproject pin
`57bbde9e` (`version.txt` 1.8.0), which is the tip of the fork's `nano-ros`
branch; upstream tags `1.8.0` (`eb47e2e0`, the patch line's merge base) and
`1.10.1`.

| | count |
| --- | --- |
| patch-line commits on top of 1.8.0 | 92 |
| of those, already carried by 1.10.1 (`git cherry`) | 2: `66b5bf9b` `_z_socket_accept` field name, `75dd7713` `z_random_fill` pointer advance |
| upstream commits 1.8.0 → 1.10.1 | 61, touching 440 files |
| files the patch line touches | 88, of which 50 upstream also changed |
| trial merge onto 1.10.1: files that merge clean | 66 |
| trial merge onto 1.10.1: files that conflict | **22** (phase-415's 1.7.2 → 1.8.0 port had 42) |

The 22 conflicts, by kind:

- **content (15):** `examples/unix/c11/z_pub_st.c`, `config.h.in`,
  `link/link.h`, `session/keyexpr_match_template.h`,
  `system/common/platform.h`, `system/platform/unix.h`,
  `transport/transport.h`, `src/net/session.c`, `src/session/keyexpr.c`,
  `src/system/common/platform.c`, `src/system/unix/network.c`,
  `src/system/zephyr/network.c`, `src/system/zephyr/system.c`,
  `src/transport/unicast/lease.c`, `src/transport/unicast/read.c`.
- **file location (4):** our added `include/zenoh-pico/system/link/{can,custom,isotp,ivc}.h`
  sit in a directory upstream renamed, so they move with it.
- **modify/delete (3):** `include/zenoh-pico/config.h` (1.10.1 no longer has
  it in the tree), `src/system/common/serial.c` and
  `src/system/freertos/lwip/network.c` (both replaced by upstream's
  `src/link/transport/**` layout). These are the restructure this issue
  predicted; the serial one is the deletion the migration is for.

Config knobs (`#define Z_*` in `config.h.in`): 83 at 1.8.0, 96 at 1.10.1. New
in 1.10.1 (14): the connect/listen timeout and exit-on-failure `_KEY`/`_DEFAULT`
pairs, `Z_RUNTIME_MAX_TASKS`, `Z_RUNTIME_IDLE_READ_TASK_SLEEP`,
`Z_MAX_NUM_SCOUT_INTERFACES`, `Z_TRANSPORT_ACCEPT_TIMEOUT` and
`Z_TRANSPORT_CONNECT_TIMEOUT`. Removed (1): `Z_FEATURE_PERIODIC_TASKS`, which
our generator still emits. `config_header()` emits 41 of the 96 and leaves 55
(27 `*_KEY`, 12 `*_DEFAULT`, 16 sizes and timeouts) to upstream's defaults,
which at 1.8.0 came from the committed `config.h` and at 1.10.1 have no
committed home. Deriving them from upstream rather than transcribing them is
still the plan. The generator's 12 knobs that are not in 1.10.1's template are
our fork's own additions (the CAN/custom/ISO-TP/IVC links, the RX task budget,
the TX split lock, the lease sleep chunk) plus `Z_FEATURE_SCOUTING_UDP`,
`Z_FEATURE_UNSTABLE_API` and the removed `Z_FEATURE_PERIODIC_TASKS`.

Reproduce, from a clone of `jerry73204/zenoh-pico` with the two upstream tags
fetched: `git checkout 1.10.1 && git merge --no-commit 57bbde9e` and count
`git diff --name-only --diff-filter=U`; `git cherry 1.10.1 57bbde9e 1.8.0` for
the carried commits.

## Measurement 2026-10-24 — a 1.8.0 image against a 1.10.1 `rmw_zenohd`

Interop holds, so the move is not urgent. The same nano-ros fixtures (zenoh-pico
pin `57bbde9e`, 1.8.0) ran the zenoh live-peer interop binaries against two ROS
stacks on one host:

| stack | router and `ros2` peer | zenoh-c | result |
| --- | --- | --- | --- |
| installed Humble | `rmw_zenoh_cpp` 0.1.9 | 1.8.0 | 27 passed, 3 skipped |
| Humble 0.1.11 overlay | `rmw_zenoh_cpp` 0.1.11 | 1.10.1 | 27 passed, 3 skipped |

The same 27 tests passed in both runs; each run also skipped 3, among them
the ignored `ros2_topic_echo_sees_each_nodes_parameter_events`. The binaries were `interop_e2e`,
`graph_interop`, `rosout_interop`, `params_per_node_interop` and
`qos_event_interop`, plus the zenoh tests of `ros2_action_e2e`. Together they
cover pub/sub in both directions, a stock `demo_nodes_cpp` peer, services in
both directions, the lifecycle cycle, graph enumeration and the graph-change
guard, publisher GIDs, `/rosout`, per-node `ros2 param`, the liveliness event,
and actions in both directions. The filter also caught some Cyclone and XRCE
cases, which say nothing about zenoh; they passed in both runs too.

How the overlay was arranged, so the run can be repeated without sudo:
`apt-get download` the jammy `ros-humble-rmw-zenoh-cpp` and
`ros-humble-zenoh-cpp-vendor` at `0.1.11-1jammy.20261011`, `dpkg -x` both into
one prefix, then prepend `<prefix>/opt/ros/humble` to `AMENT_PREFIX_PATH`,
prepend its `opt/zenoh_cpp_vendor/lib` and `lib` to `LD_LIBRARY_PATH`, and set
`NROS_RMW_ZENOHD` to its `lib/rmw_zenoh_cpp/rmw_zenohd`. The router then loads
the overlay's `libzenohc.so`, because `paired_zenoh_library_dir` derives it from
that path. The `ros2` peer loads it too: re-sourcing `/opt/ros/humble/setup.bash`
keeps the overlay first, and `ldd` on the resolved `librmw_zenoh_cpp.so`
confirms it. Before the run, `strings` on that `libzenohc.so` showed
`ZENOH_C "1.10.1"`.

## Correction 2026-10-24 — Zephyr serial is a re-port, not a deletion

This issue, and phase-508 before today, said most of our Zephyr serial code has
an upstream equivalent, so the migration would mostly delete it. Measured, it
does not. Ours is `src/system/zephyr/network.c`, 2451 lines at the pin, with an
interrupt-driven and async receive path (119 lines reference `uart_irq`,
`uart_rx_enable`, async or DMA). Upstream 1.10.1's
`src/link/transport/serial/uart_zephyr.c` is 129 lines. On POSIX, upstream's
`tty_posix.c` (183 lines) maps baud rates only up to `B230400`; ours reaches
921600. So the plan is to carry our implementation into upstream's
`src/link/transport/{tcp,udp,serial}/*_<platform>.c` layout, take upstream's
serial protocol framing where it matches ours, and add the higher baud rates
to `tty_posix.c` rather than keeping a second POSIX serial backend.

## Survey 2026-10-24 (second pass) — the async executor and the link ABI

A replay of the patch line was started on a scratch clone and stopped at
commit 7 of 92. The merge survey above counted conflicting FILES, and missed
two upstream changes that decide the cost.

**1. Upstream 1.9.0 replaced the read and lease threads with an async
executor** (`65f80db8`, "Asynchronous executor (#1190)", then `278f4629`
and `6cdd442a`). Read, lease, keep-alive, query timeout and reconnect are
now futures under `src/runtime/`:

- In a multi-thread build, all of them run on ONE executor thread, which
  holds its mutex while a task runs (`background_executor.c:56-83`).
- `zp_start_read_task` starts that thread. `zp_start_lease_task` and
  `zp_stop_lease_task` are no-ops that return OK (`api.c`).
- `zp_read`, `zp_send_keep_alive` and `zp_spin_once` exist only when
  `Z_FEATURE_MULTI_THREAD == 0`, and in a single-thread build
  `zp_spin_once` runs one task per call.
- `Z_RUNTIME_MAX_TASKS` and `Z_RUNTIME_IDLE_READ_TASK_SLEEP` exist only as
  `config.h.in` substitutions, with no `#ifndef` fallback, so our generator
  must emit both. `zp_spin_once` returns `false` only when
  `Z_RUNTIME_IDLE_READ_TASK_SLEEP > 0`, so a drain loop over it needs that
  knob or a budget.

For nano-ros that means `zpico.c` must change how it drives the session, at
about 25 call sites:

- **Lease task:** its priority, stack and attributes become dead. The
  transport priority band from issue 0623 collapses to one executor
  priority, and one stack must hold read, lease, reconnect (a blocking
  connect) and user callbacks.
- **ThreadX multi-thread arm:** it calls `zp_read` from tier threads with no
  executor running, so it no longer compiles. It must start the executor or
  build single-threaded.
- **`zpico_send_keep_alive`:** it is unguarded, so it must use the internal
  `_zp_send_keep_alive`.
- **Single-thread polling** (bare-metal smoltcp, serial): it must move from
  `zp_read` + `zp_send_keep_alive` + `_z_pending_query_process_timeout` to a
  bounded `zp_spin_once` loop. On the old path, the lease, reconnect and
  query-timeout futures would never run.

**2. The platform link ABI was renamed and reshaped.** `_z_open_tcp` became
`_z_tcp_open`, `_z_send_tcp` became `_z_tcp_write`,
`_z_read_serial_internal` became `_z_serial_read`, and so on, declared in
`include/zenoh-pico/link/transport/{tcp,udp_unicast,udp_multicast,serial,socket}.h`.
`_z_socket_wait_event` became `_z_socket_wait_readable` over an iterator. On
our side this reaches:

- the alias TU `zpico-sys/c/zpico/platform_aliases.c`, which carries about
  37 aliases;
- `zpico-sys/c/platform/threadx/network.c`;
- `zpico-serial/src/ffi.rs`;
- every `extra_sources` row in `nros-rmw-zenoh/nros-rmw.toml` that names
  `system/<rtos>/network.c`;
- our own CAN, ISO-TP, IVC and custom links.

1.10.1 also gained `ZP_SYSTEM_PLATFORM_HEADER`, which selects a platform
header by macro. That may let `ZENOH_GENERIC`'s header plumbing go.

**The 21 transport and session patches, classified against 1.10.1's code**

Each was read against upstream's code, not run. References are to
`up-1.10.1` files.

| class | commits |
| --- | --- |
| OBSOLETE (5) | `0ac5c201` batch guard (transport.h:307-321, `07c84ebc`); `23a117d2` #0959 chunked lease sleep (the lease is a timed future, and stop signals the executor's condvar); `0343ad1b` MATCHING=0 guard (filtering.c:323, `07c84ebc`); `a1c741db` `== true` in `#if` (`08e4e584`); `7335492a` peer-mode read remainder (inert for our client-mode islands) |
| PORT (12) | `affe5a6e` query `_start_time` after send (primitives.c:605/621); `ccffda45` try-lock keep-alive (lease.c:66, :202 — now it also stalls read); `4b72b53e` OOM in `_z_unsafe_register_pending_query` (query.c:98); `98ab67c4` write filter off on single-thread (filtering.c:251); `8330e0f3` and `86f1b1df` split TX lock (tx.c is almost unchanged); `730fa365` declarations always express (tx.c:40); `52f60b79` one rejected message must not kill the read task (now worse: the error closes the transport and reconnects, read.c:401); `6b574173` lock the declaration cache (session.c:613, after the #0899 redesign; its retry part is obsolete); `eb16447f` DECLARE_FINAL clones the interest (interest.c:508, unchanged); `57bbde9e` a dropped DROP put reports OK (tx.c:296); `717153c1` #0924 SO_LINGER abort, moving to `tcp_lwip.c:_z_tcp_lwip_close` (its `_reconnecting` guard is obsolete) |
| REDESIGN (2) | `ae59aa48` #0899 teardown use-after-free: `_zp_unicast_failed_result` frees the transport under `_mutex_transport` while `_z_send_n_msg` takes no lock, and reconnect holds that mutex across a blocking `_z_open`, so porting the lock as is deadlocks or stalls. The fix must mark the transport `NONE` under the lock and close it outside the lock. `00f8e2f0` read-task frame budget: `run_forever` loops without yielding while any task is ready, so the budget has to become a `wake_up_after(rest_ms)` from the read future, with `rest_ms` of at least 2, because the executor only waits when the next wake is more than 1 ms away. |
| DEPENDS ON the `zpico.c` choice (2) | `81459416` and `f13ae3b5`, the polled-read drain and compaction. They are OBSOLETE if single-thread polling moves to `zp_spin_once`, whose read future already compacts the remainder (read.c:82-115). They are needed if `zp_read` stays. The two paths must never be mixed. |

**Other behaviour changes between 1.8.0 and 1.10.1:**

- Query timeouts are checked every 1 s. Before, the check ran every `lease/3`,
  which is 20 s at our 60 s lease.
- A read error closes the transport and reconnects every 1 s.
- User callbacks run on the executor thread, so a slow callback delays the
  lease and keep-alive.
- The handshake takes `Z_TRANSPORT_CONNECT_TIMEOUT`.
- Zephyr gained a thread-stack slot pool, `CONFIG_ZENOH_PICO_THREADS_NUM`
  (default 4), next to our own `5c000233`.
- The executor adds a mutex and a condvar per session, so recheck Zephyr's
  pthread pool counts.

What this does to the estimate: the move is no longer a rebase plus a serial
re-port. It is a rebase plus a change to how nano-ros drives zenoh-pico on
every platform, with two patches to redesign and the platform link ABI to
re-port. The first four commits replayed cleanly onto 1.10.1, and two
carried commits were dropped; that work is kept on the scratch branch
`port/wip-1.10.1-partial`, which was not pushed.
