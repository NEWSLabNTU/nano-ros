---
id: 2470
title: "A migrated `nros-bench` workspace leaves `src/<pkg>/generated/` unignored, so `check-message-crate-identity` fails after codegen"
status: resolved
type: bug
severity: high
area: [build, testing, ci]
related: [0718, 0196]
found: 2026-11-12
resolved: 2026-11-12
resolved_in: "each bench workspace ignores `/src/*/generated/`; gated by `check-workspace-generated-ignored`"
---

> **Resolved (2026-11-12).** The three bench workspaces ignore
> `/src/*/generated/`, and `check-workspace-generated-ignored` (fast line)
> refuses any workspace package whose `generated/` git does not ignore. See
> "Fix".

## Measured

After `just _codegen`, `just ci gate` fails `fast::check::message-crate-identity`:

```
tracked generated message crate(s) shipping an UNPREFIXED name: builtin_interfaces, std_msgs, ...
```

The crates sit in untracked, unignored directories:

- `packages/testing/nros-bench/large-msg-baremetal/src/qemu_bsp_large_msg_test/generated/`
- `packages/testing/nros-bench/loan-copy-count/src/loan_copy_count/generated/`
- `packages/testing/nros-bench/wake-latency-cortex-m3/src/qemu_wcet_bench/generated/`

## Why

Codegen writes a package's message crates into `generated/` beside its
`package.xml`. phase-531 W7 moved these three programs into one-package
workspaces, so the package and its `generated/` moved one level down, to
`<ws>/src/<pkg>/`. Each leaf's `.gitignore` kept `/generated/`, which matches
only the old location.

The other migrated workspaces were covered by a rule above them. `examples/`
has the repo-root `examples/**/generated/` rule, and
`packages/testing/nros-tests/bins/.gitignore` has `/*/src/*/generated/`.
`packages/testing/nros-bench/.gitignore` has neither, so only the bench
workspaces were exposed. No gate asked whether a workspace package's
`generated/` is ignored; `check-example-leaf-build-dirs` (issue 0718) asks the
sibling question for build dirs, under `examples/` only, and only after a
build.

## Fix

- The three bench `.gitignore` files add `/src/*/generated/`.
- New gate `check-workspace-generated-ignored`
  (`scripts/check-workspace-generated-ignored.py`, fast line). For every tracked
  `.colcon_workspace` root and every tracked `<ws>/src/<pkg>/package.xml`, it
  asks `git check-ignore --no-index` about a path inside `<pkg>/generated/`.
  It is static, so it holds on a tree nothing has built, and it covers every
  workspace in the repository, not just `examples/`. Its selftest builds a
  repository with the W7 shape and expects one finding.

Sweep (the same question by hand; it named exactly the three bench workspaces):

```
for ws in $(git ls-files '*/.colcon_workspace' | xargs -n1 dirname); do
  for m in $(git ls-files "$ws/src/*/package.xml" | xargs -rn1 dirname); do
    git check-ignore -q --no-index "$m/generated/x/Cargo.toml" || echo "$m"
  done
done
```

Measured: before the fix the gate names the three bench packages and their
bringups (6 findings); after it, 315 workspace packages pass.
