---
id: 2515
title: "The CLI's checkout-ownership guard compares path TEXT, so a bind-mount alias of the same checkout reads as a foreign binary"
status: open
type: bug
severity: medium
area: [cli, testing, dev-env]
related: [1280, 1248]
found: 2026-11-14
---

**Found by** `just ci gate` in the ros2 distrobox on 2026-11-14. Its
`test-lane-contracts` step failed 8 `orchestration_*` tests:
`orchestration_composable`, `orchestration_e2e`, `orchestration_conditionals`
(2), `orchestration_includes` and `orchestration_set_remap_env` (3). Each one
fails because `nros plan` refuses to run:

```
Error: this `nros` does not belong to the checkout it is being run against.
    running: /run/host/mnt/wd/data/projects/nano-ros-box2/.claude/worktrees/p546v3/packages/cli/target/release/nros
    checkout: /mnt/wd/data/projects/nano-ros-box2/.claude/worktrees/p546v3
```

The same tests pass when run alone with `cargo nextest run -p nros-tests
--test orchestration_composable`. Running `nros plan` by hand with the same
arguments also succeeds.

## The two paths are one file

A distrobox mounts the host root at `/run/host`. This host's data disk is also
bind-mounted at `/mnt/wd` inside the box. Measured inside the box:

```
$ stat -c '%d:%i %n' $W /run/host$W $W/packages/cli/target/release/nros /run/host$W/packages/cli/target/release/nros
51:77625181  /mnt/wd/.../p546v3
51:77625181  /run/host/mnt/wd/.../p546v3
51:113204273 /mnt/wd/.../p546v3/packages/cli/target/release/nros
51:113204273 /run/host/mnt/wd/.../p546v3/packages/cli/target/release/nros
$ findmnt -n -o SOURCE,TARGET -T /mnt/wd           # /dev/sdd1 /mnt/wd
$ findmnt -n -o SOURCE,TARGET -T /run/host/mnt/wd  # /dev/sdd1 /run/host/mnt/wd
```

So the binary does belong to the checkout. This is the same binary in the
same checkout, built where it runs, which is what issue 1248's rule asks for.
It is not a setup mistake.

## Defect

`refuse_if_foreign_to_workspace` (`packages/cli/nros-cli-core/src/stale_guard.rs`)
canonicalizes both paths and then compares them with `exe_real.starts_with(&expected_real)`.
`canonicalize` resolves symlinks. It does not resolve bind mounts, because two
mount points of the same filesystem are both real paths. So the comparison is
between two strings, and two spellings of one directory compare unequal.

Fix: decide ownership by file identity, not by path text. Walk up from the
exe's parent directories and check whether any of them has the same
`(st_dev, st_ino)` as `<checkout>/packages/cli/target`. Do the same for
`checkout_root_of`, and check the class: any other `starts_with` taken after a
`canonicalize` that is used to decide whether something is "this checkout" has
the same flaw. Sweep:

    git grep -nE 'canonicalize\(\).*' -- 'packages/cli/**/*.rs' | grep -n starts_with
    git grep -nE '\.starts_with\(&?(expected|root|ws_root|checkout)' -- '*.rs'

## Open question

Why only the recipe path gets the `/run/host` spelling is not explained yet.
The exe path comes from `nros_tests::nros_cli_bin_path()`, which builds it
from `project_root()`. That function is `env!("CARGO_MANIFEST_DIR")` baked in
at compile time. So the `nros-tests` binaries were compiled by a cargo that
saw the checkout through `/run/host`. This was still true after
`cargo clean -p nros-tests` and a rebuild by the same recipe. The box's
environment carries `OLDPWD=/run/host/...`, but no `PATH` or `CARGO_*` value
under `/run/host` was found. One lead: in the same box, a `pre-push`
`check fast` run had cargo report
`cannot update the lock file /run/host/mnt/wd/.../packages/cli/nros-launch-resolve/Cargo.lock`
for a leaf the recipe had reached as `/mnt/wd/...`. So at least one `just`
recipe hands cargo a `/run/host` path, and the alias is not limited to the
test build. Fixing the guard makes the spelling harmless.
Finding where the spelling comes from is still worth doing, because the same
spelling feeds issue 1280's re-rooting rule, which also works on path text.

## Acceptance

- A unit test in `stale_guard.rs` for the identity helper: an exe under the
  checkout's target directory is accepted, and an exe under a sibling
  checkout is still refused. Without privileges a test cannot create a bind
  mount, so the alias case itself is covered by the box run below.
- `just ci gate` in the ros2 distrobox: `test-lane-contracts` has no
  `orchestration_*` failures.
