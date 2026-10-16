---
id: 1830
title: "`check-template-copy-out` fails on two templates: a copied Rust package cannot resolve `nros`, which `nros sync` now refuses (issue 1817)"
status: resolved
type: bug
severity: medium
area: [templates, cli]
related: [1817, 1453, 1829]
found: 2026-10-12
---

## Measured

`just ci gate` on a branch based on main at `380648444d`, in `check::build`:

```
local-msg-package: FAIL — the copy does not build
    Error: rust_consumer::rust_consumer: cargo cannot resolve
    /tmp/nros-template-copy-out.b2vcHz/copy/examples/templates/local-msg-package/src/rust_consumer/Cargo.toml:
    error: no matching package named `nros` found
    This fails the image's own build too, so sync stops here rather than fall
    back to the SystemModel bound (issue 1817).
multi-package-workspace: FAIL — the copy does not build
    Error: pkg_rust_publisher::talker: cargo cannot resolve .../pkg_rust_publisher/Cargo.toml:
    error: no matching package named `nros` found
check-template-copy-out: FAILED — a template a user copies does not build.
```

The branch does not touch either template or the copy-out recipe.

## Why it is red now

Issue 1817 (`dbb9be70cb`) made `nros sync` STOP on a leaf whose `Cargo.toml`
cargo cannot resolve, where it used to print `done.` and exit 0. A copied
template's Rust package names `nros` by registry name, and outside the
checkout nothing patches it to a path, so cargo cannot resolve it. That was
already true; 1817 made it a failure instead of a silent pass. The gate is
right to fail.

`check-template-copy-out` is not merge-gating (issue 1453), which is why this
reached main without a red on any pull request.

## Fix direction

Give a copied template the same `nros` resolution an in-tree build gets:
the central patch include `nros sync` writes for a leaf, or a path pin the
copy recipe rewrites. Then the copy builds and the 1817 refusal never fires.
Not "make sync tolerant again": 1817's point is that a leaf cargo cannot
resolve fails the image's own build too.

## Resolution

Already fixed on main by `d6d6c7af2c` (issue 1829), before this issue was
filed. The measurement above was taken on a branch based on `380648444d`, and
1829's fix landed on main after that base but before this issue's filing
commit `0d35db7264`. No code change here.

The cause was not missing `nros` resolution in the copy. `nros sync` DID
write the copied leaf's `[patch.crates-io]`, with absolute rows for the
`nros`/`nros-core`/`nros-serdes` trio, because the leaf sits outside the
checkout. #1817's resolution check reused the metadata probe's
`probe_patch_config`, and that function gathered patch rows only from the
harness's ancestor chain. Both templates are colcon workspaces whose Rust
package is its own cargo workspace under `src/`, so the leaf's
`.cargo/config.toml` is a sibling of that chain, never an ancestor. The probe
therefore resolved without the patch. 1829 walks the component's own chain too
(unit test `the_probe_takes_a_sibling_leafs_patch_rows`, run by
`check-cli-tests`).

Measured with `scripts/check-template-copy-out.sh local-msg-package
multi-package-workspace` (in-tree CLI and launch resolver rebuilt at each
commit, `nano_ros_ROOT` set to that checkout as `just` does):

| commit | local-msg-package | multi-package-workspace |
| --- | --- | --- |
| `380648444d` (the issue's base) | FAIL: `no matching package named nros` | FAIL: same |
| `d6d6c7af2c~1` | FAIL | FAIL |
| `d6d6c7af2c` (fix 1829) | OK, 2 artifacts | OK, 3 artifacts |
| `0d35db7264` (this issue filed) | OK | OK |
| origin/main `474f0ea977`, `nano_ros_ROOT` unset | OK, 2 artifacts | OK, 3 artifacts |

Coverage: `check-template-copy-out.sh --list` names both templates as `[linux]`
projects, and the gate's own run builds them (the table's last row), so
neither template is silently skipped.
