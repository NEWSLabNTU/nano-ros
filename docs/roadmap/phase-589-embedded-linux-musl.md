# Phase 589 — Embedded Linux (musl) as a deployment target

**Status (2026-11-23). OPEN.** W1 is done; the rest are open. W10 to W12
(the original Jetson Nano) are owned by nros 241; the others have no owner
yet. Makes a hosted Linux image for an embedded SoC running musl a
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

- **Status:** DONE (2026-11-23, PR #4643; issue 2844).
- **What:** the POSIX port takes `PTHREAD_STACK_MIN` from `<limits.h>`, and no
  build script names `-lpthread` / `-lrt` for a musl target
  (`nros_cc_flags::libc_libs`, read from `CARGO_CFG_TARGET_ENV`).
- **Acceptance:** met. The camera talker builds at the fix commit with no local
  patch: a static aarch64 ELF of 1,718,584 bytes, the same size as the patched
  proof of concept. `just ci gate` green.

### W2 — A board states its type

- **Status:** OPEN (issue 2849).
- **What:** every in-tree board descriptor states `[board.entry] type`, so an
  out-of-tree board copied from one needs no `NROS_BOARD_TYPE`.
- **Acceptance:** in the issue.

### W3 — A cross build needs no ROS 2 for the core messages

- **Owner:** nros 175.
- **Status:** DONE (2026-11-24, issue 2850). A dependency the bundled core
  interfaces satisfy no longer counts as a reason to require ament; the
  Linux talker builds with no ROS 2 in the environment. The musl half of the
  acceptance waits on W5's in-tree board (the check is target-independent).
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
- **The vendor runtime on the proof-of-concept board:** HiSilicon MPP V1.0.1.1
  (the `ss_mpi_*` API), plus `libss_mpi_km.so` and `libsvp_acl.so` for the
  NPU. All of them are musl shared objects, so a node linking them cannot be
  a static image.
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

### W10 — An image for the original Jetson Nano

- **Owner:** nros 241.
- **Status:** OPEN
- **The board:** the original Jetson Nano Developer Kit (Tegra X1, 4x
  Cortex-A57, 128-core Maxwell GPU). This is not the Orin Nano. Its last
  software release is JetPack 4.6.6, L4T R32.7.6, which is final for JetPack 4:
  Ubuntu 18.04, kernel 4.9, CUDA 10.2, TensorRT 8.2. Ubuntu 18.04's glibc is
  2.27, below the 2.34 floor of our host dists, so neither the `nros` CLI nor a
  glibc image built on a current host runs there. A static musl aarch64 image
  (W1) depends on none of it.
- **Where the board lives:** in a user project, as an out-of-tree board found
  through `NROS_EXTRA_BOARD_PATH`, per the board-support ruling. It reuses
  `nros-board-linux`, with target `aarch64-unknown-linux-musl` and
  `+crt-static`. nano-ros takes what any such board would hit, in W2 to W5.
- **What:** the Linux talker and listener, built on a host with `nros build`,
  copied to the Nano, and run against a zenoh router on the host (W12).
- **Acceptance:** on a Nano running stock L4T R32.7.6, the static talker runs
  and the host receives it, with no package installed on the Nano. `file`
  reports a statically linked aarch64 ELF, and the out-of-tree board's
  descriptor is the only Nano-specific file.

### W11 — A real-time kernel on the Nano, measured

- **Owner:** nros 241.
- **Status:** OPEN
- **Why a kernel:** the Tegra X1 has no real-time co-processor a user can
  program. Its BPMP runs boot and power firmware, and there is no SPE as on
  Xavier and Orin, so the IVC co-processor work does not apply. Real time here
  means a PREEMPT_RT L4T kernel, plus the `SCHED_FIFO` tiers
  `nros-board-linux` already applies.
- **The kernel path:** the L4T R32.7.6 kernel source carries
  `scripts/rt-patch.sh apply-patches`. Build it with the Linaro GCC 7.3.1
  aarch64 toolchain NVIDIA names for R32, from `tegra_defconfig` with
  Fully Preemptible Kernel (RT) and 1000 Hz, then install `Image`, the DTBs
  and the modules. NVIDIA ships no prebuilt RT kernel package for the Nano on
  R32 (its `nvidia-l4t-rt-kernel` repository covered AGX Xavier only), and
  calls the patches evaluation-grade. Forum reports describe lockups on some
  R32.7 releases, so the stock kernel image stays bootable beside it.
- **Measurements, each on the stock and the RT kernel:**
  - `cyclictest -m -S -p 90 -i 1000 -D 10m`, idle and under `stress-ng` CPU
    and memory load: the maximum and the p99 latency;
  - the nano-ros timer jitter at 100 Hz and 1 kHz under `SCHED_FIFO`, the same
    measurement as the proof of concept's, including the rate the 1 kHz timer
    actually reaches (W7).
- **Acceptance:** a table of those numbers in this document, with the kernel
  build recorded (config delta and source release). The RT kernel's
  `cyclictest` maximum under load is below 100 us, or the measured value and
  its cause are recorded.

### W12 — ROS 2 interop through a router on a host

- **Owner:** nros 241.
- **Status:** OPEN
- **Why a host:** ROS 2 Humble targets Ubuntu 22.04 and ships no 18.04
  packages, so no ROS 2 runs on the Nano. The Nano's nodes run in zenoh client
  mode against `rmw_zenohd` on a host with ROS 2, which is the proof of
  concept's topology.
- **Acceptance:** on the host, `ros2 topic echo` receives the Nano's talker,
  and a host `ros2 run demo_nodes_cpp talker` reaches the Nano's listener.
  `ros2 topic echo` also shows phase-574 W8's detections from the Nano.

### Not feasible: Genuino 101

The user's Genuino 101 (Intel Curie: a Quark SE core with 80 KB SRAM) was
judged not feasible on 2026-11-23, in the manager session's report to the
user. Its two reasons: no current Zephyr release supports the board, and 80 KB
of SRAM is below what a networked nano-ros image needs.

## Order

W1 first. W2 to W4 are small and independent. W5 depends on W1 and absorbs
the proof of concept's wrapper. W6 and W7 are measured on the device. W8 and
W9 follow W5. W10 needs only W1, so it does not wait for W5; W11 and W12
follow W10, and W11 measures W7's 1 kHz case on a second device.
