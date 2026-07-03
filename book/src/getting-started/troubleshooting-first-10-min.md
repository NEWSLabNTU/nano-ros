# Troubleshooting — First 10 Minutes

The Linux starter walkthroughs assume `nros setup native --rmw zenoh`
has run, `zenohd` is reachable, and the right Rust target is
installed. When something goes wrong in the first ten minutes, the
error you see usually points at one of the predictable misses below.

Each branch quotes the **real** stderr you can grep against — not a
paraphrase. If your error text matches, the fix on the right is the
one to try.

## A. Build failures (cargo / cmake)

### A1. nros crates don't resolve (missing / stale patch block)

```
error[E0432]: unresolved import `nros`
error: failed to load source for dependency `nros`
error: no matching package named `nros` found
```

The example's `Cargo.toml` declares nano-ros crates registry-style
(`nros = { version = "*" }` — they are not published to crates.io);
the example's `.cargo/config.toml` carries the `# nros-managed`
`[patch.crates-io]` block that resolves them into a nano-ros
checkout. This error means the patch block is missing or its
relative paths no longer reach a checkout (typical right after
copying the example somewhere else). Fix:

```bash
cd <the example dir>
NROS_REPO_DIR=/path/to/nano-ros nros sync
```

which regenerates the message crates and rewrites the patch block
for the example's current location.

This is **not** an `nros setup` issue — `nros setup` only fetches
the SDK / source-package payload (zenoh-pico, mbedtls, cyclonedds,
…); it does not synthesise missing Cargo dependencies.

### A2. nros codegen tool not found

```
nros (codegen tool) not found on PATH or in packages/cli/target/release/
or ${NROS_HOME:-~/.nros}/bin. nano-ros assumes `nros` is provided
(Phase 218 carries the CLI in-tree at packages/cli/). Build it with:
  just setup-cli                 # or: just setup
```

Missing the `nros` binary on PATH **and** in the per-checkout location.
Phase 218 builds it from the in-tree sub-workspace; first activate the
workspace, then build:

```bash
source ./activate.sh        # OR: direnv allow / source ./activate.fish
just setup-cli              # builds packages/cli/target/release/nros
```

If `packages/cli/target/release/nros` exists but PATH doesn't see it —
that's the `[PATH]` doctor status (D2 below), not this branch (the
activate file is what puts it on PATH).

The `nros` binary ships the codegen — there is no separate
`nros-codegen` build step. CMake examples auto-resolve `nros` from
PATH / `packages/cli/target/release/` / `${NROS_HOME:-~/.nros}/bin/`;
`-D_NANO_ROS_CODEGEN_TOOL=<path>` is an override, not a requirement.

### A3. Rust target not installed

```
error: could not compile … due to previous error
the target `thumbv7m-none-eabi` is not installed
```

Add it:

```bash
rustup target add thumbv7m-none-eabi
# or whichever target the example's `.cargo/config.toml` names
```

### A4. Cross linker not found

```
error: linker `arm-none-eabi-gcc` not found
```

The cross toolchain wasn't provisioned. Run nros setup for your
board (it ships a prebuilt arm-none-eabi-gcc):

```bash
nros setup qemu-arm-freertos       # or qemu-arm-nuttx / mps2-an385 / …
```

### A5. Cyclone DDS runtime missing

```
ld: cannot find -lddsc
ld: cannot find -lcyclonedds-ddsc
```

The Cyclone DDS runtime wasn't provisioned:

```bash
nros setup native --rmw cyclonedds
```

### A6. `cargo build --features rmw-cyclonedds` can't link

```
undefined reference to `nros_rmw_cyclonedds_register`
undefined reference to `dds_create_participant`
```

`rmw-cyclonedds` cannot link from cargo alone — the Cyclone backend
is C++ + CMake, registered via `nros_rmw_cffi_register` from a
CMake-built target. Phase 175 wired this through `CMakeLists.txt` +
Corrosion. Use the cmake build path instead:

```bash
cd examples/native/c/talker        # (or cpp / rust)
cmake -B build-cyclone -DNROS_RMW=cyclonedds
cmake --build build-cyclone
```

The pure `cargo build --features rmw-cyclonedds` only succeeds for
the zenoh-pico + xrce backends today.

### A7. "current package believes it's in a workspace"

```
error: current package believes it's in a workspace when it's not:
current:   …/Cargo.toml
workspace: /…/nano-ros/Cargo.toml
```

cargo walks up the directory tree looking for a workspace root and
adopts the example into the outer nano-ros workspace. Per-example
`Cargo.toml`s don't ship an empty `[workspace]` table yet (tracked
as F1 in `phase-208-followups.md`).

Hits on:

- nested clones / worktrees of nano-ros that share an ancestor path
  with the outer `nano-ros/Cargo.toml`;
- a user vendoring an example into their *own* workspace.

Workaround on a regular clone: build from the nano-ros root, e.g.
`cargo build -p qemu-bsp-talker`, instead of `cd`'ing into the
example dir.

### A8. `direnv allow` reminder

```
NROS_PLATFORM_CFFI_INCLUDE not set (direnv allow, or build via just)
FREERTOS_PORT not set
```

Phase 208.D.1 made the common build sites autoresolve these from
the in-tree checkout, so a fresh `cargo build` no longer panics on
them in canonical examples. If your custom build site still does,
run `direnv allow` once after clone, or set the env explicitly /
build via the `just <plat>` recipe.

## B. Binary runs but no output

### B1. Rust: `Failed to open session: Transport(ConnectionFailed)`

