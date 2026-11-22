# Phase 589 — Embedded Linux (musl) as a deployment target

**Status (2026-11-23). OPEN.** W1 is in review; the rest are open, with no
owner yet. Makes a hosted Linux image for an embedded SoC running musl a
supported target: provisioned, built from an in-tree board, and run on the
device, with no hand wiring.

## Why

nano-ros covers two kinds of target today: the RTOS boards, and the host
build (`native`, glibc Linux). An embedded Linux SoC is neither. It runs a
real Linux kernel, so the POSIX platform port and the hosted entry fit it, but
its userland is often musl and BusyBox, with no ROS 2, no package manager, and
no glibc. Cameras, gateways and vision boards look like this, and they are
where a small static ROS 2 node is worth most.

## Proof of concept

A downstream user built and ran one on an IP camera SoC:

| | |
| --- | --- |
| Board | HiSilicon Hi3519DV500: 2x Cortex-A55, Linux 5.10, musl 1.2.3, BusyBox |
| Image | Rust talker, zenoh in client mode to a router on a host; static, 1.3 MB |
| Idle cost | peak RSS 1.37 MB, about 0.1 % CPU at 1 Hz |
| Throughput | 100 kB strings at 30 Hz: 1950 of 1950 delivered, 23.6 Mbit/s, 4.8 % CPU |
| Timer, 100 Hz, SCHED_FIFO | p99 jitter about +/-80 us |
| Timer, 1 kHz | reaches only about 845 Hz |

It was built with an out-of-tree board (`NROS_EXTRA_BOARD_PATH`):

- `platform = "posix"`, `entry_kind = "hosted-main"`;
- target `aarch64-unknown-linux-musl`, `linker = "rust-lld"`,
  `-C target-feature=+crt-static`;
- C compiler `zig cc -target aarch64-linux-musl -fno-sanitize=all`, behind a
  wrapper that drops cc-rs's `--target=`;
- `NROS_BOARD_TYPE` set by hand;
- three source patches, now issue 2844.

Each piece of hand wiring is a work item below.

## Work items

### W1 — A musl target builds

- **Status:** OPEN (issue 2844; the fix is in review).
- **What:** the POSIX port takes `PTHREAD_STACK_MIN` from `<limits.h>`, and no
  build script names `-lpthread` / `-lrt` for a musl target
  (`nros_cc_flags::libc_libs`, read from `CARGO_CFG_TARGET_ENV`).
- **Acceptance:** met. The camera talker builds at the fix commit with no local
  patch: a static aarch64 ELF of 1,718,584 bytes, the same size as the patched
  proof of concept. `just ci gate` pending.

### W2 — A board states its type

- **Status:** OPEN (issue 2849).
- **What:** every in-tree board descriptor states `[board.entry] type`, so an
  out-of-tree board copied from one needs no `NROS_BOARD_TYPE`.
- **Acceptance:** in the issue.

### W3 — A cross build needs no ROS 2 for the core messages

- **Status:** OPEN (issue 2850).
- **What:** messages the tree's committed core interfaces satisfy do not
  require ament on the build host.
- **Acceptance:** in the issue.

### W4 — Setup builds the launch resolver

- **Status:** OPEN (issue 2851).
- **Acceptance:** in the issue.

### W5 — An in-tree musl board

- **Status:** OPEN (issue 2852).
- **What:** an aarch64 hosted musl board, its target in the SDK index and the
  toolchain, a provisioned C cross compiler with the flags cc-rs needs, the
  stack it states, and a `[board.run]` that deploys to and starts on a device.
- **Acceptance:** in the issue.

### W6 — Large messages without hand-set transport sizes

- **Status:** OPEN (issue 2853).
- **What:** the zenoh fragment size is derived from the image's stated message
  sizes on POSIX, as the large-subscriber class already is.
- **Acceptance:** in the issue.

### W7 — 1 kHz timers

- **Status:** OPEN (issue 2854).
- **What:** find why a 1 kHz timer reaches 845 Hz. The executor keeps its
  phase grid, so the cause is below it (sleep granularity, kernel preemption).
- **Acceptance:** in the issue.

### W8 — C and C++ nodes linking a vendor SDK

- **Status:** OPEN (proposed; no issue yet).
- **What:** the user's next step is C/C++ nodes that link the SoC vendor's
  media SDK and NPU runtime. Those ship as shared libraries, so the image links
  musl dynamically, and each image needs its own link libraries, search paths
  and include directories through CMake.
- **Acceptance:** a C++ node on the in-tree musl board links a shared library
  named only in its own CMake, builds dynamically against the board's sysroot,
  and runs on the device.

### W9 — Message stamps from an external capture clock

- **Status:** OPEN (proposed; no issue yet).
- **What:** a camera stamps frames from its capture clock, not from the
  system clock. Find whether the board clock hook (the wall-clock source work)
  can supply a stamp source per image, and what a node needs to stamp with it.
- **Acceptance:** a design note naming the hook and its limits, then a node on
  the musl board stamping messages from a clock it supplies.

## Order

W1 first. W2 to W4 are small and independent. W5 depends on W1 and absorbs
the proof of concept's wrapper. W6 and W7 are measured on the device. W8 and
W9 follow W5.
