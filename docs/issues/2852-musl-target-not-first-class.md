---
id: 2852
title: "A musl Linux target is not first-class: no SDK index entry, no toolchain target, no board, and cc-rs's flags break a zig cross compiler"
status: open
type: limitation
severity: medium
area: [boards, sdk, toolchain, build]
related: [phase-589, issue-2844]
found: 2026-11-23
---
Found by a downstream proof of concept on an embedded Linux camera SoC (HiSilicon Hi3519DV500: 2x Cortex-A55, Linux 5.10, musl 1.2.3, BusyBox), built as an out-of-tree hosted board; see phase-589.

## What
With issue 2844's fixes the camera talker builds, but only through an
out-of-tree board and hand wiring:

- `aarch64-unknown-linux-musl` is in neither `nros-sdk-index.toml` nor the
  `rust-toolchain.toml` targets, so `nros setup` provisions nothing for it.
- No in-tree board names a musl target. The proof of concept's board reused
  `nros-board-linux` with `platform = "posix"`, `entry_kind = "hosted-main"`,
  `linker = "rust-lld"` and `-C target-feature=+crt-static`.
- The C compiler is `zig cc -target aarch64-linux-musl -fno-sanitize=all`
  behind a wrapper that drops cc-rs's `--target=`, which zig rejects; zig's
  default sanitizer flags break the link too. Nothing in the tree handles
  either.
- That board states no stack, so `nros build` reports "stack floor not
  checked: the posix board states no stack on this road".
- It states no `[board.run]` or `[board.flash]`, so `nros run` / deploy have
  nothing to act on.

## Fix direction
An in-tree hosted musl board (aarch64 first), with its target in the SDK
index and the toolchain, a provisioned C cross compiler (zig or a musl gcc),
the stack it states, and a `[board.run]` that copies to and starts on a device
over ssh.

## Acceptance
`nros setup` then `nros build` for the in-tree musl board, with no wrapper
script and no hand-set variables, produces a static binary that runs on the
target.

## Progress (2026-11-25, nros 175)

The user ruled the shape: a cross board `linux-aarch64-musl`, a QEMU
user-mode run kind for the on-host stage
([RFC-0118 D4b](../design/0118-target-execution-model.md)), and zig as an SDK
dist. The board landed with:

- the descriptor (`packages/boards/linux-aarch64-musl`), on
  `nros-board-linux`, static (`+crt-static`, `rust-lld`);
- `aarch64-unknown-linux-musl` in the SDK index, `rust-toolchain.toml` and
  `config/rust-targets.txt`;
- `[tool.zig]`, with one-word entry points (`zig-cc`, `zig-c++`, `zig-ar`). A
  two-word `CC` loses its second word to the header-deps compiler pin. The
  board's `CFLAGS_<triple>` carries zig's `--target=aarch64-linux-musl`, which
  overrides cc-rs's (zig keeps the last one);
- the host-board invariant narrowed to "no target triple" in its test, its gate
  and the key tables;
- a CMake module that refuses C and C++ (phase-589 W8) rather than build a
  host program.

### Acceptance, line by line

| Line | State | Evidence |
| --- | --- | --- |
| `nros setup` for the in-tree musl board | met | `nros setup linux-aarch64-musl` added the rustup target and installed zig from its dist |
| then `nros build`, no wrapper script, no hand-set variables | met | `nros new mtalker --board linux-aarch64-musl --lang rust`, then `nros build`, with nothing set by hand. The dist's one-line `zig-cc`, `zig-c++` and `zig-ar` entry points do not count as a wrapper script (user ruling, 2026-11-26): they ship in the provisioned dist and rewrite no flags |
| produces a static binary | met | `file`: ELF 64-bit aarch64, statically linked |
| that runs on the target: its user space | met on host (QEMU user mode) | `nros run` started the router, and the image published (`Publishing: 'Hello World: 1'` after 1.2 s) under `qemu-aarch64` 11.0 |
| that runs on the target: the real SoC kernel | OPEN, device-only | needs the device; QEMU's system calls are the host kernel's |

Two findings from the QEMU run:

- **QEMU user mode needs 7.2 or newer.** The POSIX port's priority-inheriting
  mutexes lock with `FUTEX_LOCK_PI`, which QEMU answers `ENOSYS` before 7.2.
  Ubuntu 22.04's `qemu-user-static` is 6.2, so the image failed at its first
  lock and never opened a socket (seen with `qemu-aarch64 -strace`). The runner
  now refuses an older QEMU, naming the reason. The passing run used
  `qemu-aarch64` built from the SDK's own QEMU fork (11.0) with
  `--target-list=aarch64-linux-user`; adding that target to the SDK's QEMU
  dist is a follow-up.
- **`nros run` treats a user-mode image as a host process for the network.**
  It starts the router and hands it the locator, since its sockets are the
  host's.

## Progress (2026-11-29, nros 175): the stack the board states

`nros build` used to say "stack floor not checked: the posix board
`linux-aarch64-musl` states no stack on this road". Now:

- **Which stack.** The app (registration and `Executor::open`, the `OPEN`
  role) runs on the process main thread, whose size is `RLIMIT_STACK` at
  run time, not a link-time number. A cross build of `nros-board-linux` (one
  whose target is not the host) states `memory.app_stack_bytes` for it
  (default 256 KiB), and `init_hardware` holds the limit to the statement:
  enough stays, a lower soft limit is raised (Linux grows the main stack up to
  the CURRENT soft limit), and a hard limit below it refuses to start, naming
  the knob. The host board's build states nothing, as before.
- **Measured, not copied.** The user accepted a QEMU user-mode measurement
  for this line (ruling 2026-11-29); the device-kernel run stays its own
  device-only line. Under QEMU 11.0 user mode, the talker template
  for 30 s publishing to a router, each guest thread's high-water read from
  `/proc/<qemu>/mem` as the lowest non-zero byte of its fresh, zero-filled
  stack mapping: main thread 106,616 B (dev) and 47,584 B (release, 3 runs of
  3 identical), argv/environment/auxv included. The zenoh-pico executor
  thread, on musl's 128 KiB default thread stack, peaked at 3,568 B (dev) and
  2,432 B (release), so musl's smaller thread default is no risk to it. This
  is the same static binary and C library the device runs, so the depth does
  not depend on the kernel; signal frames do, which the margin covers.
- **The check.** `nros build` holds a posix board with a target triple to its
  measured peak, as it does threadx-linux: zig's musl has no frame sizes for
  the walk. `config/stack-floors.toml` has the `[[observed]]` row
  (`talker/rust/zenoh@linux-aarch64-musl`, peak = the dev number). A fresh
  `nros new … --board linux-aarch64-musl` then `nros build` reports
  `OPEN stack 262144 B holds the floor 117280 B (measured)`.
- **The boot side, checked under QEMU** (its `prlimit64` passes the host's
  limits through): soft 8 MiB boots unchanged; soft 128 KiB is raised and
  says so ("nros: raised the main thread's stack limit from 131072 to 262144
  bytes, the stack this image states (memory.app_stack_bytes)") before it
  boots; hard 200 KiB refuses with "the stack limit's hard
  ceiling is 204800 bytes. This image states a 262144-byte main-thread
  stack (memory.app_stack_bytes); raise the limit (`ulimit -s`) or lower that
  knob."

### Still open in this issue

- **Deploying to a device.** This stays an image's `[image.<id>.run] command`
  (D4b). No board table states a device address.
