# Troubleshooting — First 10 Minutes

<!-- type: reference · level: 1 -->

Every first build of a project needs ROS 2 sourced, because the messages are
generated from it ([A10](#a10-ament_prefix_path-is-unset)). The
[Run a talker on an emulated board](../tutorial/run-a-talker.md) runs a talker in QEMU over Zenoh, so it
also needs ROS 2 for the router; its Linux section runs on Cyclone DDS,
where no router exists, so "no output" there is never a router problem. The
First Node pages run on Linux over Zenoh and assume `nros setup linux --rmw
zenoh` has run and the ROS 2 router (`ros2 run rmw_zenoh_cpp rmw_zenohd`) is
reachable. When something goes wrong in the first ten minutes, the
error you see usually points at one of the predictable misses below.

Each branch quotes the **real** stderr you can grep against — not a
paraphrase. If your error text matches, the fix on the right is the
one to try.

## A. Build failures (cargo / cmake)

### A1. Cargo cannot find `nros`

```text
error: no matching package named `nros` found
```

A node package's `Cargo.toml` declares nano-ros crates registry-style
(`nros = { version = "*" }` — they are not published to crates.io); the
`[patch.crates-io]` block that resolves them into the nano-ros SDK is
GENERATED, under `build/`, and `nros build` hands it to cargo with
`--config`. `nros build` and `nros run` write it themselves, so this error
means cargo ran without it: a plain `cargo build` in a package, or an IDE
that opened the workspace before anything was generated. Fix:

```bash
cd <the workspace root>
nros build        # or, to prepare without building: nros configure
```

`nros configure` writes the generated
settings without building; `nros build --dry-run` prints the cargo line that
uses them. Either is also what ties a copied-out workspace to an SDK (the
`nros` binary's own, or the one `--nano-ros-path` names), and runs again by
itself on the next build after you move the directory.

This is **not** an `nros setup` issue — `nros setup` only fetches
the SDK / source-package payload (zenoh-pico, mbedtls, cyclonedds,
…); it does not synthesise missing Cargo dependencies.

### A2. nros CLI not found

```text
nros (codegen tool) not found on PATH or in packages/cli/target/release/
or ${NROS_HOME:-~/.nros}/bin. …
```

(The rest of the message names how a *contributor* builds the CLI from
a checkout; as a user you install it instead.)

The `nros` binary is not installed, or not on PATH. Install a release
and put `~/.nros/bin` on PATH:

```bash
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh | sh
export PATH="$HOME/.nros/bin:$PATH"
```

If `~/.nros/bin/nros` exists but PATH doesn't see it — that's the
`[PATH]` doctor status (D2 below), not this branch.

The `nros` binary ships the codegen — there is no separate
`nros-codegen` build step. CMake projects auto-resolve `nros` from
PATH / `${NROS_HOME:-~/.nros}/bin/`;
`-D_NANO_ROS_CODEGEN_TOOL=<path>` is an override, not a requirement.

### A3. Rust target not installed

```text
error: could not compile … due to previous error
the target `thumbv7m-none-eabi` is not installed
```

Add it:

```bash
rustup target add thumbv7m-none-eabi
```

The triple is the board's, not the example's — you will not find it in any
file you wrote. Read it back from the `[build] target` line of the generated
`nros-cargo.toml` under `build/`, which `nros build` writes from the board
your bringup's `system.toml` names.

### A4. Cross linker not found

```text
error: linker `arm-none-eabi-gcc` not found
```

The cross toolchain wasn't provisioned. Run nros setup for your
board (it ships a prebuilt arm-none-eabi-gcc):

```bash
nros setup mps2-an385-freertos       # or qemu-armv7a-nuttx / mps2-an385-baremetal / …
```

### A5. Cyclone DDS runtime missing

```text
ld: cannot find -lddsc
ld: cannot find -lcyclonedds-ddsc
```

The Cyclone DDS runtime wasn't provisioned:

```bash
nros setup linux --rmw cyclonedds
```

### A6. `cargo build --features rmw-cyclonedds` can't link

```text
undefined reference to `nros_rmw_cyclonedds_register`
undefined reference to `dds_create_participant`
```

`rmw-cyclonedds` cannot link from cargo alone — the Cyclone backend
is C++ + CMake, registered via `nros_rmw_cffi_register_named` from a
CMake-built target — wired through `CMakeLists.txt` +
Corrosion. Use the cmake build path instead, and choose the backend where
backends are chosen: `[system] rmw` in the bringup's `system.toml`.

```toml
[system]
rmw = "cyclonedds"
```

```bash
cd my_ws                          # a workspace with C or C++ node packages
nros build
```

There is no `-DNROS_RMW=` to pass: a backend selected on
the command line disagrees with the one the rest of the build resolves from
`system.toml`.

A Rust node package does not take `--features rmw-cyclonedds` at all: its RMW
is its image's. A bringup that declares a `linux_cyclonedds`
image (as
[`examples/linux/rust/talker`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/rust/talker)
does) builds it with `nros build linux_cyclonedds` — the board crate's `rmw-cyclonedds` builds Cyclone
through `nros-rmw-cyclonedds-sys`.

### A7. The board name is a typo

```text
error[NROS-0002]: `[image.linux] board = "mps2-an358"` matches no board
  help: check the spelling; the known boards are an536, bare-metal, baremetal, … Out-of-tree boards are added through `$NROS_EXTRA_BOARD_PATH`
  explain: nros explain NROS-0002
```

`[image.<id>] board` is where a bringup names its board, and a typo there is
refused by name rather than guessed at. The legal spellings are the
`names = [...]` arrays of the board descriptors, one per board crate at
`packages/boards/*/nros-board.toml`; several are aliases for the same board
(`mps2-an385-baremetal`, `baremetal` and `bare-metal` all name one
descriptor).

The neighbouring refusals, when the image is missing rather than misspelt:

```text
error[NROS-0003]: this workspace declares no `[image.*]`, and an image is the buildable unit
  help: add `[image.<id>] board = "<board>"` to the bringup package's system.toml
  explain: nros explain NROS-0003
error[NROS-0004]: …/system.toml: `[image.linux]` names no `board`
  help: add `board = "<board>"` to the `[image.<id>]` table in system.toml
  explain: nros explain NROS-0004
```

The first means no bringup in the workspace has a `system.toml` with an
image, so there is nothing for `nros build` to plan. The second means the file is
there and its `[image.<id>]` table has no `board` key.

### A8. `system.toml` next to code

`nros build` refuses a `system.toml` in a directory that also holds a
`Cargo.toml` or a `CMakeLists.txt`. A `system.toml` belongs to a bringup
package, never to the package with the code, because it says where that code
runs and the code itself must not.

To fix it, give the configuration its own package:

1. Make `src/<pkg>_bringup/` beside your node package, with a `package.xml`.
2. Move `system.toml` into it.
3. Delete any `main` that only starts the node; the entry is generated.

`nros new <ws> --board <board> --template talker` writes this shape, so a
scaffolded project never hits it.

### A9. `direnv allow` reminder

```text
NROS_PLATFORM_CFFI_INCLUDE not set (direnv allow, or build via just)
FREERTOS_PORT not set
```

(The quoted message is emitted by the build script; "direnv allow" and
"build via just" in it refer to a contributor checkout.) `nros build`
passes these to cargo from the board's settings, so a build driven by
`nros build` — or by cargo with the generated `--config` file — does not
hit it. If a cargo command you drive yourself does, you skipped the
`--config build/<image>/nros-cargo.toml` argument.

### A10. `AMENT_PREFIX_PATH` is unset

`nros build`, `nros run` or `nros configure` stops before building anything.
The message names the command you ran; from `nros build`:

```text
error[NROS-0001]: `AMENT_PREFIX_PATH` is unset, and `/home/you/sensor_ws` has no earlier configure that recorded one
  help: source ROS 2 in this shell (`source /opt/ros/<distro>/setup.bash`, or your overlay's `install/setup.bash`) and run `nros build` again; later shells need not source it
  explain: nros explain NROS-0001
```

The project has messages to generate (every talker uses `std_msgs`), and
nano-ros generates them from the message definitions of your ROS 2 install,
so the types and their hashes match your ROS 2 peers. This shell has not
sourced ROS 2, and no earlier configure of the project ran in one that had.
It happens on every board and every RMW, Linux and Cyclone DDS included.

Source ROS 2 once, in this shell, and run the same command again:

```bash
source /opt/ros/humble/setup.bash   # or your overlay's install/setup.bash
nros build
```

That configure records the prefix in the project. Later builds and runs of
the same project reuse it, from any shell, an IDE or a CI step. A new
project records its own, so its first build needs a sourced shell again.

### A11. The stack is below the image's floor

```text
nros-stack-floor: ERROR: the OPEN stack is 26624 B, below the measured floor 56368 B. Raise it: NROS_BAREMETAL_STACK_KB=56
```

Every build of an embedded image works out a stack floor for the thread that
opens nano-ros and runs its executor. It walks the compiled code for the
deepest call path, counts every indirect call as able to reach every function
it could reach, and adds a margin. That makes it a heuristic floor: it usually
sits well above the stack the image really uses, because no run takes every
path the walk allows, and it is not a proof that a larger stack never
overflows. An exact search that would tighten it is not done yet.

An image template whose real peak has been observed on a run is measured.
For a measured image, a stack below the floor fails the build, and the
message names the setting to raise and the value that clears it:
`NROS_BAREMETAL_STACK_KB` on bare metal, `NROS_FREERTOS_APP_STACK_KB` on
FreeRTOS, `CONFIG_MAIN_STACK_SIZE` in a Zephyr image's `os_config`. Raise it
to that value and build again. `nros new` writes a stack that holds the floor
into the projects it scaffolds. A FreeRTOS image can instead state
`stack_floor = "raise"` under `[image.<id>]` in `system.toml`, and the build
rebuilds it once at the floor ([`system.toml` reference](system-toml.md)).

For an image nobody has measured, the floor is only printed, as a `WARNING`
that names the same setting. Nothing is refused, and if the image overflows
that setting is the first thing to try.

## B. Binary runs but no output

### B1. Rust: `RMW session open failed — Transport(ConnectionFailed)`

```text
[ERROR nros] RMW session open failed — Transport(ConnectionFailed)
```

The zenoh router isn't running, or isn't reachable on the locator the
talker is pointed at. The router comes from ROS 2 (nano-ros no longer
ships one) — start it in another terminal:

```bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

Default ports: `tcp/127.0.0.1:7447` on POSIX,
`tcp/10.0.2.2:7451` on QEMU FreeRTOS (Slirp forwards to host),
`7452` NuttX, `7453` ThreadX-RV, `7454` ESP32, `7455`
ThreadX-Linux, `7447` Zephyr.

### B2. C: `RCLC_CHECK failed: rclc_support_init(...) -> -4`

```text
RCLC_CHECK failed at src/main.c:92: rclc_support_init(&app.support, argc, argv, &allocator) -> -4
```

Process exits `1` (the `retval` passed to `RCLC_CHECK_RET`).
`-4 = RCLC_RET_NOT_FOUND` — the locator was unreachable (zenohd
not running, or wrong port). Same fix as B1 above.

The C API entry point is `rclc_support_init` (rclc's
`(&support, argc, argv, &allocator)`) or, with an explicit locator,
`rclc_support_init_with_locator` — **not** `nros_init` or `nros::init`,
which don't exist in the C API.

### B3. C++: process exits 156 after a `rclcpp::init_in` failure

```text
nros::init returned NROS_CPP_RET_TRANSPORT_ERROR (-100)
```

Same root cause as B1/B2 — zenohd not reachable.
`NROS_CPP_RET_TRANSPORT_ERROR = -100` is the C++ result code that
`RCLCPP_TRY_RET` propagates from `main()`; on POSIX this becomes
`(unsigned char)-100 = 156` as the process exit code. Treat
"exited 156 after starting" as the C++ equivalent of B1.

### B4. Override the locator at runtime

When a native talker can't reach the daemon, override the locator with
the canonical env var:

```bash
NROS_LOCATOR=tcp/192.168.1.50:7447 ./build/c_talker
ROS_DOMAIN_ID=7 ./build/c_talker         # also overridable
```

The native Rust / C / C++ talkers all read `NROS_LOCATOR`, then fall back
to the build-time default. The retired `ZENOH_LOCATOR` / `ZENOH_MODE` are
REFUSED at startup with an error naming `NROS_LOCATOR` / `NROS_SESSION_MODE`
— unset them. Embedded
targets have no runtime env — their locator is compile-baked from
`system.toml` (`[image.<id>] locator`, or `[system] locator` for an image that
does not state one), so editing that file and rebuilding is the override.

### B5. Binary exits immediately, no error printed

Buffering: `setvbuf(stdout, NULL, _IOLBF, 0)` if you piped the run.
POSIX terminals flush on newline; piped stdout full-buffers and
may eat short outputs. Add a `RUST_LOG=info` (Rust) or unbuffer
the C / C++ output (`stdbuf -oL`).

## C. ROS 2 side sees nothing

### C1. RMW mismatch

```bash
# On the ROS 2 side, default rmw_fastrtps_cpp will NOT see nano-ros:
export RMW_IMPLEMENTATION=rmw_zenoh_cpp     # for Zenoh
export RMW_IMPLEMENTATION=rmw_cyclonedds_cpp # for Cyclone
```

### C2. QoS mismatch — echo silent, list sees the topic

```text
ros2 topic list           # /chatter shown
ros2 topic echo /chatter  # … nothing
```

nano-ros publishers default to `BEST_EFFORT`; stock
`ros2 topic echo` defaults to `RELIABLE`. The QoS-mismatched
subscriber is created but receives no data. Force best-effort on
the echo:

```bash
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

## D. Doctor + last-resort

### D1. Run the doctor first

```bash
nros doctor
```

It prints fixit hints for the toolchain you actually need. (A
just-free *platform-scoped* doctor spelling does not exist today;
contributors with an in-tree checkout can scope via the platform
recipes.)

### D2. `[PATH] nros built but not on PATH`

The doctor reports this distinct from `[MISSING]` when the binary is
installed at `${NROS_HOME:-~/.nros}/bin/nros` but PATH doesn't see it.
Put the directory on PATH (and in your shell's startup file):

```bash
export PATH="$HOME/.nros/bin:$PATH"
```

### D3. Full sweep (slow)

```bash
nros doctor
nros setup --check
```

Only run this when you're standing up every supported platform in
one go. It walks the provisioning checks and can take a few
minutes.

### D4. File an issue

When all else fails, include:

- the exact command you ran,
- the full stderr,
- `rustc --version`, `cmake --version`, `qemu-system-arm --version`,
- `nros --version`.

## E. zenoh-pico error codes

| Code | Name | Meaning |
| --- | --- | --- |
| -102 | `_Z_ERR_TRANSPORT_OPEN_FAILED` | could not connect to the router |
| -73 | `_Z_ERR_SESSION_CLOSED` | the session closed after an earlier failure |
| -78 | `_Z_ERR_SYSTEM_OUT_OF_MEMORY` | an allocation did not fit |
| -100 | `_Z_ERR_TRANSPORT_TX_FAILED` | a transmission failed |
| -128 | `_Z_ERR_GENERIC` | a generic error |

The full list is zenoh-pico's `include/zenoh-pico/utils/result.h`. Fixes by
symptom are in [Troubleshoot](../guide/deploy/troubleshoot.md).

## What success looks like

A correctly-running Rust Linux talker
([`examples/linux/rust/talker`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/rust/talker)) prints something like this on
stderr (with `RUST_LOG=info`):

```text
[INFO  talker] Publishing: 'Hello World: 1'
[INFO  talker] Publishing: 'Hello World: 2'
[INFO  talker] Publishing: 'Hello World: 3'
```

(That is the full expected output — the talker logs one line per
publish, nothing else.)

A correctly-running C talker ([`examples/linux/c/talker`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/c/talker)) prints
on stdout:

```text
Publishing: 'Hello World: 1'
Publishing: 'Hello World: 2'
Publishing: 'Hello World: 3'
```

A correctly-running C++ talker prints the same
`Publishing: 'Hello World: N'` line once per second.

The ROS 2 side (`ros2 topic echo /chatter std_msgs/msg/String
--qos-reliability best_effort` with
`RMW_IMPLEMENTATION=rmw_zenoh_cpp`) should see:

```text
data: 'Hello World: 1'
---
data: 'Hello World: 2'
---
data: 'Hello World: 3'
---
```

If you see all three of these — talker logging, ROS 2 echo
output, and matching counter values — interop is verified
end-to-end.

## See also

- [Install + first build](../start/install.md) — full setup walkthrough
- [Add a second node](../tutorial/add-a-second-node.md) — the canonical Rust starter
- [Troubleshooting](../guide/deploy/troubleshoot.md) — broader
  issue-by-issue reference for post-first-build problems
