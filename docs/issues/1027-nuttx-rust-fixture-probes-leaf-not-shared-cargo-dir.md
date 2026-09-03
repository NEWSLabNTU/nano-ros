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

**CORRECTION (2026-09-04): that sentence was wrong.** FreeRTOS fixtures do NOT
land in the leaf — `freertos` IS in `NROS_FIXTURE_SHARED_PLATFORMS` and its
artifacts are in `build/cargo-fixtures/freertos/`. It was green because it
happened to spell the CARVE-OUT profile directly, which is the one on disk. So
it was one profile spelling away from the same failure, not one directory away.
Fixed with the others.

(`cache_key.rs`'s `target/nros-*` paths are a different thing — a scratch dir
under the project root, not a fixture artifact. Leave them.)

## Not covered

Zephyr and ThreadX locators, which resolve differently (west leaves,
`librustapp.d`) and were not part of this sweep.

## Fix (2026-09-04, branch `fix/1007-1026-1027-followups`)

All five sites in the sweep are addressed, and the two NuttX literals the issue
flagged (the leaf `target/` prefix, and "the carve-out is the profile on disk")
are both gone.

**One derivation added.** `binaries/mod.rs::row_profile_dir(row)` — the profile
DIRECTORY a row's platform builds into, from `nros_cargo_profile::
platform_profile(row.coord.0)` (the Rust twin of `nros_cargo_platform_profile`).
It is deliberately the same derivation `rel_at_row_profile` already applies
inside the row-keyed chokepoint, exposed so a resolver can ask "which profile is
on disk?" BEFORE it hands a path over — which the NuttX carve-out/ambient choice
needs.

**`binaries/nuttx.rs`** — `build_rust_example` (sites `:201`/`:227`) is DELETED.
It was dead (`#[allow(dead_code)]`, kept alive only by a `_keep_` shim) and it
was a second spelling of the same artifact path, which is how the fallback arm
came to look in a place nothing writes. `require_entry_binary` (sites
`:308`/`:313`) is now the one resolver, and it asks the manifest:
`groups::select_sole_row(dir)` → `groups::row_resolved_dir(row)` for the artifact
ROOT (leaf or group, decided by the row's own `shared`/`slug`) and
`row_profile_dir(row)` for the PROFILE. The only literal left is the target
triple, which no `GroupRow` carries.

**The miscompile warning is kept and now means what it says.** Its condition is
three-part: the platform's carve-out differs from the ambient profile, the
carve-out artifact is ABSENT, and the ambient artifact is PRESENT. "Carve-out
absent" alone is also what an unbuilt tree looks like — warning about a codegen
bug there is the cry-wolf this issue measured. The ambient arm resolves through
the PATH route on purpose: `rel_at_row_profile` rewrites a rel's profile
component to the carve-out, which is exactly what that arm must not do, while
`groups::resolved` redirects the artifact root only.

**`binaries/freertos.rs`** (site `:106`) moved with them, to
`select_sole_row` + `require_prebuilt_row_binary_fresh` + `row_profile_dir`.
Note the issue's reading of why FreeRTOS was green is inverted: MEASURED, its
artifacts are NOT in the leaf — `freertos` is in
`NROS_FIXTURE_SHARED_PLATFORMS` and they are under
`build/cargo-fixtures/freertos/thumbv7m-none-eabi/nros-minsizerel/`. It reached
them because it spelled the carve-out profile DIRECTLY and let the path route's
root redirect do the rest. So it was one profile spelling away from the NuttX
failure, not one lane away.

## Measured

Same tree, same fixtures, `just nuttx build-fixtures-arm` already run:

| | before | after |
| --- | --- | --- |
| `rtos_e2e` NuttX Rust cells (pubsub/service/action) | 0 passed, 3 failed | **3 passed, 0 failed** |
| `rtos_e2e` FreeRTOS Rust cells | 3 passed | 3 passed (no regression) |

The three failures all read
`not prebuilt: build/cargo-fixtures/nuttx-<slug>/armv7a-nuttx-eabihf/nros-relwithdebinfo/<bin>`
— the root redirect had already worked; only the profile component was wrong.

**The warning was demonstrated, not asserted.** A real ambient-profile build
(`NROS_CARGO_PROFILE=nros-relwithdebinfo bash scripts/build/fixtures-build.sh
nuttx rust`) was run, then the carve-out `talker`/`listener` were moved aside so
the ambient artifact was the only one present. The cell resolved and ran, with:

    [nros-tests] WARNING: NuttX Rust fixture `talker` is present at the ambient
    `nros-relwithdebinfo` profile but NOT at the `nros-minsizerel` carve-out
    (…/armv7a-nuttx-eabihf/nros-minsizerel/talker); running the ambient build,
    which hits the 177.8.c armv7a-nuttx-eabihf codegen bug …

The carve-out artifacts and the ambient build output were both restored/removed
afterwards. (Incidentally: that ambient image PASSED the pubsub cell on this
host — the 177.8.c miscompile is non-deterministic, which is why the warning
rather than a hard refusal.)
