# Quick Reference

## Finding Commands

Root commands are grouped by audience:

```bash
just --list                         # grouped root command overview
just --groups                       # group names
just --group main --list            # normal development workflows
just --group full-matrix --list     # build-all / fixtures / test-all / ci
just --group setup --list           # setup and doctor entry points
just --group maintenance --list     # clean and generated-binding commands
just --group docs --list            # book and API docs
```

Platform and backend commands stay namespaced:

```bash
just --group full-matrix --list zephyr
just zephyr build-fixtures
just zephyr build-all
just qemu setup-network             # QEMU TAP networking only
just zephyr help                    # Zephyr-specific help
just zenohd build                   # Build the pinned zenoh router
```

Install the local book tooling before previewing docs:

```bash
just setup-docs                     # mdbook + mdbook-mermaid
just book-serve                     # serve book/src with live reload
just book                           # full deployed preview with API docs
```

## Test profiling & slow-test reporting

`just test` and `just test-all` always print the slowest tests after the
run summary — the top 20 by duration (binary, test name, time, status),
parsed from the active profile's `target/nextest/<profile>/junit.xml`. No
flag needed; it is part of the normal output.

Deeper profiling is **opt-in** and adds nextest's experimental event/output
recording plus artifact export. It preserves the normal nextest execution
model (same filters, cargo profile, parallelism) — it only records and
exports, so leave it off for routine runs:

```bash
NROS_NEXTEST_RECORD=1 just test
NROS_NEXTEST_RECORD=1 just test-all
```

Each profiled run writes a timestamped directory under `tmp/` with a
stable `-latest` symlink:

```text
tmp/nextest-profile-test-YYYYMMDD-HHMMSS/      tmp/nextest-profile-test-latest -> …
tmp/nextest-profile-test-all-YYYYMMDD-HHMMSS/  tmp/nextest-profile-test-all-latest -> …
```

Artifacts in that directory:

- `nextest-run.zip` — portable recording archive; replay with full
  captured output (incl. successful tests) via `cargo nextest replay`.
- `nextest-trace.json` — Chrome/Perfetto timeline (slot/group occupancy,
  idle slots, retries, long-pole tests). Canonical concurrency artifact.
- `junit.xml` — copy of the run's JUnit report.
- `env.txt`, `command.txt` — the knobs and exact command used.

Knobs:

| Variable | Effect |
|----------|--------|
| `NROS_NEXTEST_RECORD=1` | Enable recording + artifact export. |
| `NROS_NEXTEST_RECORD_DIR=<path>` | Override the output dir (and its `-latest` link). |
| `NROS_NEXTEST_TRACE_GROUP_BY=slot\|binary` | Perfetto grouping; default `slot` (wall-clock/concurrency view). |
| `NROS_NEXTEST_REPLAY_LOG=1` | Also write `nextest-replay.log` (full captured stdout/stderr — can be large on chatty tests). Off by default; rely on the portable archive otherwise. |
| `NROS_NEXTEST_RECORD_KEEP_STATE=1` | Keep the temp `NEXTEST_STATE_DIR` (`<dir>/state`); removed after export otherwise. |
| `NROS_NEXTEST_RUN_PROFILE=fail-fast` | Stop at the first failure instead of the default `--no-fail-fast` full report (uses `target/nextest/fail-fast/junit.xml`). |

Overhead and retention: recording adds event/output-store writes during
the run, and `nextest-run.zip` (plus an enabled `nextest-replay.log`) can
get sizable on output-heavy suites — keep it opt-in for local runs and
prune old `tmp/nextest-profile-*` directories. Recording uses a
profile-local `NEXTEST_STATE_DIR`, so it never pollutes the user's global
nextest record store. Do **not** reach for `--no-capture` to inspect
output: it serializes execution and skews every timing. Use the replay
archive instead.

## Manual Testing

```bash
# Base setup builds/checks the in-tree zenoh router.
just setup base
source ./activate.sh

# Terminal 1: Router
zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: Talker
cd examples/native/rust/talker && RUST_LOG=info cargo run --features zenoh

# Terminal 3: Listener
cd examples/native/rust/listener && RUST_LOG=info cargo run --features zenoh
```

## UDP Transport

On native/POSIX, zenoh-pico has built-in UDP support via OS sockets:

```bash
# Use UDP instead of TCP for the zenoh locator
NROS_LOCATOR=udp/127.0.0.1:7447 cargo run --features zenoh
```

On bare-metal, enable the `link-udp-unicast` feature to use UDP over smoltcp:

```toml
nros = { features = ["rmw-zenoh", "platform-bare-metal", "link-tcp", "link-udp-unicast"] }
```

> The `rmw-zenoh` feature here is the *lowering* of the declared RMW —
> you declare the backend once in `system.toml` (`[system].rmw` /
> `[deploy.<t>].rmw`) and the toolchain sets the cargo feature; the
> feature is what the build uses, not the user-facing selector. See
> [RFC-0031](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0031-rmw-selection-and-lowering.md).

## TLS Transport

TLS layers on top of TCP using mbedTLS. Requires a self-signed certificate (or real CA cert) and the `link-tls` Cargo feature.

