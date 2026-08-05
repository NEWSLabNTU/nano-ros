---
id: 440
title: NuttX Rust entries lost the board's static link args when phase-338 W2 collapsed the -entry packages, so every one fails to link
status: resolved  # fixed 2026-08-06
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

## Resolution (2026-08-06)

The board's 24 static link args are restored in all six leaves, taken from
`nros-board.toml`'s `cargo_config` (the SSoT) rather than from the deleted
`-entry` files, so nothing stale rode along. Only the `rustflags` array was
replaced — each leaf keeps its own `[patch.crates-io]` and `[env]`.

Verified: `just nuttx build-fixtures-arm` → RC=0, **0 undefined references**
(was ~3680), and all three NuttX action Runtime cells — Rust, C and C++ — pass.

**Gate: `check-board-cargo-config-applied`**, in `check-fast`. For every board
whose `cargo_config` declares a `-l<kernel lib>` group, each leaf deploying to
that board must carry a representative arg from it. Representative, not
exhaustive: the point is to catch a config that lost the GROUP, not to diff two
files that legitimately differ in patch tables and paths. It reports 6 leaves —
exactly the set that broke.

Watched to fire: truncating `talker`'s rustflags back to the cpu flags fails it
with the file named and exit 1; restoring goes green.

## Why nothing caught it

Worth stating, because the gate is shaped around it. The broken config was valid
TOML, `cargo metadata` accepted it, and `nros sync` reported success — the file
is TRACKED and sync deliberately leaves it alone. The loss was observable only at
LINK time, and only on the one platform whose archives went missing. Every
cheap check in the repo passed.

That is the same shape as issue 0196's rule: the thing that could detect the
defect was not watching the thing that broke.
