# Adoption probe, round 2: education

- **Host:** docker `ubuntu:24.04` (seat 1 `probe2-edu`, seat 2 `probe2-edu-seat2`), non-root user `dev` with passwordless sudo, x86_64. ROS 2 Jazzy (`ros-jazzy-ros-base` 0.11.0, `rmw_zenoh_cpp` 0.2.11) installed per docs.ros.org as the book directs.
- **Release:** `install.sh` installed **0.6.0-nros7** (`nros version` prints `nros 0.6.0`, codegen version 21, play_launch 8d2c78d4, ros-launch-manifest v0.3.12; `nros pin` reports nano-ros 538ccb9, index 2026-11-06).
- **Start time:** 2026-11-07T01:47:40Z (seat 1 `docker run`). Every time below is measured from that point unless it says otherwise.
- **Source used:** the book sources (scratchpad copy of `book/src`) only. I opened no file in the repository and followed no link into GitHub.

## Verdicts

| # | Task | Verdict | Time against budget |
|---|---|---|---|
| 1 | Install a seat, first QEMU output | **done with stops** | First QEMU output at 01:56:35, **8 m 55 s** against the 15-minute budget, including 2 m 28 s for the ROS 2 apt install. The default scaffold failed to build (stop 3). I got past it in about 30 s from the book's knob page; a student might not find that page. The chapter's "See it in ROS 2" half never worked (stop 5). |
| 2 | Plan a semester's labs | **done with stops** | About 10 min of reading. One of the seven topics (pub/sub) has a chapter that runs on an emulator, and its ROS 2 half is broken. Lifecycle has no chapter at all. The rest are taught only on the Linux host, or not on an emulator. See the coverage table. |
| 3 | Grading line for one lab | **done** | `nros run <pkg> --expect "Publishing: 'Hello World: 1'" --timeout 120` exits 0 when the line appears (3.2 s after boot). With text that never appears it exits 1 (checked with a 20 s timeout). |
| 4 | Pin the semester, second seat | **done with stops** | Pin is `nros-toolchain.toml` `[toolchain] version = "0.6.0-nros7"` (Install options › Pinning a version). Seat 2 went from `docker run` (about 02:07:00) to a matched first output at 02:14:39, **about 7 m 40 s**. The console output of both seats is identical once the boot timestamps are stripped. Stop 3 hit seat 2 the same way, so the same hand edit went to every seat. ROS 2 apt packages are not pinned by anything the book describes (see notes). |
| 5 | First error a student would search | **done** | The first error was stop 3's `NROS_EXECUTOR_MAX_CBS` panic. Its message does **not** tell a student what to type next (see below). |

### Task 2: lab coverage

| Lab topic | Chapter / example that teaches it | On an emulator? |
|---|---|---|
| Publish and subscribe | Tutorial › First Project (talker, `mps2-an385-baremetal` in QEMU); listener in First Node (Rust/C/C++) and Project layout §1 (`--template listener`) | Talker: **yes**. Listener: Linux host only (`--board linux`). No emulated pub→sub pair is walked through. |
| QoS | User guide › Use QoS, status events and discovery; Contracts › Declare QoS in a contract | **No**. Neither page names an emulated board. |
| A service | Choose your path step 7 → First Node — C, "`--template server` / `client` (AddTwoInts)"; Project layout › "Calling a service from a node" | **No**. Linux only. Only one sentence names the templates; no page builds and runs them. |
| Parameters | Tutorial › Project layout §3 "Parameters" (launch `<param>`, `ros2 param` services) | **No** board-specific steps, and no emulator run. |
| Lifecycle | **No chapter.** Mentioned only in sizing tables, the C/C++ API reference and the board breadth rows ("Lifecycle" witnessed on linux and zephyr native_sim). | **No chapter.** |
| Real-time tasks and timing | User guide › Real-time › Pick a scheduling model; Analyze timing | **No**. Analyze timing says "Timing measured in QEMU is meaningless … Measure execution times on the real board", so this lab needs hardware. |
| Talking to a ROS 2 graph on the host | Tutorial › First Project "See it in ROS 2" (QEMU); Tutorial › ROS 2 Interoperability (Linux host) | QEMU path: **yes, but it delivered nothing** (stop 5). The interop chapter itself is Linux-host only. |