```
thread 'main' panicked at examples/native/rust/talker/src/lib.rs:96:58:
Failed to open session: Transport(ConnectionFailed)
```

`zenohd` isn't running, or isn't reachable on the locator the
talker is pointed at. Start it in another terminal (`nros setup native
--rmw zenoh` lands `zenohd` under `${NROS_HOME:-~/.nros}/sdk/zenohd/`;
the activate file puts it on PATH):

```bash
zenohd --listen tcp/127.0.0.1:7447
```

Default ports: `tcp/127.0.0.1:7447` on POSIX,
`tcp/10.0.2.2:7451` on QEMU FreeRTOS (Slirp forwards to host),
`7452` NuttX, `7453` ThreadX-RV, `7454` ESP32, `7455`
ThreadX-Linux, `7456` Zephyr.

### B2. C: `NROS_CHECK failed: nros_support_init(...) -> -4`

```
NROS_CHECK failed at src/main.c:152: nros_support_init(&app.support, locator, domain_id) -> -4
```

Process exits `1` (the `retval` passed to `NROS_CHECK_RET`).
`-4 = NROS_RET_NOT_FOUND` — the locator was unreachable (zenohd
not running, or wrong port). Same fix as B1 above.

The C API entry point is `nros_support_init`, **not** `nros_init`
or `nros::init` — those don't exist in the C API.

### B3. C++: process exits 156 after a `nros::init` failure

```
nros::init returned NROS_CPP_RET_TRANSPORT_ERROR (-100)
```

Same root cause as B1/B2 — zenohd not reachable.
`NROS_CPP_RET_TRANSPORT_ERROR = -100` is the C++ result code that
`NROS_TRY_RET` propagates from `main()`; on POSIX this becomes
`(unsigned char)-100 = 156` as the process exit code. Treat
"exited 156 after starting" as the C++ equivalent of B1.

### B4. Override the locator at runtime

When the talker can't reach the daemon and you don't want to edit
`nros.toml`, override the locator with the canonical env var:

```bash
NROS_LOCATOR=tcp/192.168.1.50:7447 ./build/c_talker
# Legacy alias (still accepted): ZENOH_LOCATOR=… ./build/c_talker
ROS_DOMAIN_ID=7 ./build/c_talker         # also overridable
```

The Rust / C / C++ talkers all read `NROS_LOCATOR` first, fall back
to `ZENOH_LOCATOR`, then to `nros.toml`, then to the build-time
default.

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

```
ros2 topic list           # /chatter shown
ros2 topic echo /chatter  # … nothing
```

nano-ros publishers default to `BEST_EFFORT`; stock
`ros2 topic echo` defaults to `RELIABLE`. The QoS-mismatched
subscriber is created but receives no data. Force best-effort on
the echo:

```bash
ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
```

## D. Doctor + last-resort

### D1. Use the per-platform doctor first

```bash
just freertos doctor       # FreeRTOS / QEMU / arm-none-eabi
just nuttx doctor          # NuttX
just zephyr doctor         # Zephyr
just threadx_linux doctor  # ThreadX-Linux
# etc.
```

Each scoped doctor is fast and prints the same fixit hints for
the toolchain you actually need.

### D2. `[PATH] nros built but not on PATH`

The doctor now reports this distinct from `[MISSING]` when the
binary is built at `packages/cli/target/release/nros` (or the
transitional `${NROS_HOME:-~/.nros}/bin/nros`) but PATH doesn't see
it. Activate the workspace — it wires PATH:

```bash
source ./activate.sh        # bash / zsh
# OR
source ./activate.fish      # fish
# OR
direnv allow                # auto-activates on `cd nano-ros`
```

Don't loop on `just workspace cargo-tools` — that re-runs the
build which short-circuits on the same PATH miss.

### D3. Full sweep (slow)

```bash
just doctor tier=default
```

Only run this when you're standing up every supported platform in
one go. It walks every per-platform doctor and can take a few
minutes.

### D4. File an issue

When all else fails, include:

- the exact command you ran,
- the full stderr,
- `rustc --version`, `cmake --version`, `qemu-system-arm --version`,
- `nros --version`.

## What success looks like

A correctly-running Rust Linux talker
(`examples/native/rust/talker`) prints something like this on
stderr (with `RUST_LOG=info`):

```text
[INFO  native_rs_talker] nros Native Talker (Zenoh Transport)
[INFO  native_rs_talker] =========================================
[INFO  native_rs_talker] Node created: talker
[INFO  native_rs_talker] Publisher created for topic: /chatter
[INFO  native_rs_talker] Published: 0
[INFO  native_rs_talker] Published: 1
[INFO  native_rs_talker] Published: 2
```

A correctly-running C talker (`examples/native/c/talker`) prints
on stdout:

```text
nros C Talker
=================
Published: 0
Published: 1
Published: 2
```

A correctly-running C++ talker prints the same `Published: N`
line once per second.

The ROS 2 side (`ros2 topic echo /chatter std_msgs/msg/Int32
--qos-reliability best_effort` with
`RMW_IMPLEMENTATION=rmw_zenoh_cpp`) should see:

```text
data: 0
---
data: 1
---
data: 2
---
```

If you see all three of these — talker logging, ROS 2 echo
output, and matching counter values — interop is verified
end-to-end.

## See also

- [Install + first build](./installation.md) — full setup walkthrough
- [First Node — Rust](./first-node-rust.md) — the canonical Rust starter
- [Troubleshooting](../user-guide/troubleshooting.md) — broader
  issue-by-issue reference for post-first-build problems
