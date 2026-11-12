---
id: 2481
title: "No recipe builds the ten mps2-an385 bare-metal C/C++ workspace rows, so every case of the bare-metal C API end-to-end test is unmet after `just build qemu`"
status: resolved
type: bug
severity: medium
area: [build, testing, baremetal]
found: 2026-11-13
related: [phase-535]
resolved_in: "fix-2481-qemu-ws-langs: qemu build-fixtures calls workspace-fixtures-build.sh baremetal with no language filter; gate check workspace-fixture-lang-reach holds the class"
---

## What

10d35dc20942e26e9547457769d31689011facc6 (phase-535 W6) turned the bare-metal
C and C++ listener, service and action examples into one-package workspaces.
Their ten `examples/fixtures.toml` rows are WORKSPACE rows now
(`baremetal-{c,cpp}-zenoh-{listener,service-server,service-client,action-server,action-client}`),
built by `scripts/build/workspace-fixtures-build.sh`, not by
`scripts/build/fixtures-build.sh`.

The qemu module's fixture build (`build-fixtures` in
`just/qemu-baremetal.just`, reached by `just build qemu`) was not moved with
them. It calls `workspace-fixtures-build.sh baremetal rust` only, and nothing
else in `just/` or `.github/` calls `workspace-fixtures-build.sh baremetal`
with `c`, `cpp` or no language filter.

Measured at c229eae0905762a106788efd4cbe9f2e9f31ab69 against the manifest:

| query | C/C++ bare-metal rows selected |
|---|---|
| `fixtures-manifest.py --platform baremetal --lang rust list-workspaces` (what the recipe runs) | 0 |
| `... --lang c list-workspaces` | 5 |
| `... --lang cpp list-workspaces` | 5 |
| `fixtures-manifest.py --platform baremetal --lang c --builder cargo list` (the recipe's `fixtures-build.sh baremetal c`) | 1: `c/talker` |
| same, `--lang cpp` | 1: `cpp/talker` |

So after `just build qemu`, only the two talkers exist.
`emulator::test_qemu_baremetal_capi_e2e` needs both images of each pair, and
the first image of every case (`c/listener`, `c/service-server`,
`c/action-server` and the C++ three) is a workspace row. All six cases are
`[UNMET PRECONDITION]`, the two pubsub cases included: their talker is built,
their listener is not.

When the ten rows are built by hand
(`workspace-fixtures-build.sh baremetal --id <id>`, 38 s to 56 s each), the
service and action cases pass (c_service, cpp_service, c_action, cpp_action,
measured on main 8cf84ff5b9f5c6eae307ae3bb166d86bf750110b). The defect is the
build lane, not the images.

## Why nothing caught it

The ten rows are in no lane's build, and no gate asks whether every
`[[workspace]]` row is reached by some recipe's call. Bare-metal is outside
tier 1, so a tier-1 run never asks for these fixtures. The nuttx module shows
the shape the qemu one lacks: `just/nuttx.just` calls
`workspace-fixtures-build.sh nuttx` once per language.

## Fix direction (not done here)

- Have the qemu `build-fixtures` recipe build the C and C++ workspace rows
  too: drop the `rust` filter, or add `c` and `cpp` calls as nuttx does.
- Consider a check that every workspace row's platform and language is
  reached by some recipe's `workspace-fixtures-build.sh` call, so the next
  migration cannot strand a row the same way.

## Evidence

- `just/qemu-baremetal.just`, recipe `build-fixtures`: the
  `workspace-fixtures-build.sh baremetal rust` line.
- The measurement table above (manifest queries, re-runnable from any
  checkout).

## Resolution (2026-11-13)

`just/qemu-baremetal.just` `build-fixtures` now runs
`workspace-fixtures-build.sh baremetal` with no language, so every
bare-metal workspace row (Rust, C, C++) is in the lane.

The class is held by a new fast-line gate, `check workspace-fixture-lang-reach`
(`scripts/check-workspace-fixture-lang-reach.py`): every `[[workspace_fixture]]`
(platform, lang) must be reached by a recipe call with no language or with that
language; a call narrowed by `--id` / `NROS_FIXTURE_ID` reaches one row and does
not count, and the west-lane platforms are out of scope. On the tree before the
fix it reported exactly `baremetal` `c` and `cpp` (5 rows each) and nothing
else; after, all 26 platform/lang pairs are reached. Its self-test carries the
shipped rust-only shape as a negative control.

Measured after `just qemu build-fixtures` in a fresh worktree:
`test_qemu_baremetal_capi_e2e` 6/6 and `test_qemu_bsp_pubsub_e2e` pass.
The rows themselves are unchanged; phase-553 W6 converts them to CMake packages.
