# Troubleshooting — First 10 Minutes

The Quick Start runs on `--rmw cyclonedds` — no router daemon exists
on that path, so "no output" there is never a router problem; skip
section B's router branches. The zenoh walkthroughs (First Node pages)
additionally assume `nros setup native --rmw zenoh` has run and the
ROS 2 zenoh router (`ros2 run rmw_zenoh_cpp rmw_zenohd`) is reachable. When something goes wrong in the first ten minutes, the
error you see usually points at one of the predictable misses below.

Each branch quotes the **real** stderr you can grep against — not a
paraphrase. If your error text matches, the fix on the right is the
one to try.

## A. Build failures (cargo / cmake)

### A1. You built before `nros sync`

```
… has not been synced — missing the resolved model under `build/nros/models/`,
the generated message crate `std_msgs` (generated/std_msgs).
  Run `nros sync` in … (RFC-0098 D2), then build again.
```

The example's `Cargo.toml` declares nano-ros crates registry-style
(`nros = { version = "*" }` — they are not published to crates.io);
the `[patch.crates-io]` block that resolves them into a nano-ros
checkout is GENERATED, into `build/<image>/nros-cargo.toml`, which
`nros build` hands cargo with `--config` (RFC-0098 D1). This error
means that file has not been written yet, or its relative paths no
longer reach a checkout (typical right after copying the example
somewhere else). Fix:

```bash
cd <the example dir>
NROS_REPO_DIR=/path/to/nano-ros nros sync
```

Driving cargo directly skips that refusal and gets cargo's own version of it
instead — `no matching package named 'nros' found`, or an unresolved import.
Same cause, same fix: sync, then pass the generated file with `--config`.
`NROS_REPO_DIR` is what ties a copied-out example to a checkout, so re-run sync
after moving the directory.

This is **not** an `nros setup` issue — `nros setup` only fetches
the SDK / source-package payload (zenoh-pico, mbedtls, cyclonedds,
…); it does not synthesise missing Cargo dependencies.

### A2. nros codegen tool not found

<!-- no-just-ok: the fence below quotes tool output verbatim -->
```
nros (codegen tool) not found on PATH or in packages/cli/target/release/
or ${NROS_HOME:-~/.nros}/bin. nano-ros assumes `nros` is provided
(the CLI lives in-tree at packages/cli/). Build it with:
  just setup-cli                 # or: just setup
```

(The message names the contributor recipe; `./scripts/bootstrap.sh`
runs the same build without `just`.)

Missing the `nros` binary on PATH **and** in the per-checkout location.
It builds from the in-tree sub-workspace; build it, then
activate the workspace:

```bash
./scripts/bootstrap.sh      # builds packages/cli/target/release/nros
source ./activate.sh        # OR: direnv allow / source ./activate.fish
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
```

The triple is the board's, not the example's — you will not find it in any
file you wrote. Read it back from the `[build] target` line of the generated
`build/<image-id>/nros-cargo.toml`, which `nros sync` writes from the board
your `system.toml` names.

### A4. Cross linker not found

```
error: linker `arm-none-eabi-gcc` not found
```

The cross toolchain wasn't provisioned. Run nros setup for your
board (it ships a prebuilt arm-none-eabi-gcc):

```bash
nros setup mps2-an385-freertos       # or qemu-armv7a-nuttx / mps2-an385-baremetal / …
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
CMake-built target — wired through `CMakeLists.txt` +
Corrosion. Use the cmake build path instead, and choose the backend where
backends are chosen: `[system] rmw` in the leaf's `system.toml`.

```toml
[system]
rmw = "cyclonedds"
```

```bash
cd examples/native/c/talker        # (or cpp)
cmake -B build-cyclone
cmake --build build-cyclone
```

A single-package C / C++ leaf needs no `nros sync` — its message bindings are
a CMake-time output. There is no `-DNROS_RMW=` to pass: a backend selected on
the command line disagrees with the one the rest of the build resolves from
`system.toml`, which is the drift RFC-0098 removes.

The pure `cargo build --features rmw-cyclonedds` only succeeds for
the zenoh-pico + xrce backends today.

### A7. The board name is a typo

```
…/system.toml: board `mps2-an358` is claimed by no board descriptor under /…/nano-ros
```

`[image.<id>] board` is where a leaf names its board, and a typo there is
refused by name rather than guessed at. The legal spellings are the
`names = [...]` arrays of the board descriptors, one per board crate at
`packages/boards/*/nros-board.toml`; several are aliases for the same board
(`qemu-mps2-an385` and `rtic-mps2-an385` are two entry shapes on one
descriptor).

The neighbouring refusals, when the image is missing rather than misspelt:

```
this workspace declares no `[image.*]`. An image is the buildable unit — see RFC-0065 D6.
…/system.toml: names no board (RFC-0098 D3: `[image.<id>] board = "<board>"`)
```

The first means there is no `system.toml` beside the package at all — a leaf
without one is not a leaf `nros build` can plan. The second means the file is
there and its `[image.<id>]` table has no `board` key.

### A8. `direnv allow` reminder

```
NROS_PLATFORM_CFFI_INCLUDE not set (direnv allow, or build via just)
FREERTOS_PORT not set
```

(The quoted message is emitted by the build script; "build via just"
in it refers to the contributor recipes.) The
common build sites autoresolve these from
the in-tree checkout, so a fresh `cargo build` no longer panics on
them in canonical examples. If your custom build site still does,
run `direnv allow` once after clone, or set the env explicitly
(**contributors** can build via the in-tree `just <plat>` recipe).

## B. Binary runs but no output

### B1. Rust: `RMW session open failed — Transport(ConnectionFailed)`

```
[ERROR nros] RMW session open failed — Transport(ConnectionFailed)
```

The zenoh router isn't running, or isn't reachable on the locator the
talker is pointed at. The router comes from ROS 2 (nano-ros no longer
ships one) — start it in another terminal:

```bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd
```

Default ports: `tcp/127.0.0.1:7447` on POSIX,
`tcp/10.0.2.2:7451` on QEMU FreeRTOS (Slirp forwards to host),
`7452` NuttX, `7453` ThreadX-RV, `7454` ESP32, `7455`
ThreadX-Linux, `7447` Zephyr.

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

When a native talker can't reach the daemon, override the locator with
the canonical env var:

```bash
NROS_LOCATOR=tcp/192.168.1.50:7447 ./build/c_talker
# Legacy alias (still accepted): ZENOH_LOCATOR=… ./build/c_talker
ROS_DOMAIN_ID=7 ./build/c_talker         # also overridable
```

The native Rust / C / C++ talkers all read `NROS_LOCATOR` first, fall
back to `ZENOH_LOCATOR`, then to the build-time default. Embedded
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

```
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

Don't loop on the contributor recipe `just workspace cargo-tools` — that re-runs the
build which short-circuits on the same PATH miss.

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

## What success looks like

A correctly-running Rust Linux talker
(`examples/native/rust/talker`) prints something like this on
stderr (with `RUST_LOG=info`):

```text
[INFO  talker] Publishing: 'Hello World: 1'
[INFO  talker] Publishing: 'Hello World: 2'
[INFO  talker] Publishing: 'Hello World: 3'
```

(That is the full expected output — the talker logs one line per
publish, nothing else.)

A correctly-running C talker (`examples/native/c/talker`) prints
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

- [Install + first build](./installation.md) — full setup walkthrough
- [First Node — Rust](./first-node-rust.md) — the canonical Rust starter
- [Troubleshooting](../user-guide/troubleshooting.md) — broader
  issue-by-issue reference for post-first-build problems
