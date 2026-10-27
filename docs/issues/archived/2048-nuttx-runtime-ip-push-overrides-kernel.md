---
id: 2048
title: "NuttX board pushes `eth0`'s address with SIOCSIFADDR, overriding the kernel that owns it"
status: resolved
type: tech-debt
area: [boards, nuttx, network]
severity: low
found: 2026-10-25
resolved: 2026-10-27
related: [phase-514, RFC-0109]
---

## What

RFC-0109 D2 says NuttX owns its board's network (`[board.net] stack = "os"`)
and nano-ros only READS it. Phase-514 W4 moved the board's address default
there: `BOARD_NET_*` are now generated from `nuttx-config/<arch>/defconfig`'s
`CONFIG_NETINIT_*`, and the arm / arm-smp defconfigs were moved from
`10.0.2.30` to `10.0.2.15` so the generated value equals what the board
applied before (no runtime change).

But the board still WRITES the address at run time:
`nros-board-nuttx-qemu/src/node.rs::apply_ip_config` issues `SIOCSIFADDR` /
`SIOCSIFNETMASK` / `SIOCSIFDSTADDR` on `eth0` from `Config::ip()`, reached by
`entry_212n::configure_entry_eth0` (Rust and C entries alike) and by the
direct-mode presets (`Config::listener()` etc., `ip + 1/2/3`). Its comments
give two reasons: two guests sharing one network would collide (Phase
97.4), and "without this push the guest keeps its defconfig address
[`10.0.2.30`], never reaches slirp's `10.0.2.2`".

## Question to answer before retiring it

With the defconfig now stating `10.0.2.15`, does an image that skips the
push still reach the router? Every guest runs behind its own SLIRP, so the
collision reason applies only to a bridge/TAP network, where the address
would come from a NuttX config fragment the image names in `os_config`. The
`/dev/urandom` reseed from the IP (ZID uniqueness) is a separate concern and
needs its own source if the push goes.

## Done when

Either the push is gone and the NuttX QEMU cells pass with the kernel's own
address, or this issue records the measurement that shows it is still
needed and why.

## Resolution

The push is gone. NuttX's own `defconfig` address is the one every image runs
with, and the board only reads it back.

### Measured

Images built on this branch, with the push and the `/dev/urandom` reseed
removed, booted under the test harness's own QEMU line (`-netdev user`,
`virtio-net-device`), one SLIRP per guest, against one host `rmw_zenohd`:

- **Address.** Every guest reads `eth0` back as `10.0.2.15` (`SIOCGIFADDR`,
  printed at boot as `nros: eth0 10.0.2.15 (kernel-owned, RFC-0109 D2)`):
  the Rust example images (`nros::main!` → `BoardEntry::run_with_deploy`),
  the arm C/C++ example images (`nros-nuttx-ffi` `main`), and the rv-virt C
  talker (`nros-nuttx-riscv-ffi`).
- **Router reachability.** Rust talker + listener, 4 runs: 43/43, 24/24,
  24/24, 24/24 published/heard. The rv-virt C talker opened a router session
  on `tcp/10.0.2.2:8700` and published 39 samples in 40 s.
- **ZIDs.** The 4 talker+listener runs gave the router 8 distinct client
  ZIDs (`New transport opened … and <zid>` in its debug log). Two guests
  start with the same address, so this is not an artefact of the reseed's
  absence: the reseed fed `Config::ip()`, which was `10.0.2.15` in every
  guest (no image, fixture row or `NROS_IP` bake states another), so it
  gave every guest the SAME seed. And the session ZID on NuttX is not read
  from `/dev/urandom` anyway: `zpico_fill_session_zid` takes its generic
  arm there (session address, a per-process counter, `CLOCK_REALTIME`).

### Why the push existed, and why that reason is gone

The role-path push came from Phase 97.4 (per-role addresses for DDS guests
sharing one multicast/dgram L2); issue #130 extended it to the Entry path,
diagnosing that an entry image kept the defconfig address (the arm one then
said `10.0.2.30`) and failed `Executor::open`. Phase-514 W4
moved the defconfigs to `10.0.2.15`, so the push wrote back what the kernel
had already applied. The two-guests-on-one-L2 case has no live user:
`start_nuttx_virt_mcast` / `start_nuttx_virt_dgram` are called by no test,
and an image that wants another address on such a network now names it
where RFC-0109 D2 puts it, the kernel's `CONFIG_NETINIT_*`. The rv-virt C
entry never pushed at all and was already passing on the defconfig's
address.

### What changed

- `nros-board-nuttx-qemu`: `node::init_hardware` / `apply_ip_config` and
  `entry_212n::configure_entry_eth0` + `SLIRP_DEFAULT_{IP,GATEWAY,PREFIX}`
  removed. `entry_net_init` (Rust single-tier, `run_with_deploy`,
  `run_tiers`) now only merges the deploy overlay for locator/domain and
  calls `node::report_kernel_eth0()`, the `SIOCGIFADDR` read-back.
- `nros-nuttx-ffi` (arm C/C++): the push and the never-baked
  `NROS_IP`/`NROS_GATEWAY`/`NROS_PREFIX` parse removed; reports instead.
  `nros-nuttx-riscv-ffi` reports too.
- `Config::{talker,listener,server,client}` role presets removed — they
  existed to give the push distinct addresses and would now name an address
  no guest has. No caller in the tree.
- Regression guard: `rtos_e2e`'s `ensure_ready` asserts, for every NuttX
  cell, that the ready image printed a `NUTTX_ETH0_REPORT` line with a SLIRP
  host address (`10.0.2.x`), so a `defconfig` that loses its `NETINIT`
  address fails there with the address in the message rather than as an
  opaque `ConnectionFailed`.

### Tests

All on fixtures rebuilt from this branch (`just nuttx build-fixtures-arm`,
`just nuttx build-riscv-c`), run solo/serial with `cargo nextest`:

- `rtos_e2e`, every NuttX cell (Rust/C/C++ x pubsub/service/action), now
  including the eth0 assertion: 9/9 green. In-sweep reds seen on the way,
  each green when re-run solo — C and C++ pubsub in a parallel run
  (`ConnectionFailed` at session open, both images reporting `10.0.2.15`),
  and the Rust action cell once in a serial run (the server completed the
  goal and the client received feedback, so both sessions were up; 3/3 solo
  afterwards). Neither is about the address: each red's own log shows the
  kernel-owned `10.0.2.15`, and the same images went green solo. The
  parallel-run `ConnectionFailed` was not investigated further.
- `typed_carrier_e2e::nuttx_arm_typed_carrier_boots_and_publishes`: green.
- `entry_e2e` / `realtime_tiers_e2e` NuttX cells need native peer fixtures
  that were not built here, so the two workspace entries were booted by hand
  against a router instead: `realtime-rust` (`NuttxQemu::run_tiers`, port
  8291) and `realtime-c` (`nros-nuttx-ffi`, port 8391) both reported
  `eth0 10.0.2.15` and opened a router session.
- rv-virt C talker (`c_riscv_nuttx_e2e`'s image) booted by hand, as above.
