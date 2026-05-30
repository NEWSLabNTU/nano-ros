# Troubleshooting — First 10 Minutes

The Linux starter walkthroughs assume `nros setup native --rmw zenoh` has
run, `zenohd` is reachable, and the right Rust target is installed. When
something goes wrong in the first ten minutes, the error you see usually
points at one of the predictable misses below.

## Decision tree

```
Did `cargo build` / `cmake --build` fail?
├─ error[E0432]: unresolved import `nros`
├─ error: failed to load source for dependency `nros`
├─ error: could not find `nros-rmw-zenoh`
│   → The path-dep at the top of the example's Cargo.toml
│     (`path = "../../../../packages/core/nros"`) doesn't
│     resolve. Either you're outside the nano-ros checkout, or
│     you copied the example to a new dir without updating the
│     relative path. Fix: adjust the `path = "…"` value (or add
│     an empty `[workspace]` table to the copied example and
│     change the path to point at the real nano-ros checkout).
│     If you're INSIDE the nano-ros checkout but the dependency
│     itself (zenoh-pico, mbedtls) is missing from the
│     submodule, run `nros setup native --rmw zenoh`.
│
├─ error: failed to find tool. Is `nros` installed?
├─ error: `nros-codegen` not found
│   → The `nros` CLI is missing on PATH. Reinstall:
│       curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install-nros.sh | sh
│       export PATH="$HOME/.nros/bin:$PATH"
│     The `nros` binary ships the codegen — there is no
│     separate `nros-codegen` build step. CMake examples
│     auto-resolve `nros` on PATH; `-D_NANO_ROS_CODEGEN_TOOL=`
│     is an override, not a requirement.
│
├─ error: could not compile … due to previous error
│  followed by:  the target `thumbv7m-none-eabi` is not installed
│   → `rustup target add thumbv7m-none-eabi`
│     (or whichever target the example's `.cargo/config.toml` names)
│
├─ error: linker `arm-none-eabi-gcc` not found
│   → The cross toolchain wasn't provisioned. Run nros setup for
│     your board (it ships a prebuilt arm-none-eabi-gcc):
│       nros setup qemu-arm-freertos   # or qemu-arm-nuttx / mps2-an385 / …
│
├─ ld: cannot find -lddsc / -lcyclonedds-ddsc
│   → The Cyclone DDS runtime wasn't provisioned:
│       nros setup native --rmw cyclonedds

Did the binary build but not produce output?
├─ Talker panics `panicked … Failed to open session:
│  Transport(ConnectionFailed)` (Rust) or
│  `nros::init -> -3` / `-100` (C / C++)
│   → zenohd isn't reachable. Open another terminal and start
│     it (the `install-nros.sh` script provisions a
│     `~/.nros/bin/zenohd` forwarder that resolves the SDK-
│     store install):
│       zenohd --listen tcp/127.0.0.1:7447
│     Check the locator the example points at matches the
│     port zenohd is listening on (default 7447 for POSIX,
│     per-platform 7450..7456 for the embedded fixtures).
│
├─ binary exits immediately, no error printed
│   → Buffering: `setvbuf(stdout, NULL, _IOLBF, 0)` if you piped
│     the run. POSIX terminals flush on newline; piped stdout
│     full-buffers and may eat short outputs.
│
├─ binary runs but ROS 2 side sees nothing
│   → Mismatched RMW_IMPLEMENTATION. On the ROS 2 side:
│       export RMW_IMPLEMENTATION=rmw_zenoh_cpp     # for Zenoh
│     The default `rmw_fastrtps_cpp` will NOT see nano-ros
│     publishers on the Zenoh backend.

Stuck on something else?
├─ `just <platform> doctor` is the fast scoped variant — prefer
│   it over `just doctor`. E.g. `just freertos doctor` for
│   FreeRTOS / QEMU / arm-none-eabi prints one fixit hint per
│   missing tool and exits in seconds.
├─ `just doctor tier=default` is slower (network calls to
│   verify rustup toolchain pins); use it for a workspace-wide
│   sweep, not for a quick "what's missing".
└─ When all else fails, file an issue with:
    - the exact command you ran,
    - the full stderr,
    - `rustc --version`, `cmake --version`, `qemu-system-arm --version`.
```

## What success looks like

A correctly-running Rust Linux talker (`examples/native/rust/talker`)
prints something like this on stderr (with `RUST_LOG=info`):

```text
[INFO  native_rs_talker] nros Native Talker (Zenoh Transport)
[INFO  native_rs_talker] =========================================
[INFO  native_rs_talker] Published: 0
[INFO  native_rs_talker] Published: 1
[INFO  native_rs_talker] Published: 2
```

A correctly-running C talker (`examples/native/c/talker`)
prints on stdout:

```text
nros C Talker
=================
Locator: tcp/127.0.0.1:7447
Published: 0
Published: 1
Published: 2
```

A correctly-running C++ talker prints the same `Published: N` line
once per second.

The ROS 2 side (`ros2 topic echo /chatter std_msgs/msg/Int32` with
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
output, and matching counter values — interop is verified end-to-end.

## See also

- [Install + first build](./installation.md) — full setup walkthrough
- [First Node — Rust](./first-node-rust.md) — the canonical Rust starter
- [Troubleshooting](../user-guide/troubleshooting.md) — broader
  issue-by-issue reference for post-first-build problems
