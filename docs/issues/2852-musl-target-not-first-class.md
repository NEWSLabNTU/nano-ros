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
