# Phase 74 — Test Infrastructure: Parallel Isolation

**Goal**: Enable parallel execution of test groups that are currently
serialized (`max-threads = 1`) by isolating network interfaces, zenohd
instances, and CMake build directories per test.

**Status**: In Progress (74.1 complete)

**Priority**: Medium

**Depends on**: None

## Overview

All QEMU-based E2E test groups (`qemu-network`) and C/C++ API test groups
(`c_api`, `cpp_api`) run with `max-threads = 1` in `.config/nextest.toml`.
Tests that could run in parallel are executed sequentially.

The bottleneck is shared infrastructure:
- **2 TAP devices** (`tap-qemu0`/`tap-qemu1`) shared by 7 QEMU platform groups
- **Port 7447** hardcoded in firmware config.toml — only one zenohd per test
- **Shared CMake build dirs** — C/C++ library race conditions

Each QEMU E2E test takes 30–90 seconds (boot + stabilization + data exchange).
With 7 platform groups serialized in one `qemu-network` group, total wall
time is 5–10 minutes for QEMU tests alone.

See `docs/known-issues.md` issue 9 for full documentation.

## Work Items

- [x] 74.1 — QEMU slirp (user-mode) networking for tests
  - [x] 74.1.1 — MPS2-AN385 bare-metal (ARM, LAN9118)
  - [x] 74.1.2 — MPS2-AN385 FreeRTOS (ARM, lwIP/LAN9118)
  - [x] 74.1.3 — NuttX ARM virt (Cortex-A7, virtio-net)
  - [x] 74.1.4 — ThreadX RISC-V 64 (virt, virtio-net-device)
  - [x] 74.1.5 — ESP32-C3 QEMU (Espressif fork, open_eth)
  - [x] 74.1.6 — ThreadX Linux simulation (veth — kept as-is, not QEMU)
  - [x] 74.1.7 — Test harness: remove `cleanup_tap_network()`
  - [x] 74.1.8 — Launch script: add `--slirp` flag
- [ ] 74.2 — Configurable zenohd port in firmware and tests
- [ ] 74.3 — Per-platform zenohd instances with scouting disabled
- [ ] 74.4 — Per-example CMake build directories for C/C++ tests
- [ ] 74.5 — Split `qemu-network` into per-platform test groups
- [ ] 74.6 — Update documentation

---

### 74.1 — QEMU slirp (user-mode) networking for tests

Replace TAP-based networking with QEMU's built-in slirp (user-mode)
networking. Each QEMU instance gets its own fully isolated NAT stack — no
TAP devices, no bridges, no `sudo`, no `setup-network.sh`.

**Validated**: PoC confirmed full E2E talker→listener communication via
slirp on MPS2-AN385 (bare-metal). Both QEMU instances connect to zenohd
on the host through the slirp gateway `10.0.2.2`. See `tmp/slirp-test/`
for test artifacts.

**Current** (persistent, shared, requires sudo):
```
Host:
  tap-qemu0 ──┐
               ├── qemu-br (192.0.3.1/24) ── zenohd :7447
  tap-qemu1 ──┘
  (all platforms contend for these 2 devices)
```

**Proposed** (slirp, per-test, no privileges):
```
QEMU instance 1 (talker):
  NIC ── slirp NAT ── 10.0.2.2 ──→ host:<port> (zenohd)
  guest IP: 10.0.2.<unique>

QEMU instance 2 (listener):
  NIC ── slirp NAT ── 10.0.2.2 ──→ host:<port> (zenohd)
  guest IP: 10.0.2.<unique>

Each QEMU process has its own isolated slirp stack.
No TAP, no bridge, no sudo.
```

**Critical constraint**: Each QEMU instance **must use a different guest
IP** within `10.0.2.0/24`. Board crates seed ephemeral port counters and
zenoh session IDs from the IP address. Same IP → same source port → TCP
4-tuple collision → `ConnectionFailed`. Validated in PoC.

**IP allocation scheme** (per-platform, unique across all parallel tests):

| Platform | Peer 0 (talker/server) | Peer 1 (listener/client) | Peer 2 (extra) |
|----------|------------------------|--------------------------|----------------|
| bare-metal | 10.0.2.10 | 10.0.2.11 | 10.0.2.12 |
| FreeRTOS | 10.0.2.20 | 10.0.2.21 | 10.0.2.22 |
| NuttX | 10.0.2.30 | 10.0.2.31 | 10.0.2.32 |
| ThreadX RISC-V | 10.0.2.40 | 10.0.2.41 | 10.0.2.42 |
| ESP32-C3 | 10.0.2.50 | 10.0.2.51 | 10.0.2.52 |
| ThreadX Linux | 127.0.0.1 | 127.0.0.1 | — |

