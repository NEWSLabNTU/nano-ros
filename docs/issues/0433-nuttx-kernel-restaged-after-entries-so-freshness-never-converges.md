---
id: 433
title: The NuttX kernel is re-staged after the entries link, so the fixture freshness probe can never converge
status: open
type: bug
area: testing
related: [phase-337, rfc-0069, issue-0418]
---

## Problem

`just nuttx build-fixtures` exits 0, and the fixtures it just built immediately
read STALE:

```
Failed to build first binary (nuttx rust Action): BuildFailed(
  "Test fixture is STALE — a source is newer than the built binary:
     binary: …/rust/action-server-entry/target/armv7a-nuttx-eabihf/nros-minsizerel/nuttx_rs_action_server_entry
     newer:  …/third-party/nuttx/nuttx/staging/libc.a")
```

Measured right after a green build:

```
20:42:48  …/nuttx_rs_action_server_entry
20:46:00  third-party/nuttx/nuttx/staging/libc.a
```

The kernel artifacts (`staging/libc.a`, `include/nuttx/config.h`) are written
**3 minutes after** the entry links against them. They are inputs to the entry,
so the probe is right that the binary is older — but running the build again
reproduces the same ordering. Two consecutive `just nuttx build-fixtures` runs,
both `rc=0`, leave the same four cells unrunnable.

Confirmed not a one-off: `nuttx c Action` fails the same way against
`third-party/nuttx/nuttx/include/nuttx/config.h`.

## Why it matters

`nuttx rust` and `nuttx c` action cells cannot be run at all — not "fail", but
never execute, reported as a skip-shaped failure. That is the 0350 class: a
coordinate that never runs looks the same as one that cannot run on this host.

It blocked RFC-0069's last acceptance item (every action Runtime cell green on
real targets, the raw↔raw pairs the payload-envelope change actually alters).
`nuttx cpp` passes, so the lane is half-verified in a way no summary shows.

## Not the cause

Two other things were wrong on this path and are FIXED, so they will not confuse
the next reader:

* Stale `CMakeCache.txt` files naming `packages/boards/nros-board-nuttx-qemu-arm`
  — the board dir phase-337 W3 consolidated into `nros-board-nuttx-qemu`. Five
  workspace build dirs plus twelve example ones. Wiped.
* `_nros_profile_query args` returning `--profile nros-minsizerel` as ONE string,
  which the nuttx carve-out mapfiled into a single argv element and cargo
  rejected. Fixed in `nros_cargo_profile_args_for`.

With both fixed the nuttx build goes green; this issue is what remains.

## Fix direction

Either stage the kernel BEFORE the entries link (the dependency order the probe
already assumes), or exclude the regenerated kernel artifacts from the entry's
input signature and depend on the kernel's own inputs instead. The first is
probably right — the current order means the linked entry and the staged kernel
are not provably the same build.
