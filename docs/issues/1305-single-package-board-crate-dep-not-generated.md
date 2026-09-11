---
id: 1305
title: "Switching a single-package Rust leaf's board is TWO places, not one — the board crate dependency is still hand-written (RFC-0098 D6 unmet)"
status: open
type: bug
area: tooling, cli, examples
severity: medium
found: 2026-09-11
related: [rfc-0098, rfc-0065, phase-445]
---

# The claim

RFC-0098 D6: *"the board crate dependency is generated… No user manifest
names a board crate, so switching board touches no `Cargo.toml`."*
phase-445's acceptance: *"Switching a single-package example to another board
is ONE line in `system.toml`; `git diff` afterwards shows that line only."*

# What actually happens

Measured 2026-09-11, on a copy of `examples/mps2-an385-baremetal/rust/talker`
outside the checkout, with phase-445 W4b + W5 applied. One line changed in
`system.toml` and nothing else:

```diff
 [image.mps2-an385-baremetal]
-board = "qemu-mps2-an385"
+board = "freertos"
```

`nros sync` reports success. `nros build` then fails in the leaf's own crate:

```text
error[E0433]: cannot find `nros_board_mps2_an385_freertos` in the crate root
 --> src/main.rs:4:1
  |
4 | nros::main!(panic = "own");
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^ could not find `nros_board_mps2_an385_freertos`
  |                            in the list of imported crates
```

# Why — two readers of "which board", and they disagree

D6 *is* implemented, for the generated workspace entry. A single-package leaf
is its own entry, and nothing generates its manifest, so the board reaches it
through two independent paths:

- `nros::main!` resolves the board the IMAGE names (phase-445 W5 made this
  exact: "resolves the declared board and nothing else"). It correctly asked
  for `nros_board_mps2_an385_freertos`.
- The leaf's `Cargo.toml` still hand-names the board crate —
  `nros-board-mps2-an385 = { version = "*", features = ["board-entry"] }` —
  and `nros sync` writes its `[patch.crates-io]` rows from what the MANIFEST
  names, so the generated settings file patched `nros-board-mps2-an385` while
  the macro asked for the other one.

Everything else followed the switch correctly, which is what makes this a
narrow gap rather than a design problem. The generated
`build/<image>/nros-cargo.toml` picked up the new board's whole toolchain:
the link group moved from `-Tlink.x` (cortex-m-rt's) to
`-Tmps2_an385.ld --nmagic`, `NROS_BOARD` became `freertos`, and the
`CC_thumbv7m_none_eabi` cross compiler came along. Only the crate dependency
did not.

# Consequence

- The acceptance sentence above is not met for the 70-odd single-package Rust
  leaves, and the failure is a `rustc` error naming a crate the user never
  wrote, rather than anything that says "also change the dependency".
- The docs have to say "two places" (`docs/design/0098-…` promises one).
  phase-445 W7 documents the true shape and links this issue.
- `nros sync` is the natural place to catch it and currently does not: it has
  both facts in hand — the image's board and the manifest's board crate — and
  reports `done.`

# Fix

Two candidates, and they are not equivalent:

1. **Generate the dependency.** The leaf's board crate row moves out of the
   authored `Cargo.toml` and into what `nros sync` writes, the way the
   generated workspace entry already gets it. This is what D6 says, and it
   makes the switch genuinely one line. It needs somewhere for a generated
   dependency to live for a package whose manifest the user owns — cargo has
   no `--config` hook for `[dependencies]`, so this probably means the
   single-package leaf gets a generated entry too, i.e. it stops being its own
   entry.
2. **Refuse, loudly, at sync.** Cheap and strictly an improvement: when the
   image's board crate is not among the manifest's dependencies, fail naming
   both spellings and the line to change. This does not meet D6; it converts a
   confusing `rustc` error into an actionable one.

(2) is worth doing regardless of whether (1) lands, because (1) is a shape
change and (2) is a check over facts sync already has.

Acceptance for either is a BUILD, not a gate (#393): flip one `board =` line
in a single-package Rust leaf, run `nros sync` + `nros build`, and get an
image — with `git diff` showing that one line.
