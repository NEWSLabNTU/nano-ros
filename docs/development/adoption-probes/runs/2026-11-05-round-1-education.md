# Adoption probe report: education, round one (2026-11-05)

Saved from the prober's hand-back message (its own write was refused).
Release probed: nano-ros 0.6.0-nros6 on ubuntu:24.04. Run log: run-log.md.

## Install time (T0 = 16:33:32 UTC, first apt command on installation.md)

| Milestone | Since T0 |
|---|---|
| First QEMU output line (`nros QEMU Platform` banner) | 8 min 50 s |
| Promised line `[INFO] sensor_node: [ 1.18] Publishing: 'Hello World: 1'` | 12 min 04 s |
| `ros2 topic echo` prints `data: 'Hello World: 31'` | 22 min 26 s |

Within budget only with four off-book workarounds (two source symlinks, a
`QEMU_SYSTEM_ARM` override, Jazzy from outside knowledge, force-removing an apt
package the book requires). Following the book exactly stops at 7:05
(`nros run` missing) and 7:37 (`nros build` fails), with no documented way on.

## Task verdicts

1. Install a seat: done with stops (1-9, 12-14); not possible from the book alone.
2. Plan a semester's labs: done with stops; only pub/sub has an emulator chapter.
3. Grading line: done with stops; `nros run --expect` is absent from the release, a hand-written grader PASSes with the router and FAILs without.
4. Pin the semester: done with stops; both seats byte-identical ELF, but `nros setup` fetched a newer index (QEMU nros8 vs the release's nros7) and rewrote the pinned lock, so the next build refused on drift.
5. First error: its message does not tell a student what to type next.

## Lab coverage (task 2)

| Topic | Emulator chapter | Status |
|---|---|---|
| Publish and subscribe | first-project.md; talker sections of the platform pages | Talker covered; no subscriber walkthrough |
| QoS | concepts/status-events.md (API only) | No chapter |
| Service | `nros new --template server/client` only; GitHub pointers | No chapter |
| Parameters | workspace-bringup.md, simulated-time.md (Linux host) | No chapter |
| Lifecycle | mentioned only | No chapter |
| Real-time tasks and timing | threadx.md one paragraph; rt-execution-model uses contributor-only `cargo nextest` | No chapter |
| ROS 2 graph on the host | first-project.md "See it from ROS 2" | Covered, with stops |

## Stops

1. installation.md "Check the install": `nros dev sdk root` -> `error: unrecognized subcommand 'dev'` (+2:33).
2. first-project.md "Run it": `nros run --expect 'Publishing:'` -> `error: unrecognized subcommand 'run'` (+7:05).
3. first-project.md "Scaffold the talker": `nros new sensor_node --board mps2-an385-baremetal --template talker` -> `requires --platform <p>` (+6:26); generated tree differs from the page.
4. bare-metal.md "Build": `nros build` -> `zenoh-pico source not provisioned at ".../0.6.0-nros6/share/nano-ros/packages/rmw/zenoh/zpico-sys/zenoh-pico". Run: nros setup --source zenoh-pico`; that command skips as already present and the build fails again (+7:37).
5. bare-metal.md "Build": `note: no qemu-system-arm ... (pinned 11.0.0-nros7) — provision it with nros setup --tool qemu`; setup says `present 11.0.0-nros8 (skip)`, note repeats (+8:14).
6. first-project.md intro: requires Humble + ros-humble-rmw-zenoh-cpp, not installable on 24.04, no install steps; contradicts support-status.md / ros2-interop.md (+9:24).
7. installation.md "Host prerequisites": `python3-catkin-pkg` makes `ros-jazzy-ros-base` fail with a file overwrite conflict (+11:05).
8. first-project.md "See it from ROS 2": Jazzy `ros2 topic echo` prints nothing for a humble-edition image; no error anywhere (+12:40 to +21:00).
9. installation.md "Provision a whole workspace": drift error; `nros setup` refuses board key `mps2-an385` that `nros new` wrote (+20:42).
10. installation.md "Pinning a version": `nros setup` rewrote the pinned project's `nros-sdk.lock` to nros8; next build refused on drift (seat 2, +3:55).
11. SUMMARY / Next sections: five of seven lab topics have no emulator chapter.
12. first-project.md "Run it": first `nros sync` prints internal-issue warnings (issues 1233, 1381); board prints `contract violation: release-jitter-runtime spin measured=10556 declared=10000` (+6:39, +22:26).
13. installation.md "Install the nros CLI": install.sh no longer prints the `export PATH` line the page promises (+2:29).
14. serial-transport.md "Create the PTY pair": needs `socat`, absent from prerequisites (+11:51).

## Filed as (2026-11-05)

| Stop | Issue |
| --- | --- |
| 1, 2, 3 | 2255 (book ahead of the latest release) |
| 4 | 2256, via 2257 |
| 5 | 2257 |
| 6 | 2259 |
| 7 | 2261 |
| 8 | 2260 |
| 9 | 2258 (the drift) and 2263 (the board key) |
| 10 | 2258 |
| 11 | 2269 |
| 12 | 2270 |
| 13 | 2271 |
| 14 | 2272 |
