---
id: 440
title: NuttX Rust entries lost the board's static link args when phase-338 W2 collapsed the -entry packages, so every one fails to link
status: open
type: bug
area: build
related: [phase-338, phase-339, rfc-0032, rfc-0048]
---

## Problem

Every NuttX Rust example fails to link with ~3680 undefined references to basic
libc symbols:

```
undefined reference to `abort'
undefined reference to `calloc'
undefined reference to `clock_gettime'
undefined reference to `nsh_initialize'
…
```

Not a few symbols — the kernel archives are not on the link line at all.

## Cause

`packages/boards/nros-board-nuttx-qemu/nros-board.toml` declares the STATIC
image-link args in its `cargo_config` block, and its comment states the
contract:

> "#127 — STATIC image-link args only (board-centric link convention, RFC-0032
> 'third leg'). … ZERO entry build.rs."

```toml
rustflags = [
    "-C", "link-arg=-Tdramboot.ld",
    "-C", "link-arg=-nostartfiles",
    "-C", "link-arg=-Wl,--start-group",
    "-C", "link-arg=-lsched",
    "-C", "link-arg=-ldrivers",
    …
]
```

The entry's `.cargo/config.toml` carries only the three cpu flags. The whole
`-T…` / `--start-group` / `-l…` set is missing, so nothing pulls in the kernel
archives:

```
$ grep -c lsched examples/qemu-arm-nuttx/rust/*/.cargo/config.toml
action-client 0   action-server 0   listener 0
service-client 0  service-server 0  talker 0
```

Zero of six. `nros sync` on the leaf exits 0 and does not add them.

## When

`ab486a8db` — "refactor(phase-338 W2): collapse the 18 `-entry` packages into
their node packages", 2026-08-05 21:18. That commit is the last to touch these
files, and `git log -S lsched` finds no commit that ever added the args to the
SURVIVING path: the args lived in the deleted `*-entry/.cargo/config.toml`, and
the collapse did not carry them into the node package that replaced it.

The NuttX Rust action cells were green at ~21:30 the same evening, on fixtures
built before the collapse landed — which is why this was not caught immediately.

## Scope

Rust lane only. The C/C++ NuttX fixtures are cmake-driven and get their link
args from `NROS_CMAKE_EXTRA_DEFS` + the board cmake module, not from a leaf
`.cargo/config.toml`.

Blocks: every `nuttx rust` Runtime cell, and the phase-339 W2/W3 verification
that needs a NuttX Rust entry to link.

## Fix direction

`.cargo/config.toml` in a Rust leaf is `nros sync`-managed (CLAUDE.md /
RFC-0048 W9), so the board's `cargo_config` should be projected into the
collapsed node package the same way it was into the `-entry` package. Either
the collapse dropped a per-package field that selected the board, or sync no
longer recognises these packages as board consumers — worth checking which,
because the second would be silent for other boards too.

A gate is cheap here and the class has bitten before: assert that every leaf
whose `package.metadata.nros` names a `deploy` with a board `cargo_config`
actually carries that block. Silent loss of link args is only visible at link
time, and only on the platform whose archives went missing.
