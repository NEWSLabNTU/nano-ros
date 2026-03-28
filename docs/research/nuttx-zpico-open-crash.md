# NuttX `z_open()` Crash Investigation

**Date**: 2026-03-29
**Status**: ROOT CAUSE FOUND — multiple networking configuration issues

## Summary

`z_open()` (zenoh-pico session establishment) crashes the NuttX init task
during QEMU ARM virt interactive runs. The task is silently killed by NuttX
and restarted, producing a loop of `nros NuttX platform starting` messages.

## Root Causes (3 issues)

### 1. QEMU creates PCI virtio-net, but NuttX only supports MMIO

**The primary blocker.** `-nic user` creates a PCI-based NIC by default on
QEMU ARM virt. NuttX's virtio driver only discovers MMIO transport devices
via FDT scanning. All 32 MMIO slots show device ID 0 (empty).

**Fix:** Use explicit MMIO device:
```
-netdev user,id=net0 -device virtio-net-device,netdev=net0
```
instead of `-nic user`.

Verified via GDB: `virtio_register_mmio_device_()` returns -ENODEV (-19) for
all 32 MMIO transport slots with `-nic user`. With `-device virtio-net-device`,
the net device is assigned to an MMIO slot and `ifconfig` shows `eth0`.

### 2. Rust binaries bypass NSH init sequence

NuttX's `nsh_main` entry point runs `nsh_initialize()` which calls:
1. `boardctl(BOARDIOC_INIT)` → `board_app_initialize()` → `qemu_bringup()` → `register_devices_from_fdt()` (virtio device discovery)
2. `netinit_bringup()` → configures IP address on eth0

Rust binaries have their own `main()` that calls `run()` directly, bypassing
this entire sequence. Without it, no virtio devices are registered and no
network interface is configured.

**Fix:** Call `boardctl(BOARDIOC_INIT, 0)` and `netinit_bringup()` explicitly
from the board crate's `init_hardware()`.

### 3. zenohd on 127.0.0.1 not accessible from QEMU slirp

QEMU slirp's gateway address (10.0.2.2) connects to the host's network stack,
but NOT to the loopback interface. zenohd listening on `127.0.0.1:7452` is
only accessible from host processes, not from QEMU guests.

**Fix:** zenohd must listen on `0.0.0.0:7452` for NuttX QEMU examples.

### 4. z_open() hangs on connect() failure (secondary issue)

When `connect()` fails (ENETUNREACH or ECONNREFUSED), `z_open()` enters an
internal retry/timeout loop. On NuttX, this eventually causes the init task to
be killed and restarted by the kernel (the exact kill mechanism is still unclear
— no panic, no ARM exception, no abort — likely a watchdog or stack check).

## NuttX defconfig fixes needed

```
# IOB buffers (required by virtio-net for packet buffers)
CONFIG_IOB_ALIGNMENT=16
CONFIG_IOB_BUFSIZE=1534
CONFIG_IOB_NBUFFERS=64
CONFIG_IOB_THROTTLE=8

# Network initialization (QEMU slirp subnet: 10.0.2.0/24)
CONFIG_NETUTILS_NETINIT=y
CONFIG_NSH_NETINIT=y
CONFIG_NETINIT_IPADDR=0x0a00021e
CONFIG_NETINIT_DRIPADDR=0x0a000202
CONFIG_NETINIT_NETMASK=0xffffff00
CONFIG_NETINIT_NOMAC=y

# Ethernet packet size
CONFIG_NET_ETH_PKTSIZE=1514

# Remove LATEINIT (not needed for virtio-net)
# CONFIG_NETDEV_LATEINIT is not set
```

## QEMU flags for NuttX

```bash
qemu-system-arm -M virt -cpu cortex-a7 -nographic \
    -icount shift=auto \
    -kernel <nuttx-binary> \
    -netdev user,id=net0 -device virtio-net-device,netdev=net0
```

## Verification

With all 3 fixes applied:
- `connect()` to 10.0.2.2:7452 succeeds on second attempt (first attempt
  fails with ECONNREFUSED due to timing — network needs ~3s to stabilize)
- Task no longer crashes/restarts
- `z_open()` appears to proceed (no more restart loop)

## Previous (incorrect) hypotheses

- Stack overflow → tested with 256KB, same behavior
- Rust panic → custom panic hook never fires
- ARM exception → QEMU `-d int` shows only IRQs/SVCs
- NuttX BSD socket bug → networking works fine once properly configured
- Signal handling issue → not the cause
- pthread stack too small → not the primary cause (but 32KB is still recommended)
