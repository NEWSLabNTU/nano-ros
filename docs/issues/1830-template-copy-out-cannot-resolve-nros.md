---
id: 1830
title: "`check-template-copy-out` fails on two templates: a copied Rust package cannot resolve `nros`, which `nros sync` now refuses (issue 1817)"
status: open
type: bug
severity: medium
area: [templates, cli]
related: [1817, 1453]
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