### Task 3: grading line

The book promises (First Project › Run, the `probe-expect` block):

```text
Publishing: 'Hello World: 1'
nros run: matched after
```

The real console line carries a log prefix,
`[INFO] sensor_node: [    1.026219] Publishing: 'Hello World: 1'`. `--expect` matches substrings, so it still matches.

Per-seat grading command, run in the student's workspace with ROS 2 sourced:

```sh
nros run sensor_node --expect "Publishing: 'Hello World: 1'" --timeout 120; echo "grade rc=$?"
```

- Pass: rc 0, `nros run: matched after 3.2 s: … Publishing: 'Hello World: 1'`.
- Fail (text absent): rc 1, after the timeout (checked with `--expect NO_SUCH_TEXT --timeout 20`).
- A build failure exits 2 and an unknown package name exits 3. The chapter documents only 0 and 1; CLI reference line 708 covers `--timeout`. Any nonzero exit still grades as a fail, so the grading line works.
- The First Project chapter itself uses `--timeout` only in the Linux variant. For the QEMU variant you find it in the CLI reference.

### Task 5: does the first error tell a student what to type?

The error:

```text
error[E0080]: evaluation panicked: nros::main!: the SystemModel registers 1 callback entities but the executor callback table compiled in is smaller (board `mps2-an385-baremetal` ignores per-entry sizing). Raise `NROS_EXECUTOR_MAX_CBS` (e.g. to 3) in the entry's `.cargo/config.toml` `[env]` and rebuild from clean — resizing the executor arena mixes stale objects. issue 0257
```

**No.** The message names a variable, but it sends the student to a file that does not exist. The "entry" is the generated `build/bare-metal-zenoh/baremetal_entry/`, which has no `.cargo/` directory. The book's own place for this knob is `system.toml`
`[image.baremetal.knobs.executor] max_cbs = 3` (Configure an image › Per-image build knobs). The message also says "rebuild from clean" without naming a command. It uses "SystemModel", "entry" and "issue 0257", none of which a student has met. A pasted search for "NROS_EXECUTOR_MAX_CBS" brings back the env-var reference table (default `4`, which even contradicts "table is smaller than 1"). The build log printed just before the error suggests `re-run nros configure`, and that answers "up to date" and changes nothing.

## Stops

1. **Install › Before you start (ROS 2).**
   - Step: the page sends the reader to docs.ros.org to add the ROS 2 apt repository, then `sudo apt install ros-jazzy-ros-base ros-jazzy-rmw-zenoh-cpp`.
   - Output: ROS's documented release lookup `curl -s https://api.github.com/repos/ros-infrastructure/ros-apt-source/releases/latest` returned `{"message":"API rate limit exceeded for 140.112.28.133 …}`, so `dpkg -i` failed with `unexpected end of file in archive magic version number`.
   - Time: +1 m 39 s.
   - Workaround: read the version (1.3.0) off the `releases/latest` redirect and downloaded `ros2-apt-source_1.3.0.noble_all.deb` directly. The ROS step took 2 m 28 s.
   - Why it matters: this leaves the book, and in a lab where 30 seats share one NAT address the GitHub API limit is a realistic failure.
   - Suggested issue: "Install: inline the ROS 2 apt-source steps (or a lab-image recipe) instead of sending readers to docs.ros.org".

2. **Choose your path › Design background; First Project › Scaffold and › Design background** (also Project layout §`src/lib.rs`).
   - Text: "The three purposes and their budgets are RFC-0116 D1", plus links to `github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/…` and to `examples/…` in the repository.
   - Time: +0 (reading).
   - Workaround: did not follow them.
   - Why it matters: the book sends the reader to the repository and to design documents.
   - Suggested issue: "Book: Education-route pages link into the repo/RFCs; keep the reader in the book".

