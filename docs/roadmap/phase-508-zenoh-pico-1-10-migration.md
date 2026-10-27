# Phase 508 — move the zenoh-pico fork onto 1.10

**Status (2026-10-25). In progress, and now the TRACKED target: ROS Humble
ships zenoh 1.10.1, so the target is 1.10.1, not "1.10" in general. The port
survey is done (22 conflicting files, half of phase-415's 42), and a 1.8.0
image interoperates with a 1.10.1 `rmw_zenohd` (27 of 27 interop tests, the
same as against 1.8.0), so the move is planned work, not a fix. A second
survey found the move is bigger than a rebase: upstream 1.9 replaced the read
and lease threads with an async executor, and renamed the platform link ABI,
so `zpico.c`'s session driving and every platform's link shims change too
(issue 0910's second-pass survey). On 2026-10-25 the patch line was rebased
onto 1.10.1 and the config generator brought up to its knobs, both on local
branches: the fork branch waits for the maintainer to push it, and the pin
cannot move until `zpico.c` and the link shims are re-ported (issue 0910's
2026-10-25 rebase section).** The phase
was opened on 2026-10-17 to give issue 0910 a home after
[phase-355](archived/phase-355-dependency-and-fork-debt.md) was archived on
2026-10-12. At that point the move was parked: issue 0910's 2026-09-04 update
ruled 1.10 the wrong target, because we track the zenoh ROS ships and Humble
then shipped 1.8.0. That premise no longer holds (next section).

## Why the target moved (checked 2026-10-24)

The tracking rule is unchanged: our pin follows the zenoh that the ROS
`rmw_zenoh_cpp` links, because that is the router an image talks to (issue
0609's drift class, RFC-0075). What changed is the ROS side.

| distro | `zenoh_cpp_vendor` released on apt | its zenoh-c | branch head |
| --- | --- | --- | --- |
| humble | 0.1.11 (`1jammy.20261011`, tag dated 2026-10-06) | **1.10.1** | 1.10.1 |
| jazzy | 0.2.11 (`1noble.20260914`) | 1.8.0 | 1.10.1 |
| kilted | 0.6.8 (`1noble.20260914`) | 1.8.0 | 1.10.1 |
| rolling | — | — | 1.10.1 |

Sources: the `Version:` field of `ros-<distro>-zenoh-cpp-vendor` in
`repo.ros2.org/ubuntu/main/dists/<codename>/main/binary-amd64/Packages.gz`, and
the "corresponds to zenoh-c …" comment in `zenoh_cpp_vendor/CMakeLists.txt` of
`ros2/rmw_zenoh` at each tag and branch. Humble's 0.1.9 → 0.1.11 is the step:
0.1.9 says "zenoh-c 1.8.0 plus few fixes", 0.1.11 says "zenoh-c 1.10.1 plus few
fixes". Every distro branch is on 1.10.1, so jazzy and kilted follow at their
next sync.

So a Humble host that runs `apt upgrade` today runs an `rmw_zenohd` two minors
ahead of our 1.8.0 pin. The wire protocol is stable across 1.x (issue 0291), so
interop was expected to hold. The first work item measured it, and it does.

Upstream zenoh-pico tags 1.10.0 (2026-08-14) and 1.10.1 (2026-09-07); 1.10.1
is the newest release.

## What the migration is

[Issue 0910](../issues/0910-zenoh-pico-1-10-migration.md) has the measurements,
taken from the pre-phase-415 pin (`dac320e3`, 1.7.2), so its counts overstate
the gap from today's 1.8.0 line and are re-measured by the second work item.
Our fork's pin is behind upstream, and upstream has since restructured the
link transports into `src/link/transport/{tcp,udp,serial}/*_<platform>.c`.
It now ships its own Zephyr UART backend and serial protocol, generates
`config.h` from `config.h.in`, and has knobs our config generator does not
emit. This doc used to say most of our Zephyr serial code now has an upstream
equivalent, so the migration is mostly deletion. Measured, it is not: ours is
2451 lines with interrupt-driven and async receive, and upstream's
`uart_zephyr.c` is 129 lines. Our serial code moves into upstream's layout
rather than being deleted (issue 0910's 2026-10-24 correction). The system ABI
our shims implement (`_z_mutex_*`, `_z_task_*`) is unchanged.

## Work items

- [x] Measure a 1.8.0 image against a 1.10.1 `rmw_zenohd` (Humble's
      `rmw_zenoh_cpp` 0.1.11, in the ros2 distrobox or a live-peer lane): pub,
      sub, service and liveliness/graph. A failure here makes the rest urgent
      and is filed as its own issue. DONE 2026-10-24: the zenoh live-peer
      interop binaries pass 27 of 27 against both 0.1.9 (zenoh 1.8.0) and
      0.1.11 (zenoh 1.10.1), with router and `ros2` peer both on the overlay.
      The setup and the test list are in issue 0910's 2026-10-24
      measurement.
- [x] Survey the port the way phase-415 surveyed 1.7.2 → 1.8.0: merge the
      `nano-ros` patch line onto upstream `1.10.1` and record what applies
      clean, what conflicts and what upstream now carries, plus the config-knob
      gap between `nros-zpico-build`'s `config_header()` and 1.10.1's
      `config.h.in`. DONE 2026-10-24: 22 of 88 patched files conflict (phase-415
      had 42), 2 of 92 commits are carried upstream, and the generator leaves
      55 of 96 knobs to upstream defaults. Numbers and the conflict list are in
      issue 0910's 2026-10-24 survey.
- [x] Survey what upstream 1.9's async executor does to our 21 transport and
      session patches and to `zpico.c`. DONE 2026-10-24: 5 obsolete, 12 port,
      2 redesign (#0899 teardown, the read-task frame budget), 2 depend on how
      single-thread builds poll. Issue 0910's second-pass survey has the table.
- [ ] Decide how each build drives the executor. Drafted 2026-10-24 as
      [RFC-0110](../design/0110-zenoh-pico-executor-driving.md): THREAD mode
      (POSIX, Zephyr, NuttX, FreeRTOS) runs the one executor thread, which
      replaces the read and lease tasks; CALLER mode (bare metal, ThreadX)
      runs a bounded `zp_spin_once` loop, and ThreadX gets a fork patch to
      run the multi-thread executor from the caller. Done when RFC-0110's
      measurements are taken and it leaves Draft. MEASURED 2026-10-27: four
      of five hold (the `MAX_TASKS` floor is 5, ThreadX tiers pass, stack
      peaks are under 2.3 KiB, and app threads do not spawn in steady
      state). The NuttX quiet-link keep-alive FAILED, which was
      [issue 2094](../issues/archived/2094-nuttx-quiet-link-keepalive-starves-under-executor.md).
      It was FIXED 2026-10-27: NuttX linked the alias TU's `uint64_t` clock
      against `unix.h`'s `struct timespec`, and once the right clock was
      linked it wrapped every 4.29 s. A quiet NuttX link now holds one
      session, with keep-alives every 25–30 s. Still open:
      [issue 2105](../issues/2105-zephyr-cortex-m-timed-future-never-rewakes.md),
      a timed future that never re-woke on a Zephyr Cortex-M image. RFC-0110
      stays Draft until it is explained.
- [ ] Rebase the fork's `nano-ros` patch line onto upstream 1.10.1, keeping the
      patches as individual commits (CLAUDE.md, vendored-fork workflow). Drop
      each patch that upstream now carries, and record each drop. Redesign
      `ae59aa48` (#0899) and `00f8e2f0` (frame budget) for the executor rather
      than replaying them. DONE LOCALLY 2026-10-25 as branch `nano-ros-1.10.1`
      (70 commits on `1.10.1`) in the superproject's zenoh-pico module store,
      not yet on the fork remote. Every one of the 92 commits is accounted for
      in issue 0910's 2026-10-25 section. Builds upstream-style on Linux in
      five feature sets, with upstream's unit tests passing. Checked off when
      the branch is pushed.
- [x] Re-port nano-ros's platform link shims to 1.10.1's renamed link ABI:
      the alias TU, `zpico-serial`'s FFI, the `extra_sources` rows in
      `nros-rmw.toml`, and our CAN, ISO-TP, IVC and custom links. DONE
      2026-10-26 on `feat/508-zenoh-1.10`. ThreadX's sockets turned out to
      come from the platform net ABI (`nros-platform-threadx/src/net.c`), not
      `c/platform/threadx/network.c`, which nothing has compiled since
      phase-80; the socket half 1.9 split out of each link is
      `c/zpico/platform_socket.c` for ThreadX and bare metal. The serial
      handshake is upstream's now (`_z_connect_serial`), so `zpico-serial`
      only opens the port. Verified by a build and an e2e run on every
      platform: Linux, ThreadX-Linux, ThreadX riscv64, FreeRTOS, NuttX, bare
      metal (smoltcp and serial) and Zephyr (Rust and C; Zephyr C++ is
      blocked by a pre-existing link failure — issue 0910's 2026-10-26
      table).
- [x] Re-port `zpico.c` to the executor (RFC-0110 D1–D5). DONE 2026-10-26
      on the same branch. THREAD mode on POSIX, Zephyr, NuttX and FreeRTOS:
      one `zp_start_read_task`, no lease task. CALLER mode on bare metal and
      ThreadX: a bounded `zp_spin_once` loop. The keep-alive ping uses
      `_zp_send_keep_alive`. The lease knobs still parse; the executor stack
      is the larger of the read and lease stacks, and retiring them is the
      next item.
- [x] Retire and refuse the lease-task knobs (RFC-0110 D2, step 6). DONE
      2026-10-26. The knob registry gained a `retired` field: the row stays
      as the error table, every spelling (env, `[image.<id>] env`,
      `[image.<id>.knobs]`) is refused with the reason in the Rust ladder,
      the cmake twin and the CLI, and no Kconfig symbol is generated, so a
      conf that assigns it fails as an undefined symbol. `[node.rt]` lease
      keys, `nros_app_config_t`'s `keepalive_*` fields, the shim's
      `ZPICO_LEASE_TASK_PRIORITY` and the lease half of
      `zpico_set_task_config` are gone; the transport band is
      `{executor, poll}`.
- [ ] Generate `config.h` the way upstream does, and close the gap between the
      knobs upstream defines and the ones our generator emits. Stop emitting
      `Z_FEATURE_PERIODIC_TASKS`, which 1.10.1 removed. DONE ON A LOCAL BRANCH
      2026-10-25 (`feat/508-zenoh-1.10-knobs`, commit "state the knobs
      zenoh-pico 1.10 reads"): the gap was 7 knobs, not the 55 first counted,
      because only the template's non-generic branch is skipped by a
      ZENOH_GENERIC build. `check-zenoh-config-template` now fails on any
      such knob the generator does not state. The fork carries the header a
      default configure writes, for builds that do not run its CMake. The
      branch does not build against the 1.8.0 pin, so it lands with the bump.
- [ ] Move our Zephyr serial implementation into upstream's layout as
      `src/link/transport/serial/uart_zephyr.c`, keeping its interrupt-driven
      and async receive paths, and take upstream's serial protocol framing
      where it matches ours. Upstream's 129-line backend does not replace it.
      DONE ON THE FORK BRANCH, NOT YET BUILT FOR ZEPHYR: ours keeps its own
      framing, through a new `ZP_PLATFORM_SERIAL_FRAMING` hook in upstream's
      serial protocol layer, because receiving whole frames by interrupt or
      DMA depends on owning it. The wire format is upstream's either way.
      Checked off when a Zephyr serial image builds and passes its cells.
- [ ] Extend upstream's `tty_posix.c` with the baud rates above `B230400`
      that ours supports (up to 921600), then drop our POSIX serial code. DONE
      on the fork branch (the port of `e2b48046`); checked off with the push.
- [ ] Bump the superproject pin forward only, after the fork branch is pushed.

## Acceptance

Issue 0910 is resolved. Tier 2's zenoh cells and the Zephyr serial cells pass
on the new pin, and an image interoperates with the `rmw_zenohd` of Humble's
`rmw_zenoh_cpp` 0.1.11 (zenoh 1.10.1).

## Issues homed here (2026-10-17)

| issue | why it belongs here |
| --- | --- |
| [#0910](../issues/0910-zenoh-pico-1-10-migration.md) | The migration itself. Phase-355 was archived without it, and nothing else fits. |