**Benefits over TAP**:
- Zero privileges (no `sudo`, no `CAP_NET_ADMIN`)
- Zero setup (`setup-network.sh` becomes optional, for manual use only)
- Perfect isolation (each QEMU has its own NAT, no shared interfaces)
- Works in unprivileged CI containers (no network namespace access needed)

**Limitations**:
- No guest-to-guest direct communication (guests communicate via zenohd)
- No ICMP (ping doesn't work, but zenoh uses TCP)
- Slightly higher latency than TAP (NAT overhead), but adequate for tests

---

#### 74.1.1 — MPS2-AN385 bare-metal (ARM, LAN9118)

**Status**: PoC validated

`QemuProcess::start_mps2_an385_networked()` in `qemu.rs` — used by
bare-metal Rust examples (talker, listener, service, action).

**QEMU flag change**:
```diff
- -nic tap,ifname=tap-qemu{N},script=no,downscript=no,model=lan9118,mac={MAC}
+ -nic user,model=lan9118
```

**Board crate seed**: `nros-mps2-an385/src/node.rs:158-163` seeds
`zpico_smoltcp::seed_ephemeral_port()` from `host_time + ip_byte * 251`
and `random::seed()` from `u32::from_be_bytes(config.ip)`. Unique IPs
are required.

**Config.toml change** (all examples under `examples/qemu-arm-baremetal/`):
```diff
  [network]
- ip = "192.0.3.10"
+ ip = "10.0.2.10"
- gateway = "192.0.3.1"
+ gateway = "10.0.2.2"

  [zenoh]
- locator = "tcp/192.0.3.1:7447"
+ locator = "tcp/10.0.2.2:7447"
```

**Files**:
- `packages/testing/nros-tests/src/qemu.rs` — `start_mps2_an385_networked()`
- `examples/qemu-arm-baremetal/rust/zenoh/*/config.toml` (all examples)

#### 74.1.2 — MPS2-AN385 FreeRTOS (ARM, lwIP/LAN9118)

Same QEMU machine as bare-metal (MPS2-AN385 + LAN9118). Shares the
`start_mps2_an385_networked()` function — the slirp change in 74.1.1
covers the QEMU flags.

**Board crate seed**: `nros-mps2-an385-freertos/build.rs` generates
`srand(seed)` from IP+MAC bytes. Unique IPs required.

**Config.toml change** (all examples under `examples/qemu-arm-freertos/`):
Same pattern as 74.1.1 — update `ip`, `gateway`, `locator` to slirp
subnet. FreeRTOS configs use `netmask` instead of `prefix`:
```diff
  [network]
- ip = "192.0.3.10"
+ ip = "10.0.2.20"
- gateway = "192.0.3.1"
+ gateway = "10.0.2.2"
  netmask = "255.255.255.0"

  [zenoh]
- locator = "tcp/192.0.3.1:7447"
+ locator = "tcp/10.0.2.2:7447"
```

**Additional**: C and C++ examples exist under `qemu-arm-freertos/c/` and
`qemu-arm-freertos/cpp/` — their config.toml files need updating too.

**Files**:
- `examples/qemu-arm-freertos/rust/zenoh/*/config.toml`
- `examples/qemu-arm-freertos/c/zenoh/*/config.toml`
- `examples/qemu-arm-freertos/cpp/zenoh/*/config.toml`

#### 74.1.3 — NuttX ARM virt (Cortex-A7, virtio-net)

`QemuProcess::start_nuttx_virt()` in `qemu.rs`. Uses ARM `virt` machine
with default virtio-net NIC (no explicit `model=` needed).

**QEMU flag change**:
```diff
- -nic tap,ifname={tap_iface},script=no,downscript=no
+ -nic user
```

When `tap_iface == "none"` (boot tests): keep `-nic none` (no change).

**Board crate seed**: NuttX uses POSIX network stack with kernel-managed
`/dev/urandom`, not IP-seeded smoltcp. Session ID uniqueness comes from
the OS random source, not IP. Different IPs are still recommended but
the constraint is less critical than for bare-metal platforms.

**Config.toml change** (all examples under `examples/qemu-arm-nuttx/`):
```diff
  [network]
- ip = "192.0.3.10"
+ ip = "10.0.2.30"
- gateway = "192.0.3.1"
+ gateway = "10.0.2.2"

  [zenoh]
- locator = "tcp/192.0.3.1:7447"
+ locator = "tcp/10.0.2.2:7447"
```

**Files**:
- `packages/testing/nros-tests/src/qemu.rs` — `start_nuttx_virt()`
- `examples/qemu-arm-nuttx/rust/zenoh/*/config.toml`
- `examples/qemu-arm-nuttx/cpp/zenoh/*/config.toml`

#### 74.1.4 — ThreadX RISC-V 64 (virt, virtio-net-device)

`QemuProcess::start_riscv64_virt()` in `qemu.rs`. Uses RISC-V `virt`
machine with explicit `-netdev` + `-device` syntax (required because
virtio-net-device needs `bus=virtio-mmio-bus.0`).

**QEMU flag change**:
```diff
- -netdev tap,id=net0,ifname={tap_iface},script=no,downscript=no
+ -netdev user,id=net0
  -device virtio-net-device,netdev=net0,bus=virtio-mmio-bus.0,mac={mac}
```

The `-device` line is unchanged — only the `-netdev` backend changes
from `tap` to `user`. The MAC address is still needed to match the
firmware config.

**Board crate seed**: `nros-threadx-qemu-riscv64/c/app_define.c:127-132`
seeds `srand()` from `IP * 2654435761 ^ MAC`. Unique IPs required.

**Config.toml change** (all examples under `examples/qemu-riscv64-threadx/`):
```diff
  [network]
- ip = "192.0.3.10"
+ ip = "10.0.2.40"
- mac = "52:54:00:12:34:56"
+ mac = "52:54:00:12:34:56"    # unchanged
- gateway = "192.0.3.1"
+ gateway = "10.0.2.2"
  netmask = "255.255.255.0"

  [zenoh]
- locator = "tcp/192.0.3.1:7447"
+ locator = "tcp/10.0.2.2:7447"
```

**Files**:
- `packages/testing/nros-tests/src/qemu.rs` — `start_riscv64_virt()`
- `examples/qemu-riscv64-threadx/rust/zenoh/*/config.toml`

#### 74.1.5 — ESP32-C3 QEMU (Espressif fork, open_eth)

`start_esp32_qemu()` in `esp32.rs`. Uses Espressif's QEMU fork
(`qemu-system-riscv32 -M esp32c3`) with `open_eth` NIC model.

**QEMU flag change**:
```diff
- -nic tap,model=open_eth,ifname={tap_iface},script=no,downscript=no,mac={mac}
+ -nic user,model=open_eth
```

**Needs validation**: The Espressif QEMU fork may not support slirp with
`open_eth`. If `open_eth` is a platform device (like LAN9118 on MPS2),
slirp should work. If it's PCI-only, the `-nic` syntax may not work and
we may need the `-netdev` + `-device` form. Test with:
```bash
qemu-system-riscv32 -M esp32c3 -nic user,model=open_eth ...
```
If slirp doesn't work with the Espressif fork, keep TAP for ESP32 tests
and isolate via per-platform TAP devices instead.

**Board crate seed**: `nros-esp32-qemu/src/node.rs:188` seeds
`random::seed()` from hardware RNG. The ESP32-C3 has a true RNG
peripheral — unique IPs are recommended but not strictly required for
session ID uniqueness.

**Note**: ESP32 tests currently use port **7448** (not 7447). With
per-platform zenohd instances (74.3), all platforms can use ephemeral
ports.

**Config.toml change** (all examples under `examples/qemu-esp32-baremetal/`):
```diff
  [network]
- ip = "192.0.3.10"
+ ip = "10.0.2.50"
- gateway = "192.0.3.1"
+ gateway = "10.0.2.2"

  [zenoh]
- locator = "tcp/192.0.3.1:7448"
+ locator = "tcp/10.0.2.2:7448"
```

**Files**:
- `packages/testing/nros-tests/src/esp32.rs` — `start_esp32_qemu()`
- `examples/qemu-esp32-baremetal/rust/zenoh/*/config.toml`

#### 74.1.6 — ThreadX Linux simulation (veth → loopback)

ThreadX Linux is **not a QEMU platform** — it runs as a native Linux
process. The NetX Duo Linux driver uses `AF_PACKET`/`SOCK_RAW` on veth
pairs, which requires `CAP_NET_RAW` and the `qemu-br` bridge.

Slirp does not apply here. Instead, investigate switching to **localhost
loopback** for zenoh connectivity:
- Replace `AF_PACKET`/`SOCK_RAW` veth driver with a TCP/UDP socket driver
- Firmware connects directly to `tcp/127.0.0.1:<port>` (zenohd on host)
- No bridge, no veth, no `CAP_NET_RAW` needed

**Alternative**: If the raw socket driver must be preserved, use
**per-test network namespaces** (`unshare --net`) with dedicated veth
pairs inside each namespace. This still needs `sudo` but provides
isolation.

**Interim approach**: Keep veth pairs but assign per-platform veth names
(already done: `veth-tx0`/`veth-tx1`). ThreadX Linux tests already run
in their own test group (`threadx-linux`), so they don't contend with
QEMU platforms. The main improvement comes from isolating the QEMU
platforms via slirp.

**Board crate seed**: `nros-threadx-linux/c/app_define.c:110-115` seeds
`srand()` from `IP * 2654435761 ^ MAC`. Unique IPs required if running
multiple instances.

**Config.toml**: No subnet change needed — keep `192.0.3.x` or switch
to `127.0.0.x` loopback if/when the socket driver is implemented.

**Files**:
- `packages/testing/nros-tests/tests/threadx_linux.rs`
- `packages/boards/nros-threadx-linux/c/app_define.c`
- `examples/threadx-linux/rust/zenoh/*/config.toml`

#### 74.1.7 — Test harness: remove `cleanup_tap_network()`

With slirp, the `cleanup_tap_network()` function in `qemu.rs` becomes
unnecessary. It kills stale TCP connections and flushes ARP on the bridge
— none of which exist in slirp mode.

**Changes**:
- Remove or `#[cfg(feature = "tap")]`-gate `cleanup_tap_network()`
- Remove `cleanup_tap_network()` calls from test fixtures
- Remove `is_tap_bridge_available()` checks from test setup
- Remove `read_config_ip()` helper used by cleanup

**Files**:
- `packages/testing/nros-tests/src/qemu.rs` — lines 438-499
- `packages/testing/nros-tests/src/fixtures/zenohd_router.rs` — calls

#### 74.1.8 — Launch script: add `--slirp` flag

Update `scripts/qemu/launch-mps2-an385.sh` to support slirp as an
alternative to TAP for manual development use.

```bash
# TAP mode (existing, requires setup-network.sh first):
./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu0 --binary firmware.elf

# Slirp mode (new, no setup needed):
./scripts/qemu/launch-mps2-an385.sh --slirp --binary firmware.elf
```

When `--slirp` is passed, use `-nic user,model=lan9118` instead of
`-nic tap,...`. Print a note about needing `config.toml` with slirp
subnet (`10.0.2.x`, gateway `10.0.2.2`).

**Files**:
- `scripts/qemu/launch-mps2-an385.sh`

---

### 74.2 — Configurable zenohd port in firmware and tests

The zenohd locator is compiled into firmware:
- **C/C++ firmware**: `config.toml` → `nano_ros_read_config()` in
  `cmake/NanoRosConfig.cmake` → `APP_ZENOH_LOCATOR` define
- **Rust firmware**: `config.toml` → `include_str!("../config.toml")`

The test harness generates a per-test `config.toml` with the correct
slirp gateway IP and ephemeral zenohd port, then rebuilds the firmware.

For C/C++ builds: pass `-DNROS_CONFIG_FILE=<generated_config.toml>` to CMake.
For Rust builds: write `config.toml` to the example directory before building.

New `PlatformNetwork` struct encapsulates: platform name, guest IP range,
zenohd router handle, and generated config.toml path.

**Files**:
- `packages/testing/nros-tests/src/fixtures/binaries.rs`
- `packages/testing/nros-tests/src/qemu.rs`
- New: `packages/testing/nros-tests/src/fixtures/platform_network.rs`

### 74.3 — Per-platform zenohd instances with scouting disabled

Each platform test starts its own zenohd with full isolation.

`ZenohRouter::start()` already passes `--no-multicast-scouting`, which
prevents multicast UDP discovery between instances. No additional scouting
flags are needed.

**Additional isolation**: Bind each zenohd to `127.0.0.1:<ephemeral>`
rather than `0.0.0.0`. Since slirp NAT translates guest connections to
host-local connections, `127.0.0.1` is sufficient and prevents
cross-platform connections.

**Files**:
- `packages/testing/nros-tests/src/fixtures/zenohd_router.rs`

### 74.4 — Per-example CMake build directories for C/C++ tests

C/C++ tests share `build/` within each example directory. Use unique build
dirs per test process (e.g., `build-<pid>/`).

The pre-built static libraries (`libnros_c_zenoh.a`, `libnros_cpp_zenoh.a`
in `build/install/lib/`) are read-only during tests — the conflict is only
in each example's CMake build dir.

Cleanup: remove the build directory after the test completes.

**Files**:
- `packages/testing/nros-tests/src/fixtures/binaries.rs`
- Example `.gitignore` files (add `build-*/` pattern)

### 74.5 — Split `qemu-network` into per-platform test groups

Replace the single `qemu-network` group with per-platform groups in
`.config/nextest.toml`.

**Current**:
```toml
[test-groups.qemu-network]
max-threads = 1
# All 7 platforms share this one group
```

**Proposed**:
```toml
[test-groups.qemu-baremetal]
max-threads = 1

[test-groups.qemu-freertos]
max-threads = 1

[test-groups.qemu-nuttx]
max-threads = 1

[test-groups.qemu-threadx-riscv]
max-threads = 1

[test-groups.qemu-esp32]
max-threads = 1

[test-groups.threadx-linux]
max-threads = 1
```

Each group uses its own slirp instance and zenohd, so they run
concurrently. Within each group, tests are still serial (one QEMU pair
at a time per platform).

Also increase `c_api`/`cpp_api` parallelism now that build dirs are
isolated (74.4).

**Files**:
- `.config/nextest.toml`

### 74.6 — Update documentation

- Mark issue 9 as fixed in `docs/known-issues.md`
- Update `scripts/qemu/setup-network.sh` header comments (optional for tests)
- Update `tests/README.md` with slirp-based test networking
- Remove TAP requirement from test prerequisites

**Files**:
- `docs/known-issues.md`
- `tests/README.md`
- `scripts/qemu/setup-network.sh`

## Acceptance Criteria

- [ ] Each QEMU E2E test uses slirp (`-nic user,...`) instead of TAP devices
- [ ] Each QEMU instance has a unique guest IP (for ephemeral port + zenoh ID seeding)
- [ ] Each test starts its own zenohd with `--no-multicast-scouting` on an ephemeral port
- [ ] Firmware config.toml is generated per-test with correct zenohd locator
- [ ] C/C++ API tests use isolated build directories (no race conditions)
- [ ] `just test-all` passes with per-platform parallel execution
- [ ] Wall-time reduction observed when multiple QEMU platforms run concurrently
- [ ] No multicast scouting interference between zenohd instances
- [ ] Tests run without `sudo` or `setup-network.sh` (slirp needs no privileges)
- [ ] `setup-network.sh` still works for manual development use

## Notes

- **Slirp validated**: PoC confirmed MPS2-AN385 bare-metal talker + listener
  communicate successfully through slirp. Key requirement: unique guest IPs
  per instance (board crate seeds ephemeral ports and zenoh IDs from IP).
- **No sudo needed**: Slirp eliminates the `sudo` requirement for all QEMU
  tests. `setup-network.sh` remains for manual TAP-based development.
- **Zephyr tests**: Zephyr uses its own network stack (not TAP bridge).
  Remains in its own group, benefits from not contending with QEMU groups.
- **ThreadX Linux**: Uses `AF_PACKET`/`SOCK_RAW` via veth pairs — slirp
  doesn't apply. Keep veth for now (already in a separate test group).
  Loopback socket driver is a future option for rootless CI.
- **ESP32-C3 QEMU**: Espressif fork — slirp + `open_eth` needs validation.
  If unsupported, fall back to per-platform TAP devices for ESP32 only.
- **Disk space**: Per-example build dirs increase disk usage during tests.
  Cleanup on completion mitigates this.
- **Rebuild cost**: Firmware rebuilds per-test (with new config.toml) may
  be slow. Mitigate with `OnceCell` caching per platform — rebuild only
  when config changes.
