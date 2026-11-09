---
id: 2370
title: "A copied-out or scaffolded project builds on rustup's default toolchain, not the pin `nros setup` provisions, on the cargo and west roads"
status: resolved
type: bug
area: [cli, build, toolchain, zephyr]
severity: medium
found: 2026-11-09
resolved: 2026-11-09
resolved_in: "board_rust_channel pins a project with no rustup override to the installed pin on the cargo road; nros_cargo_build.cmake does the same on the west road"
related: [phase-548, issue-1914]
---

**Home: [phase-548](../../roadmap/phase-548-three-languages-every-board.md)**, the
sibling of W9. W9 (#3321) fixed the CMake road's `rust-lld`/`llvm-ar` lookup and
made `nros setup` install a "rustup default" need on the pin as well. This
issue covers the two roads W9 did not reach.

## What

On host arm-a100, `ci gate` failed
`template-copy-out#scaffold:mps2-an385-baremetal` with "linker `rust-lld` not
found". The copied-out project has no `rust-toolchain.toml` above it, so the
cargo that `nros build` runs uses rustup's default toolchain. Here that was
`stable` 1.98.1, whose manifest lists `rust-lld` while the file is absent on
disk. With `RUSTUP_TOOLCHAIN=1.99.0` (the pin) the same job is green.

The class: a toolchain is chosen per road, and only one road chose the pin.

| road | before | where |
| --- | --- | --- |
| cmake (Corrosion and nano-ros's own cargo commands) | pins when rustup has no override for the source dir (issue 1914) | `cmake/NanoRosRustTool.cmake` `nros_pin_default_rust_toolchain` |
| cmake sysroot lookup (`rust-lld`, `llvm-ar`) | asked rustup's default | fixed by phase-548 W9 |
| cargo (`nros build`, a leaf or a generated entry) | the pinned nightly for a `toolchain = "nightly"` board, else **whatever rustup picks** | `builder::preflight::board_rust_channel` |
| west (Zephyr C lane's cargo) | the pinned nightly for a build-std target, else **whatever rustup picks** | `zephyr/cmake/nros_cargo_build.cmake` |

The cargo road's preflight asked the same unpinned toolchain whether a board's
target was installed, so it agreed with the build and caught nothing.

## Fix

The rule is issue 1914's, now applied to the remaining roads. A project with
no `RUSTUP_TOOLCHAIN` and no rustup override (`rustup show active-toolchain`
says `(default)`) builds on the pinned toolchain if it is installed. Nothing
changes in-tree, because the root `rust-toolchain.toml` is an override there.

- cargo: `board_rust_channel` takes the project directory and returns the pin
  through `default_pin_for` for a non-nightly board. `nros build`'s cargo
  handoffs already carry the channel as `RUSTUP_TOOLCHAIN`. Preflight's target
  check asks that toolchain, and its remedy names `--toolchain <pin>`.
- west: the non-build-std branch calls `nros_pin_default_rust_toolchain()` and
  puts `nros_rust_toolchain_env` on the cargo command.

## Done when

- A project outside the checkout builds on the pin under a private
  `RUSTUP_HOME` whose default lacks `rust-lld`, and preflight names the pin
  when the pin lacks the board's target.
- `template-copy-out` is green on a host whose default toolchain is not the pin.

## Resolution (2026-11-09)

Fixed as described above, on both roads, by the rule issue 1914 set for CMake.

Reproduced on a private `RUSTUP_HOME`. Its default toolchain, `brokendef`, is a
copy of 1.99.0 with `rust-lld` deleted; its 1.99.0 is the real pin. On that
home:

- `NROS_GATE_UNIT=scaffold:mps2-an385-baremetal scripts/check-template-copy-out.sh`
  passes with the fix.
- Forcing the old choice with `RUSTUP_TOOLCHAIN=brokendef` fails with
  `lld-wrapper: could not exec rust-lld`, which is the arm-a100 failure.

Unit tests: `the_pinned_stable_is_the_root_rust_toolchain_channel`, and
`a_project_with_no_override_builds_on_the_installed_pin`, which covers the
default, no-default, override and not-installed cases.

Not measured: a Zephyr app copied out of the checkout. The west change is the
same CMake function the cmake road has run since issue 1914, and inside the
checkout it is a no-op, because the root `rust-toolchain.toml` is an override
there.