3. **First Project › Run.**
   - Command: `source /opt/ros/humble/setup.bash` (jazzy here), then `nros run sensor_node --expect 'Publishing:'`. It failed on a fresh default scaffold, on both seats, on both ROS editions.
   - Decisive line: `error[E0080]: evaluation panicked: nros::main!: the SystemModel registers 1 callback entities but the executor callback table compiled in is smaller (board 'mps2-an385-baremetal' ignores per-entry sizing). Raise 'NROS_EXECUTOR_MAX_CBS' (e.g. to 3) in the entry's '.cargo/config.toml' '[env]' …`. Exit 2. The log also shows `resolved … (max_cbs 0, nodes 1 …)` and `metadata/sensor_node.json: stale against its sources (re-run 'nros configure')`.
   - Time: +8 m 23 s (01:56:03; the first build took 82 s).
   - Workaround: `nros configure` (as the log advises) printed `configure: up to date` and the build failed the same way. I then appended `[image.baremetal.knobs.executor]` / `max_cbs = 3` to `src/sensor_node_bringup/system.toml` (from Configure an image), and the next `nros run` matched in 3.3 s. I did not need a clean rebuild.
   - Suggested issue: "First Project: default talker scaffold for mps2-an385-baremetal fails to compile (max_cbs derived 0; NROS_EXECUTOR_MAX_CBS panic)".

4. **First Project › Scaffold / Run (order of steps).**
   - Command: the chapter runs `nros new sensor_ws --board mps2-an385-baremetal --template talker` before it sources ROS 2.
   - Decisive line: `ROS 2 edition: humble (the default; source a ROS 2 setup.bash or pass --ros-edition to pick another)`, and `system.toml` got `ros_edition = "humble"` on a Jazzy host. Install says Jazzy is *the* edition on 24.04, and ROS 2 Interoperability says "a Humble-edition image against a Jazzy bus … discovers nothing".
   - Time: +8 m 00 s (seen in scaffold output, understood at +13 m 52 s).
   - Workaround: sourced ROS before `nros new` (a fresh scaffold picked `jazzy (the sourced ROS 2, $ROS_DISTRO)`). Editing `ros_edition` in place did not change the "type hash INVALID" symptom of stop 5.
   - Suggested issue: "First Project: scaffold before sourcing ROS silently picks humble on a Jazzy host; source first or pass --ros-edition".

5. **First Project › See it in ROS 2.**
   - Command: with `nros run sensor_node` streaming `Publishing: 'Hello World: N'`, in a second shell `source /opt/ros/jazzy/setup.bash; export RMW_IMPLEMENTATION=rmw_zenoh_cpp; ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort`.
   - Decisive output: nothing printed in 15 to 20 s, on the humble-scaffolded workspace and again on a fresh jazzy-scaffolded one. `ros2 topic list` shows `/chatter`. `ros2 topic info -v /chatter --no-daemon` shows `Topic type hash: INVALID` and `Reliability: RELIABLE`, although the chapter says "The talker publishes best-effort". A plain reliable echo also received nothing. Control: a native `ros2 topic pub /chatter3` is echoed fine by the same command (its hash is `RIHS01_df66…`).
   - Time: +13 m 52 s, re-checked at +17 m 50 s.
   - Workaround: none found in the book (the Troubleshooting index advice, best-effort, was already applied).
   - Why it matters: the "talking to a ROS 2 graph" lab has no working emulator path.
   - Suggested issue: "First Project: QEMU talker visible in `ros2 topic list` but `ros2 topic echo` (Jazzy, rmw_zenoh) receives nothing; type hash INVALID".

6. **First Project › Run (build output vocabulary).**
   - Command: `nros run sensor_node`.
   - Decisive lines: every build prints undefined internal words a course would have to teach or tell students to ignore: `entity-census take …: no contract`, `census check: no contract`, `take buffer not derived … (RFC-0033)`, `sizing descriptor →`, `registration observed for 0 of 0 subscription row(s), from 0 probe sidecar(s)`, `[dangling-entity] warning`, plus the stale-metadata hint that `nros configure` contradicts.
   - Time: +8 m 23 s.
   - Workaround: ignored them.
   - Suggested issue: "nros run: default build output is full of internal terms (census, sizing descriptor, probe sidecar, RFC numbers) on a clean first project".

7. **Install › 4. Provision your board.**
   - Command: `nros setup mps2-an385-baremetal --rmw zenoh` run in `~`.
   - Decisive line: `nros setup: mps2-an385-baremetal ready; locked in /home/dev/nros-sdk.lock`. The book promises `locked in /home/you/.nros/…`, but the file landed in the current directory, and no page explains `nros-sdk.lock`. As a result the two seats' locks differ: seat 1's workspace lock has an empty `[tool]` because the tools were recorded in `~/nros-sdk.lock`.
   - Time: +6 m 55 s.
   - Workaround: none needed for the run. For the pin I ran setup inside the pinned course directory on seat 2.
   - Suggested issue: "nros setup writes nros-sdk.lock into the cwd; book says ~/.nros and never explains the lock".

