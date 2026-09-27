---
id: 1511
title: "Generating every entry removes the last example of a hand-written one, and there is no tutorial for the escape hatch it leaves behind"
status: open
type: docs
area: docs, examples, tooling
severity: medium
related: [1288, 0865, rfc-0065, rfc-0098, phase-445]
found: 2026-09-27
---

# The escape hatch will exist in the code and nowhere in the docs

`nros materialize` is a shipped subcommand — "take ownership of a generated
entry package (RFC-0065 D5) … `nros build` stops regenerating it". It is the
documented answer to *what if the generated entry is not what I want*. Nothing
in the tree shows a user what taking ownership looks like, when it is the right
move, or what they are responsible for afterwards, and once issue 1288 lands
there will be no hand-written entry left to read either.

That is the gap, and the ORDER is what makes it easy to miss: the gap opens on
the day the migration SUCCEEDS. Every entry generating correctly is the state
in which the tree stops documenting the alternative.

## Measured (2026-09-27)

A layout study classified every example tree. Its split: of 22 workspace
examples, 12 take a generated entry (RFC-0098 D9) and 10 carry hand-written
entry packages. All 15 hand-written entries are Zephyr — including
`examples/workspaces/realtime-cpp/src/fvp_entry`, which reads like a separate
kind and is not: it is a Zephyr entry for board `fvp-aemv8r-smp`.

Independently re-counted here, since the argument rests on it:

- **15 hand-written entry packages across 10 workspaces** — `c`, `cpp`,
  `derived-tiers-cpp`, `features` (3), `mixed`, `realtime-c`, `realtime-cpp`
  (2, incl. `fvp_entry`), `realtime-rust`, `rust` (2, incl.
  `zephyr_entry_robot1`), `safety`. Matches the study.
- **7 of the 15 are Rust** (the ones with a `src/lib.rs`); the rest are C/C++
  west applications whose entry package has no source file at all.

**Every one of them is boilerplate.** The Rust body, doc comments stripped, is
three lines in all seven:

```rust
#![no_std]
extern crate zephyr;
nros::main!(launch = "demo_bringup");
```

One correction to the study, and it strengthens the case rather than weakening
it: the seven are **not byte-identical**. The two-line SHELL (`#![no_std]` +
`extern crate zephyr;`) is identical in all seven; what varies is the
`nros::main!` argument — `"demo_bringup"`, `"demo_bringup:rust_params.launch.xml"`,
`"demo_bringup:rust_safety.launch.xml"`, and in `zephyr_entry_robot1` an
`args = [("host", "robot1")]` besides. That varying part is precisely what
`builder/entry.rs` derives from the image ("the `nros::main!(launch = …, args =
…)` line | derived from | the image"), so measuring the difference does not
find a customisation — it finds the generator's input.

The C/C++ entries differ only in project name and in which node packages they
`add_subdirectory`, both derivable from the bringup's `[[component]]` rows.
What is genuinely per-workspace is the `prj*.conf` Kconfig set and
`nano_ros_use_board(...)` — which is the same thing RFC-0065 D5 and
`west_application_dir` already say is not derivable, and which issue 1288
carries.

## So: no existing entry qualifies as the demo

This is the part that cannot be shortcut by relabelling. **None of the 15
customises anything.** Every one is the shape the generator emits, written by
hand because no generator existed yet. Pointing the future book tutorial at one
of them would document the generated shape twice and the escape hatch zero
times.

A customisation demo has to be **written as one**: an entry that does something
`nros::main!` alone does not, with a stated reason for owning it that a reader
can recognise in their own project. That is authoring work, not a migration
leftover, and it should be scoped as such.

## What this issue wants

1. **A worked example of a customised entry**, newly written — not a promoted
   survivor of the migration. It must show the take-ownership step
   (`nros materialize`), the thing the hand-written entry does that the
   generated one cannot, and what the user now owns (nothing regenerates it, so
   a change to the bringup no longer reaches it).
2. **A book tutorial for entry customisation.** The user asked for this
   explicitly. The existing pages
   (`book/src/user-guide/component-and-entry-pkg.md`,
   `book/src/getting-started/workspace-entry-pkg.md`) describe the entry
   package; none of them covers deciding to own one.

## Deliberately deferred behind the class-2 → generated-entry migration

Do not start this before issue 1288 lands. The demo would have to be written
against the hand-written west-application shape, which is exactly the shape that
migration replaces — so it would be rewritten on arrival, and in the interval it
would read as an endorsement of the thing being retired. Writing it after means
writing it against the shape a user will actually meet: a generated entry they
chose to take over.

The dependency is one-way. 1288 does not need this issue; this issue needs
1288's answer to "what does a generated Zephyr entry look like" before its first
sentence can be true.

## References

- RFC-0098 D9 (`docs/design/0098-generated-leaf-build-config.md`) — a workspace
  has no root build file; the cargo root is the generated entry itself.
- `packages/cli/nros-cli-core/src/builder/entry.rs` — the generator, and its
  header's measurement of what an entry is ("every Rust entry source is ≤ 6
  non-comment lines"). Its "Why `nros::main!` and not the expanded form"
  section is also the argument a tutorial has to reproduce: the macro keeps the
  derivation live, so materialising freezes it.
- `nros materialize` (RFC-0065 D5) — the escape hatch this issue is about.
- Issue 1288 — the eight Rust Zephyr entries (fifteen packages in all) still
  hand-written; the migration this defers behind.
