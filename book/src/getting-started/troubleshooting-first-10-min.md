# Troubleshooting — First 10 Minutes

The Linux starter walkthroughs assume `just setup base` has run, `zenohd`
is reachable, and the right Rust target is installed. When something
goes wrong in the first ten minutes, the error you see usually
points at one of the predictable misses below.

## Decision tree

```
Did `cargo build` / `cmake --build` fail?
├─ error[E0432]: unresolved import `nros`
├─ error: failed to load source for dependency `nros`
├─ error: could not find `nros-rmw-zenoh`
│   → Run `just setup base` from the repo root. The
│     path-dep in the example's Cargo.toml points at the
│     in-tree `packages/core/nros`; if the SDK fetch didn't
│     run, transitive deps are missing.
│
├─ error: failed to find tool. Is `nros` installed?
├─ error: `nros-codegen` not found
│   → Build the host codegen tool first:
│       cargo build --release \
│         --manifest-path packages/codegen/packages/Cargo.toml \
│         -p nros-codegen-c --bin nros-codegen
│     CMake examples pass it via `-D_NANO_ROS_CODEGEN_TOOL=…`.
│
├─ error: could not compile … due to previous error
│  followed by:  the target `thumbv7m-none-eabi` is not installed
│   → `rustup target add thumbv7m-none-eabi`
│     (or whichever target the example's `.cargo/config.toml` names)
│
├─ error: linker `arm-none-eabi-gcc` not found
│   → Install the cross toolchain:
│       sudo apt install gcc-arm-none-eabi      # Debian / Ubuntu
│       brew install arm-none-eabi-gcc          # macOS
│
├─ ld: cannot find -lddsc / -lcyclonedds-ddsc
│   → Cyclone DDS backend needs its native lib installed first:
│       just cyclonedds setup     # for `rmw-cyclonedds`

Did the binary build but not produce output?
├─ Hangs after "Opening session" / no `Published:` lines
├─ `nros::init -> -3` / `-100` (Transport error)
│   → zenohd isn't running. Open another terminal:
│       just zenohd run             # in the repo root
│     Or any system `zenohd --listen tcp/127.0.0.1:7447`.
│     Check the locator the example points at matches the
│     port zenohd is listening on (default 7447 for POSIX,
│     7451+ for QEMU per-platform tests).
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
├─ `just doctor` prints a fixit hint for every missing tool.
├─ `just <platform> doctor` scopes to one RTOS (e.g.
│   `just freertos doctor` for FreeRTOS / QEMU / arm-none-eabi).
└─ When all else fails, file an issue with:
    - the exact command you ran,
    - the full stderr,
    - `rustc --version`, `cmake --version`, `qemu-system-arm --version`.
```

## What success looks like

A correctly-running Rust Linux talker (`examples/native/rust/zenoh/talker`)
prints something like this on stderr (with `RUST_LOG=info`):

```text
[INFO  native_rs_talker] nros Native Talker (Zenoh Transport)
[INFO  native_rs_talker] =========================================
[INFO  native_rs_talker] Node created: talker
[INFO  native_rs_talker] Publisher created for topic: /chatter
[INFO  native_rs_talker] Published: 1
[INFO  native_rs_talker] Published: 2
[INFO  native_rs_talker] Published: 3
```

A correctly-running C talker (`examples/native/c/zenoh/talker`)
prints on stdout:

```text
nros C Talker
=================
Published: 1
Published: 2
Published: 3
```

A correctly-running C++ talker prints the same `Published: N` line
once per second.

The ROS 2 side (`ros2 topic echo /chatter std_msgs/msg/Int32` with
`RMW_IMPLEMENTATION=rmw_zenoh_cpp`) should see:

```text
data: 1
---
data: 2
---
data: 3
---
```

If you see all three of these — talker logging, ROS 2 echo
output, and matching counter values — interop is verified end-to-end.

## See also

- [Install + first build](./installation.md) — full setup walkthrough
- [First Node — Rust](./first-node-rust.md) — the canonical Rust starter
- [Troubleshooting](../user-guide/troubleshooting.md) — broader
  issue-by-issue reference for post-first-build problems
