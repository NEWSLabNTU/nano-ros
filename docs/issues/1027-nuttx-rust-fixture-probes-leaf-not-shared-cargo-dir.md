---
id: 1027
title: "NuttX Rust fixtures resolve at a LEAF `target/` that phase-340 moved, so
  a freshly built image reports `not prebuilt`"
status: open
type: bug
area: testing, build
severity: high
found: 2026-09-04
related: [issue-0393, issue-0488, phase-340, phase-414]
---

## What happens

`binaries/nuttx.rs::build_rust_example` probes the carve-out profile at a LEAF
path:

```rust
let carve_out = nros_cargo_profile::target_dir(nros_cargo_profile::NUTTX_RUST_PROFILE);
let release_binary_path = example_dir.join(format!(
    "target/armv7a-nuttx-eabihf/{}/{}", carve_out, binary_name
));
```

Under phase-340's shared cargo dirs that path does not exist. MEASURED on this
tree, after a clean `just nuttx build-fixtures-arm`:

    examples/qemu-arm-nuttx/rust/talker/target        <- ABSENT
    build/cargo-fixtures/nuttx-2162892711/armv7a-nuttx-eabihf/
        nros-minsizerel/talker                        <- freshly built

The probe misses, falls through to the ambient-profile arm, and the cell
reports `not prebuilt: .../nros-relwithdebinfo/talker` while the real artifact
sits beside it under a different profile directory.

## Why it is worse than a missing file

The fallback exists to warn about the **phase-177.8.c CGU miscompile** — an
`lto = "off"` NuttX Rust image reboot-loops before `main` with zero console
output. So the branch that fires here is the one whose whole job is to say "you
are about to exercise the known-broken profile". It now fires for a reason that
has nothing to do with the profile, on a tree where the correct artifact was
just built. A warning that cries wolf about a real miscompile is worse than no
warning.

## The class

CLAUDE.md states the rule this broke, in its phase-340 entry:

> Give such a build a row (preferred) or derive its dir from
> `nros_fixture_target_dir_flag` + `nros_fixture_row_artifact_dir` — **never a
> literal, and move the test-side locator in the SAME commit** (#393).

The build side moved to the shared group dir; this locator did not. Same shape
as issue 0393, and a cousin of 0488 (a platform's fixture profile must be read
through `nros_cargo_platform_profile`, or the probe looks in a second profile
dir and reports permanent false-STALE).

Two literals are involved and both are suspect: the leaf `target/` prefix, and
the assumption that the carve-out profile is the one on disk. The artifact
found here is at `nros-minsizerel`, while the probe's fallback message names
`nros-relwithdebinfo`.

## Direction

Resolve the path the way the manifest already knows how, rather than
constructing it:

1. `nros_fixture_row_artifact_dir` / `row_artifact_root()` — the same helper the
   lane resolver uses to attribute an artifact back to its manifest row. That
   makes build-set and run-set one predicate on one coordinate file, which is
   the property #393 was fixed to have.
2. Read the platform's profile through `nros_cargo_platform_profile` rather than
   assuming the carve-out, so the 0488 half cannot recur.

Keep the miscompile warning — it is guarding a real defect — but make it fire on
"the artifact is at the wrong PROFILE", which is what it means, rather than on
"the artifact is not where I looked".

## Acceptance

After `just nuttx build-fixtures-arm`, the NuttX Rust `rtos_e2e` cells resolve
their fixtures and run. A deliberately ambient-profile build still triggers the
miscompile warning.

## The sweep, done — it is a class, not a site

`rg '"target/' packages/testing/nros-tests/src/fixtures/`:

| site | note |
| --- | --- |
| `binaries/nuttx.rs:201` | the measured one (carve-out probe) |
| `binaries/nuttx.rs:227` | the ambient fallback — same leaf prefix, so BOTH arms miss |
| `binaries/nuttx.rs:308`, `:313` | a second pair, same shape |
| `binaries/freertos.rs:106` | `target/thumbv7m-none-eabi/{profile}/{bin}` — the same leaf assumption on another platform |

So NuttX has FOUR sites and FreeRTOS one, and fixing only the reported line
would leave the fallback arm looking in the same wrong place — which is how the
warning would keep firing for the wrong reason.

FreeRTOS is not currently RED here only because those fixtures happen to build
into the leaf on this tree; that is a property of which lane last ran, not of the
locator being right. It should move with the others.

(`cache_key.rs`'s `target/nros-*` paths are a different thing — a scratch dir
under the project root, not a fixture artifact. Leave them.)

## Not covered

Zephyr and ThreadX locators, which resolve differently (west leaves,
`librustapp.d`) and were not part of this sweep.