8. **Install › 3. Check the install / Pinning a version.**
   - Command: `nros version`.
   - Decisive line: `nros 0.6.0`, while the installer said `installed 0.6.0-nros7` and the pin must say `version = "0.6.0-nros7"`. An instructor checking seats with `nros version` cannot see which `-nrosN` release a seat has; `nros pin` shows it.
   - Time: +5 m 00 s.
   - Workaround: used `nros pin` to verify seats.
   - Suggested issue: "nros version omits the -nrosN release suffix the pin and installer use".

### Notes (not counted as stops)

- **Pinning ROS 2.** The pin fixes nano-ros and, through its index, the cross compiler, QEMU and zenoh-pico. It does not fix the ROS 2 apt packages that generate the messages and provide the router. `nros-sdk.lock` records them (`std_msgs 5.3.8`, `rmw_zenoh_cpp 0.2.11`), but the book does not say what happens when a week-10 seat has newer ones. For a lab image, Install options › Working offline is the closest recipe (download the debs in `ros:jazzy-ros-base` and carry the stores).
- **Firmware hashes differ between seats** (`9a55075b…` vs `22d8139a…`) because the workspaces sit at different paths (`~/sensor_ws2` vs `~/course/sensor_ws2`; debug build). The console output was identical.
- **Workspace name changes package names.** `nros new sensor_ws2` creates `sensor_ws2_node` and `sensor_ws2_node_bringup`. The chapter's `nros run sensor_node` only works if the workspace is literally named `sensor_ws`. The error (`no image or node 'sensor_node' … Declared: sensor_ws2_node_bringup:baremetal`, exit 3) does name the right spelling. Graders must fix the workspace name.
- The book's Education card states the course budget and route. That was useful; it is what made this plan possible in 10 minutes.

## Appendix: run log

All times are UTC on 2026-11-07. +mm:ss is measured from T0 = 01:47:40.