**Generate a test certificate:**
```bash
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 \
  -keyout key.pem -out cert.pem -days 365 -nodes -subj "/CN=localhost"
```

**Native/POSIX** (requires `libmbedtls-dev` -- installed by `just setup base`):

```bash
# Terminal 1: Router with TLS
zenohd --no-multicast-scouting --listen tls/localhost:7447 \
  --cfg 'transport/link/tls/listen_certificate:"cert.pem"' \
  --cfg 'transport/link/tls/listen_private_key:"key.pem"'

# Terminal 2: Talker with TLS
NROS_LOCATOR=tls/localhost:7447 \
  ZENOH_TLS_ROOT_CA_CERTIFICATE=cert.pem \
  cargo run -p native-rs-talker --features link-tls
```

**Bare-metal** (QEMU ARM):

On bare-metal, only base64-encoded certificates are supported (no filesystem).
Build with `--features link-tls`:

```bash
# Build TLS-enabled examples
cd examples/qemu-arm-baremetal/rust/talker
cargo build --release --features link-tls
```

The CA certificate must be passed via `ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64` at runtime
(through the zenoh config), or embedded in the binary at build time.

## ROS 2 Interop

```bash
# Terminal 1: Router
zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: nros talker
cd examples/native/rust/talker && RUST_LOG=info cargo run --features zenoh

# Terminal 3: ROS 2 listener
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

## Actions

```bash
# Terminal 1: Router
zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: Action server (Fibonacci example)
cd examples/native/rust/action-server && cargo run

# Terminal 3: Action client
cd examples/native/rust/action-client && cargo run
```

**Zephyr action tests:**
```bash
just zephyr build-all          # Build all Zephyr examples (Rust + C + C++ + XRCE)
just zephyr test               # Run zenoh E2E tests (covers actions)
```

## Docker Development Environment

Docker provides QEMU 7.2 (from Debian bookworm) which fixes TAP networking issues present in Ubuntu 22.04's QEMU 6.2.

```bash
# One-time setup: add yourself to docker group
sudo usermod -aG docker $USER
# Log out and back in, or run: newgrp docker

# Build and use Docker environment
just docker build              # Build nano-ros-qemu image
just docker shell              # Interactive shell
just docker test-qemu          # Run QEMU tests in container
just docker help               # Show all Docker commands
```

## QEMU Bare-Metal Testing

Run bare-metal Cortex-M3 examples on QEMU (MPS2-AN385 machine with LAN9118 Ethernet).

```bash
# Build prerequisites
just qemu build-zenoh-pico    # Build zenoh-pico for ARM Cortex-M3
just qemu build                # Build all QEMU examples

# Non-networked tests (no setup required)
just qemu test                # Bare-metal QEMU integration tests

# Networked talker/listener test (Docker Compose - recommended)
just docker test-qemu         # Runs zenohd, talker, listener in separate containers
```

**Docker Compose Architecture:**
```
┌─────────────────────────────────────────────────────────────┐
│              Docker Network: 172.20.0.0/24                  │
│                                                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   zenohd    │  │   talker    │  │      listener       │  │
│  │ 172.20.0.2  │  │ 172.20.0.10 │  │    172.20.0.11      │  │
│  │             │  │  ┌───────┐  │  │  ┌───────────────┐  │  │
│  │             │  │  │ QEMU  │  │  │  │     QEMU      │  │  │
│  │             │  │  │ ARM   │──┼──┼──│     ARM       │  │  │
│  │             │  │  │ TAP   │  │  │  │     TAP       │  │  │
│  │             │  │  └───────┘  │  │  └───────────────┘  │  │
│  └──────▲──────┘  └──────┼──────┘  └─────────┼───────────┘  │
│         └────────────────┴───────────────────┘              │
│                    NAT to zenohd                            │
└─────────────────────────────────────────────────────────────┘
```

**Manual networked test (3 terminals, requires host TAP setup):**
```bash
# Terminal 1: Setup network + start router
just qemu setup-network                    # Requires sudo
zenohd --listen tcp/0.0.0.0:7447

# Terminal 2: Talker (192.0.2.10)
./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu0 \
    --binary examples/qemu-arm-baremetal/rust/talker/target/thumbv7m-none-eabi/release/qemu-bsp-talker

# Terminal 3: Listener (192.0.2.11)
./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu1 \
    --binary examples/qemu-arm-baremetal/rust/listener/target/thumbv7m-none-eabi/release/qemu-bsp-listener
```

Run `just qemu help` for more options.

## Zephyr Setup

```bash
just zephyr setup       # Initialize workspace at $repo/zephyr-workspace/
just zephyr test        # Run zenoh tests (native_sim uses NSOS on host loopback)
just zephyr test-xrce   # Run XRCE tests
```

The workspace lives at `$repo/zephyr-workspace/` by default (gitignored).
Set `$NROS_ZEPHYR_WORKSPACE` to install elsewhere. Legacy sibling installs
at `../nano-ros-workspace/` are auto-detected; run
`./scripts/zephyr/migrate-workspace.sh` to consolidate.

See [Zephyr](../getting-started/zephyr.md) for full details.
