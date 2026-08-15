---
id: 605
title: "phase-351 W5's Zephyr arm is inert for the RUST entry lane — its cargo is
  spawned by zephyr-lang-rust, which the hook never reaches"
status: open
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

## Where the fix goes

`nano_ros_entry()` / the entry `CMakeLists` is the seam that calls into
zephyr-lang-rust, so that is where the facts must be attached — either by
setting them on the target zephyr-lang-rust creates, or by exporting them into
the command it builds. Whichever it is, the acceptance is the same as the rest
of W5: the line appears in the configure output, and a board-bound build that
receives nothing says so.
