---
id: 167
title: "riscv-nuttx image panics at boot — struct pollfd ABI mismatch (std 8B ↔ NuttX 24B) smashes the entry task's return address"
status: resolved
resolved_in: d06d25fa4
type: bug
area: nuttx
related: [issue-0165, issue-0135, phase-285]
---

## Resolution (2026-07-11) — poll() `--wrap` ABI shim

Fixed. Root cause = the `struct pollfd` ABI mismatch below. The fix routes
`poll()` through a `-Wl,--wrap=poll` interposer that bridges Rust's 8-byte POSIX
`pollfd` to NuttX's 24-byte kernel struct:

- **libc fork** (`jerry73204/libc` branch `nuttx-0.2` @ `adb4c592e`) — added
  `__wrap_poll` in `src/unix/nuttx/mod.rs`: copies each caller `pollfd` into a
  private 24-byte NuttX-layout array, calls `__real_poll`, copies `revents` back.
- **superproject** (`fix(#167)` `d06d25fa4`) — added `-Wl,--wrap=poll` to the
  riscv `nros-nuttx-ffi/.cargo/config.toml` rustflags + bumped the libc pointer.

**Boot-verified:** the rv-virt C-talker image now boots past the fault —
`riscv_exception` never fires in a 50 s gdb window (previously every gdb run
crashed within the first tick); serial shows normal `telnetd` startup, no panic
dump. The three adjacent bug-fixes in `129fab4d4` (init stack, executor storage,
virtio-net ctrl DMA) remain landed but are not what closed this.

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

## DEFINITIVE ROOT CAUSE (2026-07-11) — `struct pollfd` ABI mismatch (std ↔ NuttX)

Traced end-to-end with gdb watchpoints. The corrupting write is a **CPU write from
Rust std**, not DMA:

- Rust std's `sanitize_standard_fds()` (`std/src/sys/pal/unix/mod.rs`) polls fds
  0/1/2 at program start: `poll(&[pollfd{fd:0..2, events:0, revents:0}; 3], 3, 0)`.
- The `libc` crate's `pollfd` for the nuttx target is the **generic 8-byte POSIX**
  layout (`fd:i32, events:i16, revents:i16`).
- **NuttX's `struct pollfd` (`include/sys/poll.h`) is 24 bytes**: `fd, events(u32),
  revents(u32), arg, cb, priv` — the kernel writes ALL six into the caller's array.
- So `poll()` writes 3×24 = 72 bytes into std's 24-byte (3×8) stack array → **48-byte
  overflow** → overwrites the entry task's saved return address (which sits just past
  the array) → the task returns to a garbage/zeroed address → instruction-access
  fault (`EPC=0x4` or `0x0`) at boot.

gdb watchpoint on the saved-ra slot caught the writers exactly: `poll` writes a
pointer, then `poll_teardown` writes 0, into `nsh_main`'s saved-ra slot; the poll
call is `main` (`nros_nuttx_ffi::main`, Rust std startup) with `fds=…, nfds=3`.

`arm-nuttx` has the **same** latent bug — its stack layout just absorbs the 48-byte
overflow into unused space instead of a live return address. The "timing-dependent"
observation (crashes under gdb, not `-d exec`) was a red herring of that layout
sensitivity, not a race.

### The three fixes already landed (commit fix(#167), fork c3fa5dfb06) are REAL but
### adjacent bugs, not this root cause:
- init stack 3072→64 KB (nros_cpp_init's 12 KB frame overflowed);
- executor storage fallback 79304→98304 (stale, undersized the C++ Node buffer);
- virtio-net ctrl-queue async-DMA-to-stack.
Each is a genuine defect exposed while hunting; none closes the pollfd overflow.

### Fix options (needs a decision — spans std/libc/NuttX ABI)
1. **libc `poll()` shim** in the fork: keep `pollfd` 8 bytes (std compiles), but make
   `poll`/`ppoll` allocate a 24-byte NuttX `pollfd` array internally, copy fd/events
   in and revents out. Contained to the libc fork; needs a way to call NuttX's real
   poll under a different symbol.
2. **libc `pollfd` = 24 bytes + std patch**: correct struct, but std's
   `sanitize_standard_fds` uses a 3-field literal → requires patching the pinned
   nightly std source (no in-repo std-patch mechanism today; `nuttx-libc-patch.sh`
   patches libc only).
3. **NuttX-side**: have `poll()` use internal per-fd storage instead of writing
   arg/cb/priv into the user's pollfd — a vendored NuttX change (fork branch).

## Decision (2026-07-09)

**Documented known limitation; riscv-nuttx stays off-matrix (per #165).** The fix
is a vendored-NuttX concurrency change (fork-branch workflow) or a boot/connect
sequencing redesign — deferred to when a maintainer can take it. Issue stays
`open`; the root cause and repro below are the handoff.

## Fix direction (when picked up — vendored, or design)

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
