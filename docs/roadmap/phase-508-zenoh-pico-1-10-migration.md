# Phase 508 — move the zenoh-pico fork onto 1.10

**Status (2026-10-24). In progress, and now the TRACKED target: ROS Humble
ships zenoh 1.10.1, so the target is 1.10.1, not "1.10" in general. The port
survey is done (22 conflicting files, half of phase-415's 42); nothing has
moved on the fork or the pin.** The phase
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
interop is expected to hold, but nothing has MEASURED a 1.8.0 image against a
1.10.1 router. That measurement is the first work item, because it decides how
urgent the rest is.

Upstream zenoh-pico tags 1.10.0 (2026-08-14) and 1.10.1 (2026-09-07); 1.10.1
is the newest release.

## What the migration is

[Issue 0910](../issues/0910-zenoh-pico-1-10-migration.md) has the measurements,
taken from the pre-phase-415 pin (`dac320e3`, 1.7.2), so its counts overstate
the gap from today's 1.8.0 line and are re-measured by the second work item.
Our fork's pin is behind upstream, and upstream has since restructured the
link transports. It now ships its own Zephyr UART backend and serial protocol,
generates `config.h` from `config.h.in`, and has knobs our config generator
does not emit. Most of our Zephyr serial code now has an upstream equivalent,
so the migration is mostly deletion. The system ABI our shims implement
(`_z_mutex_*`, `_z_task_*`) is unchanged.

## Work items

- [ ] Measure a 1.8.0 image against a 1.10.1 `rmw_zenohd` (Humble's
      `rmw_zenoh_cpp` 0.1.11, in the ros2 distrobox or a live-peer lane): pub,
      sub, service and liveliness/graph. A failure here makes the rest urgent
      and is filed as its own issue.
- [x] Survey the port the way phase-415 surveyed 1.7.2 → 1.8.0: merge the
      `nano-ros` patch line onto upstream `1.10.1` and record what applies
      clean, what conflicts and what upstream now carries, plus the config-knob
      gap between `nros-zpico-build`'s `config_header()` and 1.10.1's
      `config.h.in`. DONE 2026-10-24: 22 of 88 patched files conflict (phase-415
      had 42), 2 of 92 commits are carried upstream, and the generator leaves
      55 of 96 knobs to upstream defaults. Numbers and the conflict list are in
      issue 0910's 2026-10-24 survey.
- [ ] Rebase the fork's `nano-ros` patch line onto upstream 1.10.1, keeping the
      patches as individual commits (CLAUDE.md, vendored-fork workflow). Drop
      each patch that upstream now carries, and record each drop.
- [ ] Generate `config.h` the way upstream does, and close the gap between the
      knobs upstream defines and the ones our generator emits.
- [ ] Move Zephyr serial onto upstream's `uart_zephyr.c` and serial protocol,
      then delete ours.
- [ ] Bump the superproject pin forward only, after the fork branch is pushed.

## Acceptance

Issue 0910 is resolved. Tier 2's zenoh cells and the Zephyr serial cells pass
on the new pin, and an image interoperates with the `rmw_zenohd` of Humble's
`rmw_zenoh_cpp` 0.1.11 (zenoh 1.10.1).

## Issues homed here (2026-10-17)

| issue | why it belongs here |
| --- | --- |
| [#0910](../issues/0910-zenoh-pico-1-10-migration.md) | The migration itself. Phase-355 was archived without it, and nothing else fits. |
