# QEMU icount: Virtual Clock Synchronization

## Problem

QEMU's virtual clock and wall-clock time diverge when emulating bare-metal
firmware with TAP networking:

1. **Hardware timers** (CMSDK Timer0 at 25 MHz) advance with QEMU's virtual clock
2. **WFI** (Wait For Interrupt) during `Mono::delay(10ms)` advances the virtual
   clock by 10ms almost instantly in wall-clock time
3. **TAP network I/O** requires wall-clock time — packets traverse the host kernel
   TCP stack, through zenohd, and back at wall-clock speed

Without correction, a 10-second zenoh-pico query timeout (`z_get`) fires in
~1–2 seconds of wall-clock time. The server's reply (traveling through zenohd
on the host) hasn't arrived yet, causing spurious timeout failures.

This affects all bare-metal QEMU platforms that use hardware timers and TAP
networking. RTOS platforms (FreeRTOS, NuttX, Zephyr) handle this through their
own OS tick management.

## Solution: `-icount shift=auto`

QEMU's `-icount` option virtualizes time based on instruction count. The key
parameter is `sleep=on` (the default), which controls WFI behavior:

- **`sleep=on`** (default): During WFI, virtual time advances at wall-clock
  speed via `QEMU_CLOCK_VIRTUAL_RT`. A 10ms delay takes ~10ms of wall time.
- **`sleep=off`**: During WFI, virtual time jumps instantly to the next timer
  deadline. This provides determinism but breaks network timing.

With `shift=auto`, QEMU adaptively adjusts the instruction-to-nanosecond ratio
to keep virtual time tracking wall-clock time. The adaptive algorithm
(in `icount-common.c:icount_adjust()`) monitors the drift between
`QEMU_CLOCK_VIRTUAL` and `QEMU_CLOCK_VIRTUAL_RT` and adjusts the shift value
(between 0 and 10) to compensate.

From QEMU source (`accel/tcg/icount-common.c`, `icount_start_warp_timer()`):

```c
/*
 * We do stop VCPUs and only advance QEMU_CLOCK_VIRTUAL after some
 * "real" time, (related to the time left until the next event) has
 * passed. The QEMU_CLOCK_VIRTUAL_RT clock will do this.
 * This avoids that the warps are visible externally; for example,
 * you will not be sending network packets continuously instead of
 * every 100ms.
 */
```

## Usage

All networked QEMU MPS2-AN385 launches include `-icount shift=auto`:

```bash
qemu-system-arm \
    -cpu cortex-m3 \
    -machine mps2-an385 \
    -nographic \
    -icount shift=auto \
    -semihosting-config enable=on,target=native \
    -kernel firmware.elf \
    -nic tap,ifname=tap-qemu0,script=no,downscript=no,model=lan9118
```

This is configured in:
- **Test infrastructure**: `packages/testing/nros-tests/src/qemu.rs` —
  `start_mps2_an385_networked()` adds `-icount shift=auto`
- **Launch script**: `scripts/qemu/launch-mps2-an385.sh` — always includes
  `-icount shift=auto`

## icount Parameter Reference

```
-icount [shift=N|auto][,align=on|off][,sleep=on|off]
```

| Parameter | Default | Description |
|-----------|---------|-------------|
| `shift=N` | — | Fixed: each instruction = 2^N nanoseconds of virtual time |
| `shift=auto` | — | Adaptive: auto-adjusts shift (0–10) to track wall clock |
| `sleep=on` | on | WFI advances virtual time at wall-clock speed |
| `sleep=off` | — | WFI jumps instantly to next timer deadline (deterministic but breaks networking) |
| `align=on` | off | Throttle guest to not run faster than real-time (requires fixed shift, incompatible with `shift=auto`) |

### Constraints

- `shift=auto` is incompatible with `align=on` and `sleep=off`
- `align=on` is incompatible with `sleep=off`
- `shift=auto` starts at 125 MIPS (shift=3) and adjusts within 100ms

## Tradeoffs

| | Without icount | With `-icount shift=auto` |
|---|---|---|
| Boot speed | Fast (emulates at host CPU speed) | Slower (virtual time ≈ wall time) |
| Timer accuracy | Virtual time races ahead | Virtual time tracks wall clock |
| WFI behavior | Virtual clock advances instantly | Virtual clock advances at wall-clock speed |
| Network timeouts | Fire too early (spurious failures) | Fire at correct wall-clock times |
| Determinism | Non-deterministic (host-speed dependent) | Semi-deterministic (adaptive) |

## Comparison with Other Platforms

The same virtual-vs-wall-clock problem appears in other contexts:

- **Zephyr native_sim**: Uses `CONFIG_NATIVE_SIM_SLOWDOWN_TO_REAL_TIME=y` to
  throttle the simulator. Equivalent to our `-icount shift=auto`.
- **Zephyr QEMU**: Uses `-icount shift=6,align=off,sleep=off` but **disables
  it when networking is active** (`default y if !NETWORKING && !BT`). Their
  `sleep=off` causes the instant-jump behavior that breaks networking.
  We use `sleep=on` (default) to avoid this.
- **ESP32-C3 QEMU**: Uses `-icount 3` (fixed shift, `sleep=on` default) for
  the boot test in `justfile`.

## Hardware Timer Requirement

With `-icount shift=auto`, the platform clock **must** be backed by a hardware
timer (CMSDK Timer0, DWT, etc.) rather than a software counter advanced by
poll callbacks. The hardware timer reads from `QEMU_CLOCK_VIRTUAL`, which
icount keeps synchronized with wall time. A poll-driven software clock is
disconnected from QEMU's virtual clock and would not benefit from icount.

See `book/src/advanced/platform-porting-pitfalls.md` § "Clock Sources" for the
full platform clock status table.

## References

- [QEMU TCG Instruction Counting](https://www.qemu.org/docs/master/devel/tcg-icount.html)
- [Zephyr: Configure QEMU to run independent of the host clock (#14173)](https://github.com/zephyrproject-rtos/zephyr/issues/14173)
- [Zephyr: qemu_x86 and qemu_cortex_m3 time handling broken with CONFIG_QEMU_ICOUNT (#26242)](https://github.com/zephyrproject-rtos/zephyr/issues/26242)
- [QEMU source: `accel/tcg/icount-common.c`](https://github.com/qemu/qemu/blob/master/accel/tcg/icount-common.c)