| Time | +elapsed | Command (seat) | Decisive output |
|---|---|---|---|
| 01:46:30 | – | read brief, stops, SUMMARY.md, Choose your path (Education card), Install, First Project | Education route: Install → First Project → Anatomy → second node → RMW → own message → services (C templates) → params/launch → board page |
| 01:47:40 | 0:00 | `docker run -d --name probe2-edu ubuntu:24.04`; add user `dev` (seat 1) | ready 01:47:53 |
| 01:48 | 0:20 | ROS 2 apt-source steps from docs.ros.org | GitHub API `rate limit exceeded`; `dpkg-deb: error: unexpected end of file` (stop 1) |
| 01:49:19 | 1:39 | retry with version 1.3.0 from the redirect; `apt install ros-jazzy-ros-base ros-jazzy-rmw-zenoh-cpp` | 2 m 28 s; `Setting up ros-jazzy-ros-base (0.11.0-1noble…)` at 01:52:03 |
| 01:52:03 | 4:23 | Install step 1 apt line | rc 0 |
| 01:52:30 | 4:50 | `curl … install.sh \| sh`; `export PATH=…`; `nros version` | `installed 0.6.0-nros7`; `nros 0.6.0`, `codegen version 21` (stop 8) |
| 01:52:45 | 5:05 | `nros setup mps2-an385-baremetal --rmw zenoh` (in ~) | 1 m 50 s; `ready; locked in /home/dev/nros-sdk.lock` (stop 7); corrosion built from source |
| 01:54:40 | 7:00 | `nros new sensor_ws --board mps2-an385-baremetal --template talker` | `ROS 2 edition: humble (the default; …)` (stop 4) |
| 01:54:41 | 7:01 | `source /opt/ros/jazzy/setup.bash; nros run sensor_node --expect 'Publishing:'` | 82 s, rc 2, `NROS_EXECUTOR_MAX_CBS` panic (stop 3); internal-term build output (stop 6) |
| 01:56:05 | 8:25 | `nros configure`; `nros run … --expect` | `configure: up to date`; same panic, rc 2 |
| 01:56:20 | 8:40 | append `[image.baremetal.knobs.executor] max_cbs = 3` to system.toml; `nros run … --expect 'Publishing:'` | rc 0, `matched after 3.3 s: [INFO] sensor_node: [ 1.026219] Publishing: 'Hello World: 1'` |
| **01:56:35** | **8:55** | **first QEMU output (task 1 time)** | |
| 01:57–02:01 | 9:20–13:52 | `nros run sensor_node` (background) plus `ros2 topic echo /chatter … --qos-reliability best_effort` | no messages; `ros2 topic list` shows `/chatter`; `topic info -v`: `Topic type hash: INVALID`, `Reliability: RELIABLE` (stop 5) |
| 02:01:40 | 14:00 | `sed ros_edition humble→jazzy`; `nros run --expect` | rc 0, but the echo is still empty and the hash is still INVALID |
| 02:02:40 | 15:00 | control: `ros2 topic pub /chatter2` plus `topic info` | `RIHS01_df668c74…` (native has a hash) |
| 02:03:05 | 15:25 | source jazzy, then `nros new sensor_ws2 …`; `nros run sensor_node` | `ROS 2 edition: jazzy (the sourced ROS 2, $ROS_DISTRO)`; rc 3, `no image or node 'sensor_node' … Declared: sensor_ws2_node_bringup:baremetal` |
| 02:03:45 | 16:05 | `nros run sensor_ws2_node --expect 'Publishing:'` | rc 2, same `NROS_EXECUTOR_MAX_CBS` panic (stop 3 reproduces on the jazzy scaffold) |
| 02:04:25 | 16:45 | knob added; `nros run` OK; echo for 20 s | nothing; hash INVALID |
| 02:05:30 | 17:50 | control echo of native `/chatter3` with the same harness | `data: x` received; board `/chatter` echo for 15 s is empty |
| 02:06:00 | 18:20 | `cat nros-toolchain.toml`; `nros pin` | written by first build: `version = "0.6.0-nros7"`; `pinned: 0.6.0-nros7 … codegen 21 index 2026-11-06 nano-ros 538ccb9` |
| 02:06:10 | 18:30 | `nros run sensor_ws2_node --expect NO_SUCH_TEXT --timeout 20` | rc 1 |
| 02:06:27 | 18:47 | `nros run sensor_ws2_node --expect "Publishing: 'Hello World: 1'" --timeout 120` | rc 0, `matched after 3.2 s` |
| ~02:07:00 | seat 2 0:00 | `docker run -d --name probe2-edu-seat2 ubuntu:24.04`; user `dev`; ROS (1.3.0 deb direct); Install step 1; `install.sh \| sh -s -- --version 0.6.0-nros7` | `installed 0.6.0-nros7`; `nros 0.6.0` at 02:11:25 |
| 02:11:30 | seat 2 4:30 | `mkdir ~/course`, write `nros-toolchain.toml` `[toolchain] version = "0.6.0-nros7"`; `nros pin`; `nros setup mps2-an385-baremetal --rmw zenoh` | `pinned: 0.6.0-nros7 (/home/dev/course/nros-toolchain.toml)`; `ready; locked in /home/dev/course/nros-sdk.lock` |
| 02:13 | seat 2 ~6:00 | source jazzy; `nros new sensor_ws2 …`; add the max_cbs knob; `nros run sensor_ws2_node --expect "Publishing: 'Hello World: 1'" --timeout 600` | `pinned: 0.6.0-nros7` inside the workspace; rc 0, `matched after 3.2 s … Publishing: 'Hello World: 1'` at 02:14:39 (seat 2 ≈ 7 m 40 s) |
| 02:15:00 | 27:20 | diff both seats' console.log (timestamps stripped); diff nros-sdk.lock; sha256 of ELF | console **identical**; locks differ (cwd-dependent `[tool]`, path keys); ELF hashes differ (path in debug info) |
| 02:15:17 | 27:37 | CLI reference `--expect` / `--timeout` | "exit 0 as soon as a console line contains TEXT; exit 1 if the target ends, or `--timeout` passes, first" |
| 02:15:28 | 27:48 | `docker rm -f probe2-edu probe2-edu-seat2` | both removed; `docker ps -a` shows none |
