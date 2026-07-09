---
id: 167
title: "riscv-nuttx image panics at boot — garbage fn-ptr (EPC=0x4) inside ZenohRmw::open on the nros_cpp_init backend path"
status: open
type: bug
area: nuttx
related: [issue-0165, issue-0135, phase-285]
---

## Summary

The riscv32 rv-virt NuttX C-talker image
(`examples/qemu-riscv-nuttx/c/talker/build-zenoh/nuttx_riscv_c_talker`) **panics
at boot**, before the talker runs. Exposed by phase-285 W2's new
`QemuProcess::start_nuttx_riscv` harness — this is riscv-nuttx's FIRST boot; the
board had only ever been link-checked (nightly `build-riscv-c`), never run, so
the crash was latent.

```
riscv_exception: EXCEPTION: Instruction access fault. MCAUSE: 00000001, EPC: 00000004, MTVAL: 00000004
riscv_exception: PANIC!!! Exception = 00000001
up_dump_register: EPC: 00000004 ... RA: 00000004
```

`EPC = RA = 0x00000004` is a **jump/return through a garbage function pointer**
(a small non-null value, ~`0x4`) — not a plain null, so it slips past `beqz`
null-guards.

## Reproduce

```
just nuttx build-riscv-c   # builds the rv-virt kernel + the C talker image
qemu-system-riscv32 -M virt -bios none -nographic -icount shift=auto \
    -kernel examples/qemu-riscv-nuttx/c/talker/build-zenoh/nuttx_riscv_c_talker \
    -netdev user,id=net0 -device virtio-net-device,netdev=net0
# → riscv_exception PANIC within the first tick
```

(Or via the test harness: `QemuProcess::start_nuttx_riscv(binary, true)`.)

## Diagnosis (gdb on rv-virt, 2026-07-09) — root cause located

Ran `qemu-system-riscv32 … -gdb tcp::1234 -S` + `riscv-none-elf-gdb`. Caught the
fault at `riscv_exception` (mcause=1, instruction access fault) and a
hardware-breakpoint at `*0x4`, then walked the call chain by breaking at each
layer and confirming which frame is entered but never returns. The crash is **NOT
in the kernel work-queue / netdev / virtio path** (those run fine — the virtio-net
MMIO `metal_io` regions have valid ops, notify works). It is on the **nano-ros
backend-open path**, fully symbolized:

```
nros_app_main
  → nros_cpp_init(config, "node", storage)          # s0 held the "node" name string at the fault
      → nros_app_register_backends → nros_rmw_zenoh_register   [OK — CffiRmw registered into REGISTRY @0x8009da40]
      → getenv / config parse                          [OK]
      → REGISTRY scan → matches CffiRmw
      → <CffiRmw as Rmw>::open                          [0x80012ed6]
          → CffiSession::open_with_vtable              [0x80011846]
              → vtable dispatch  lw a5,0(s8); jalr a5  [0x80011934]  (vtable @0x80089314 — all 16 entries VALID)
                  → vtable[0] = nros_rmw_cffi::rust_adapter::open_trampoline::<ZenohRmw>  [0x80013078]
                      → ZenohRmw::open → zenoh-pico session open
                          → jump/ret through a GARBAGE fn-ptr (~0x4) → instruction-access fault
```

Every layer down to `open_trampoline<ZenohRmw>` is entered and never returns; the
C-FFI vtable itself is intact. The bad pointer materializes **inside
`ZenohRmw::open` / the zenoh-pico session bring-up**. At the fault: `EPC=RA=0x4`,
`a0=a5=0`, `a4=0xe0`, `s0`→rodata `"node"`, a shallow near-leaf stack — i.e. a
function-pointer read from an uninitialized / wrong-offset struct field, then
tail-jumped.

The deeper walk (above) showed every sampled pointer/vtable is *valid when read* —
the corruption is transient. Continued investigation (2026-07-09, qemu re-enabled
in the sandbox) reclassified the bug.

