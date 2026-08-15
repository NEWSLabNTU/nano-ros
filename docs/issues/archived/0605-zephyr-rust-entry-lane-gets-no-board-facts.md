---
id: 605
title: "phase-351 W5's Zephyr arm is inert for the RUST entry lane — its cargo is
  spawned by zephyr-lang-rust, which the hook never reaches"
status: resolved
type: bug
area: build
related: [issue-0590, issue-0529, issue-0460, issue-0571, phase-351]
---

## Symptom

phase-351 W5 delivers the board rung + site config to every lane that spawns
cargo. The Zephyr arm was wired into `zephyr/cmake/nros_cargo_build.cmake` and
committed as "unverified on this host" (the lane skipped: `west not found`).
With the workspace provisioned, it is now verified — as NOT working:

```
$ grep -c "nano-ros: board facts" <zephyr build log>
0
```

A full configure of `build-rust-talker-cyclonedds` prints **no** `nano-ros:`
status line at all, neither the delivery nor any of the three "NOT delivered"
reasons the helper now prints.

## Cause

`nros_cargo_build()` builds the CORE crates (`nros-c`, `nros-cpp`, the workspace
runtime). A Zephyr **Rust entry** is not built through it: `nano_ros_entry` is
the C/C++ analog of zephyr-lang-rust's `rust_cargo_application()`, and that
function builds its own cargo command inside the Zephyr module. The hook is on
a path the rust cells never take.

`set(ENV{…})` is not the answer — issue 0460 measured that it reaches only the
configure-time process, which is exactly why this wave puts values on the
build-time command.

## Not yet known

Whether the C/C++ Zephyr cells DO get them. Their configure never ran: issue
0590 fails the cyclonedds cells and the lane stops before reaching
`build-c-*` / `build-cpp-*`. So the arm may be half-working, and that must be
measured rather than assumed — the whole point of this wave is that a value
which does not arrive says nothing.

## Why the gate did not catch it

`check-board-facts-delivery` was written against `corrosion_import_crate()`, and
the Zephyr lane uses no Corrosion. It has since been widened to any file that
builds a `cmake -E env … cargo` command (which is how the Zephyr lane was
brought in at all, and it immediately found two more lanes). It still cannot see
a cargo invocation that belongs to a THIRD-PARTY module — `rust_cargo_application`
is zephyr-lang-rust's, not ours. A gate over our own files cannot cover it; the
delivery has to be arranged where we call that function.

## RESOLVED 2026-08-16

Three parts, because the lane needed all three:

1. **The module resolves once, at module scope.** `zephyr/CMakeLists.txt` calls
   `nros_resolve_board_facts()` when the nros Zephyr module configures, so the
   cache entry exists before either consumer runs — this module's own
   `nros_cargo_build()` (C/C++ core crates) and zephyr-lang-rust's
   `rust_cargo_application()`. It resolves FROM `APPLICATION_SOURCE_DIR`, the
   entry leaf.
2. **The values reach zephyr-lang-rust's command.** That cargo invocation is
   built inside the module tree, so `scripts/zephyr/cargo-features-patch.sh`
   gains a fourth hunk injecting `${NROS_BOARD_FACTS_ENV}` — BEFORE `cargo`,
   because `cmake -E env` ends the environment at the first non-`KEY=VALUE`
   argument (placing it with the other pass-throughs would have made cargo read
   `NROS_BOARD=…` as a subcommand). Idempotent, and it FAILS LOUDLY if the
   upstream layout ever moves that line, rather than silently leaving the lane
   without its rung again.
3. **The leaf shape the Zephyr examples actually use.** `nros ws board-facts`
   required `[package.metadata.nros.entry] deploy`; those examples carry only
   `[package.metadata.nros.deploy.<key>]`. A single such table is now accepted
   (the key IS the board); several without an `entry` stanza still require
   `--deploy`, because that is a question for the caller, not a guess.

*Verified end to end,* not just by the configure message:

```
-- nano-ros: board facts from …/examples/zephyr/rust/service-server — 2 value(s) delivered to cargo

$ grep -o 'NROS_BOARD[A-Z_]*=[^ ]*' zephyr-workspace/build-rust-talker-zenoh/build.ninja
NROS_BOARD_TOML=…/packages/boards/zephyr/nros-board.toml
NROS_BOARD=zephyr
```

The values are in the generated ninja command, which is what the wave is about —
`set(ENV{})` would not have been (issue 0460).

## Still unmeasured

Whether the C/C++ Zephyr cells print the same line. The mechanism is shared (the
module-scope resolve plus `nros_cargo_build`'s existing hook), but issue 0590
still fails the cyclonedds cells and the lane stops before `build-c-*` /
`build-cpp-*` ever configure. Stated rather than assumed: a value that has not
been seen arriving has not been shown to arrive.

## Where the fix went

`nano_ros_entry()` / the entry `CMakeLists` is the seam that calls into
zephyr-lang-rust, so that is where the facts must be attached — either by
setting them on the target zephyr-lang-rust creates, or by exporting them into
the command it builds. Whichever it is, the acceptance is the same as the rest
of W5: the line appears in the configure output, and a board-bound build that
receives nothing says so.
