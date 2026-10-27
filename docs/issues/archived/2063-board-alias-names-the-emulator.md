---
id: 2063
title: "Board alias `qemu-mps2-an385` names the emulator, not the board"
status: resolved
type: tech-debt
area: [boards, naming]
severity: low
found: 2026-10-26
resolved: 2026-10-27
related: [RFC-0093, phase-437, phase-517]
---

## What

`packages/boards/nros-board-mps2-an385/nros-board.toml` answers to
`names = ["baremetal", "bare-metal", "qemu-mps2-an385", "rtic-mps2-an385"]`,
and `qemu-mps2-an385` is the spelling most of the tree authors: every
`examples/mps2-an385-baremetal/{c,cpp}/*/system.toml` writes
`[image.*] board = "qemu-mps2-an385"`, the Rust leaves pass it to
`nros::main!(deploy = …)`, and the book's bare-metal chapter shows it
(`git grep -l qemu-mps2-an385` — 76 tracked files on 2026-10-26).

RFC-0093 R3 says a board name never names the emulator: QEMU is how a machine
is RUN, not what the build targets, and the image built for this board runs on
the AN385 part whether QEMU hosts it or not. The board's own index key already
follows the rule — `mps2-an385-baremetal` — so the same board is spelled two
ways, one of them the shape R3 retires.

RFC-0093 §3 left `qemu-mps2-an385` "untouched" because it was then a
`deploy=` value rather than a board key. Since phase-517 W3 there is one board
vocabulary from image to setup (`nros build --board mps2-an385-baremetal`
matches an image authored `qemu-mps2-an385` through the descriptor), so the
exemption no longer separates two vocabularies; it keeps a second spelling of
one board alive.

## Siblings

The same §3 list carries `threadx-qemu-riscv64` (machine reach — RFC-0093 §4
selects `rv-virt-threadx`), and `rtic-mps2-an385` folds a framework into the
board word. Rule each when this is picked up; do not rename by analogy.

## Not done here

No rename. A rename touches the descriptor's `names`, the `nros::main!`
board-path table (`nros_orchestration_ir::board_path_for`), every example
`system.toml`, the fixture manifest and the book, and needs a deprecation
path — an alias that refuses naming its replacement, as phase-500 W5 did for
`native`, never a silent map.

## Why the gate is green

`check-board-name-reach` enforces RFC-0093 §5's "no board name contains
`qemu` unless upstream's does", and since issue 1614 it asks descriptor
aliases too — but only for a descriptor that lists its cmake OVERLAY's name
among its `names` (`descriptor_aliases`: `owner = next(n for n in names if n
in found)`). This descriptor's names are `baremetal`, `bare-metal`,
`qemu-mps2-an385`, `rtic-mps2-an385`; its overlay is
`cmake/board/nano-ros-board-mps2-an385-baremetal.cmake`, a name not in the
list, so the whole descriptor is out of scope and the alias is never asked
(measured 2026-10-26: `OK — … 3 known violation(s)`, none of them this one).
Fixing the reach — attributing a descriptor to its overlay through the index
key it provisions through, not through name equality — will put this alias in
the baseline before the rename takes it out.

## Resolution

**Gate reach first.** `check-board-name-reach` now attributes a descriptor to
its overlay through the index key it provisions through. That is the same
derivation as `BoardDescriptor::setup_key`: `cmake_module`, then a name, then
the platform. Name equality stays as the fallback. The bare-metal mps2-an385
descriptor came into scope, and `alias:qemu-mps2-an385` entered the ratchet
baseline. It was the only new entry (3 → 4 known violations).

**Rename.** `qemu-mps2-an385` became `mps2-an385-baremetal` everywhere it is
authored: every `examples/mps2-an385-baremetal/{c,cpp,rust}/*/system.toml`, the
Rust leaves' doc comments, the test bins and bench leaf, the freertos workspace
fixture's bringup, `examples/fixtures.toml`, the book, the board crate's
`package.xml` provides, and the CLI/macro unit tests. Where a test's point was
"an alias resolves", it now uses the alias `baremetal`. In the descriptor's
`names` (and the `package.xml` provides), the word is replaced by the
canonical `mps2-an385-baremetal`, listed first. `check-provider-announcements`
wants the canonical name first, and `check-deploy-board-resolves` and
`check-image-panic-policy` read `names`, not `cmake_module`. The
`BOARD_PATHS` row is gone, because `mps2-an385-baremetal` already had one.

**Refusal, not a map.** `qemu-mps2-an385` is a row in
`nros_entry_lower::RETIRED_BOARD_KEYS`, the table phase-500 W5 built for
`native`/`posix`. Every front door that asks `retired_board_key_hint` refuses
the word with ``write `mps2-an385-baremetal` ``: `nros::main!`, `nros setup`,
the catalog's `resolve_deploy`, image validation and entry codegen. The rows now
carry their own provenance in `why`. `nros new --platform` hides only the rows
whose replacement is itself a platform choice, so this board word does not
become a hidden `--platform` value. With the rename done the baseline line is
deleted again (back to 3).

Archived issues, archived phases, the phase-517 measurements and changelog
fragments keep the old spelling: they are dated records.
`docs/reference/retired-board-names.md` and the book's `retired-names.md` list
the word. RFC-0093 §3 carries an amendment.

## Siblings — left open, for the maintainer to rule

Not renamed. Per this issue's own rule, each one is ruled separately. No
follow-up issue was filed: the rule asks for a ruling, not new work.

- `threadx-qemu-riscv64`: an alias of `rv-virt-threadx` (`cmake_module`), and
  already an `alias:` line in the reach baseline.
- `rtic-mps2-an385`: it selects a DIFFERENT entry type
  (`::nros_board_mps2_an385::RticMps2An385`) on the same descriptor, so it is
  an entry-framework selector, not a second spelling of the board.