## ROOT CAUSE — timing-dependent virtio-net IRQ re-entrancy race (NOT config)

**It is a race, not a static bug.** A qemu instruction trace (`-d exec,nochain`)
of the *same* image runs clean to idle — `riscv_exception` is never reached — while
every `gdb` run crashes. The difference is timing: **QEMU slirp packet-arrival is
host-timed, not `-icount`-controlled**, so the exact moment a virtio-net RX/txdone
IRQ lands relative to guest execution varies run-to-run.

The exposed window: zenoh-pico's TCP connect makes the calling thread run the
**entire TX poll synchronously with the virtio-net device IRQ enabled** —
`connect → psock_tcp_connect → netdev_txnotify_dev → netdev_upper_txavail →
netdev_upper_txavail_work → devif_poll → netdev_upper_txpoll → virtio_net_send →
virtqueue_add_buffer/kick` — mutating the TX virtqueue. If a virtio IRQ
(`virtio_mmio_interrupt → virtio_net_txdone`/`rxready`/`virtio_net_send_ctrl_rx`)
fires **mid-poll**, the handler touches the same vring re-entrantly → a corrupted
descriptor/return-address → the `jr`/`ret` to `~0x4`. Deterministic-looking under
one timing, absent under another = classic race.

`arm-nuttx` boots the *same* image bare (no router) without panicking — the race
just doesn't align on its timing. So the bug is riscv-**timing**-specific, not a
riscv code path that is wrong per se.

## Ruled out (each built + booted, still panics)

- **Stack overflow** — hpwork stack 16 % used (276/1712 B, `STACK_COLORATION`), IRQ
  stack 41 %. Not overflow.
- **Undersized stacks** — bumped INIT/DEFAULT/IDLE/HPWORK/IRQ stacks → still panics.
- **IOB packet-buffer config** — matched arm (`IOB_NBUFFERS=64`, `BUFSIZE=1534`,
  `ALIGNMENT=16`, `THROTTLE=8`) → still panics.
- **Full arm net-config mirror** — added `NET_SOCKOPTS`/`NET_TCP_NODELAY`/
  `NET_ROUTE`/`NET_TCPBACKLOG`/`NETDEV_IFINDEX`/`NET_IGMP`/`RECV_BUFSIZE=32768`/
  `SCHED_HPWORKPRIORITY=192` → still panics.
- `CONFIG_NETDEV_WORK_THREAD` is **not a real Kconfig symbol** in this NuttX;
  olddefconfig drops it. virtio-net leaves `rxtype` = 0 (`NETDEV_RX_WORK`).

## Fix direction (needs a decision — vendored, or design)

A defconfig change will **not** fix this; the exposure is in the vendored NuttX
netdev/virtio-net TX path. Candidates:
1. Don't run the full `devif_poll`/vring mutation **synchronously** from
   `netdev_upper_txavail` while the device IRQ is live — queue it (serialize on the
   work thread), or take a critical section / mask the virtio-net IRQ around vring
   ops in `drivers/virtio/virtio-net.c`.
2. Understand precisely why arm's timing never aligns and whether a
   nano-ros-side change (e.g. deferring the zenoh connect off the boot/init path,
   or a settle delay before connect) closes the window without touching vendored
   code.

Both touch either vendored NuttX (guarded by CLAUDE.md) or the nano-ros
riscv-nuttx boot/connect sequencing — a design decision, not a quick patch.

## Verifying (env note)

`qemu-system-riscv32` is sandbox-gated; run it **bare** (harness background) so the
`sandbox.excludedCommands` rule matches, then attach `riscv-none-elf-gdb` to
`tcp::1234`. `-d exec` will NOT reproduce (changes the race). Tracked under
[phase-285](../roadmap/phase-285-riscv-nuttx-run-tiers-boot-harness.md) W2.b.

[0135]: archived/0135-native-zenoh-service-query-path-broken.md
