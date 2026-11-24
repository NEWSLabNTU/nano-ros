---
id: 2925
title: "`NROS_ZEPHYR_BUILD_ROOT` drops the Zephyr line from west build dirs, so the 3.7 and 4.4 lines share one directory per leaf"
status: resolved
type: bug
area: [zephyr, build]
severity: medium
found: 2026-11-24
resolved: 2026-11-24
related: [issue-1596, issue-2192, issue-2916, issue-2921, rfc-0070]
---

## What happens

The Zephyr west build root is keyed by line, so the two lines' leaves, which
have the same names, never meet:
`<build_root>/zephyr-workspace-builds/<version>` (issue 1596), and the same for
west fixtures (issue 2192). `NROS_ZEPHYR_BUILD_ROOT` was taken "as is", so
setting it dropped the line. Host 241 sets it
(`/tmp/nros-build-aeon/zephyr`). There a 3.7 and a 4.4 build of
`build-ws-rust-talker-clock-sntp-zenoh` used one directory, and the second
configure inherited the first's `ZEPHYR_BASE`. Issue 2916 refuses that
crossing; this keeps the crossing from happening.

## Why this is the fix, not a design change

RFC-0070 describes the knob as a relocation: the Zephyr west dirs are "the
`zephyr-workspace-builds/<version>` kind under this root, with
`NROS_ZEPHYR_BUILD_ROOT` kept as their own override". AGENTS.md says the same
("move the whole cache tree to a faster or larger volume"). Moving the
directory was the intent. Dropping the line was a side effect of "as is".

## Resolution

Fixed 2026-11-24. `nros_zephyr_build_root` (`scripts/lib/zephyr-workspace.sh`)
and its Rust twin `nros_tests::zephyr::zephyr_build_root` now answer
`$NROS_ZEPHYR_BUILD_ROOT/<version>`. `build_root_derivation.sh` pins both:

- the override with a trailing slash gives `/z/builds/3.7`;
- with `--version 4.4` it gives `/z/builds/4.4`;
- the Rust twin joins the line.

On a host that sets the knob, existing build dirs sit one level up. The first
build after this configures fresh under `<root>/<version>/`. The old dirs are
left for `runner-sweep` to evict, since it measures the parent directory.

## Left open

The knob is still not scoped by checkout. Every worktree on host 241 shares
`/tmp/nros-build-aeon/zephyr/<version>`, which is the crossing issue 1596
removed from the default root. RFC-0070 scopes `NROS_BUILD_ROOT` per checkout
(issue 1921), but not this knob. Whether it should be scoped the same way is a
question about what an operator who sets it wants: one shared cache, or one
volume. That decision is the operator's and is not taken here.
`check-zephyr-workspace-foreign-checkout` still reports a crossed dir under it.
