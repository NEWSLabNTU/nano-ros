---
id: 1254
title: "The Zephyr SDK is provisioned INSIDE the nano-ros checkout, not in the
  store -- so a downstream workspace binds to whichever clone ran setup"
status: open
type: bug
area: zephyr, tooling
severity: medium
related: [issue-1248]
---

## Symptom

A downstream project (Autoware Safety Island) whose Zephyr 4.4 workspace was
made by `NROS_ZEPHYR_VERSION=4.4 just zephyr setup` carries this in the
workspace's generated `env.sh`:

```
export ZEPHYR_SDK_INSTALL_DIR="/home/aeon/repos/nano-ros/scripts/zephyr/sdk/zephyr-sdk-1.0.1"
```

That path is a SIBLING nano-ros clone, not the submodule the project pins.
Every board build of that project has depended on a second checkout existing,
on its `scripts/zephyr/sdk/` never being cleaned, and on nobody running
`just clean-setup` there (`justfile:5075` removes `scripts/zephyr/sdk`). Nothing
reports the dependency; it surfaces only when the other clone goes away.

## Cause

RFC-0095 D2 draws the store as

```
$NROS_STORE/
|-- sdk/zephyr-sdk/0.16.8/         # unchanged from today
```

which is not what the tree does. `scripts/zephyr/setup.sh:73` sets

```
SDK_INSTALL_DIR="$SCRIPT_DIR/sdk"
```

and passes it to `nros setup --tool "$ZEPHYR_SDK_TOOL" --prefix
"$SDK_INSTALL_DIR"`, so the fetch goes through the index (issue 0610) but the
INSTALL lands in the checkout. `--prefix` is the out-of-store escape hatch
(`cmd/setup.rs:757`: "places it outside the shared store"), and a prefix
install is not recorded in `nros-sdk.lock` (`cmd/setup.rs:871`), so
`nros sdk-path zephyr-sdk-1-0-1` cannot find it either.

Without `--prefix` the same command installs where RFC-0095 says it should.
Measured 2026-09-10:

```
$ nros setup --tool zephyr-sdk-1-0-1
nros setup --tool zephyr-sdk-1-0-1: prebuilt 1.0.1 (dist linux-x86_64) -> ~/.nros/sdk/zephyr-sdk-1-0-1/1.0.1
```

with the SDK one level down, at `.../1.0.1/zephyr-sdk-1.0.1/` (the tarball has
a top-level directory and is unpacked without `--strip-components`). That extra
level is what `ZEPHYR_SDK_INSTALL_DIR` must name, and is the one thing a
consumer cannot get from `nros sdk-path` alone today.

## Every reader of the checkout path

The workspace resolver moved to the store in phase-440 W4; the SDK did not, so
these still construct or search the checkout-relative path (sweep:
`git grep -n -E 'scripts/zephyr/sdk|SCRIPT_DIR/sdk'`):

| site | what it does |
| --- | --- |
| `scripts/zephyr/setup.sh:73` | the install target |
| `just/zephyr-setup.just:169,321` | `sdk="$(pwd)/scripts/zephyr/sdk/zephyr-sdk-0.16.8"` |
| `just/zephyr-setup.just:280,435` | `export ZEPHYR_SDK_INSTALL_DIR="$nros_root/scripts/zephyr/sdk/zephyr-sdk-0.16.8"` |
| `packages/testing/nros-tests/src/zephyr.rs:701` | searches `scripts/zephyr/sdk/zephyr-sdk-*` after `ZEPHYR_SDK_INSTALL_DIR` |
| `scripts/ci/runner-doctor.sh:198` | "checkout default" arm of the SDK ladder |
| `scripts/ci/runner-sweep.sh:607` | reclaims `scripts/zephyr/sdk` |
| `justfile:5075` | `clean-setup` removes it |

The 0.16.8 literals are also a second copy of the per-line SDK version that
`setup.sh`'s `case "$MANIFEST"` already owns.

## Fix shape

One resolver, same as the workspace (RFC-0095 D4): the store arm first, the
checkout arm last so a host provisioned earlier keeps working.

- `setup.sh` installs into the store (drop `--prefix`) and writes the resolved
  SDK dir into `env.sh`.
- Consumers ask for the SDK dir instead of constructing it. Either
  `nros sdk-path` learns the tarball's inner directory, or a helper beside
  `scripts/lib/zephyr-workspace.sh` answers "the SDK for Zephyr line V" from the
  same `case` that picks the version today.
- The test harness and runner doctor read that answer rather than globbing.

## Acceptance

- A fresh `NROS_ZEPHYR_VERSION=4.4 just zephyr setup` writes nothing under
  `scripts/zephyr/sdk/` and its `env.sh` names a store path.
- `nros store list` shows the SDK and `nros store gc` respects it.
- A host with only the legacy `scripts/zephyr/sdk/` still builds (checkout arm).
