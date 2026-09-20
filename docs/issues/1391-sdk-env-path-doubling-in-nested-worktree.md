---
id: 1391
title: "`just/sdk-env.just`'s prefix rewrite DOUBLES every defaulted path in a
  worktree nested inside the checkout it re-roots from, and the failure names a
  missing header instead of the env"
status: open
type: bug
area: build, tooling
severity: medium
related: [issue-1280, issue-1336]
found: 2026-09-20
---

## Measured

Run in an agent worktree at `<repo>/.claude/worktrees/<id>`, on `811f081a3`:

```
$ cd /home/aeon/repos/nano-ros/.claude/worktrees/agent-<id>
$ bash scripts/lib/foreign-checkout-root.sh "$PWD"
/home/aeon/repos/nano-ros

$ just --evaluate NROS_PLATFORM_CFFI_INCLUDE
/home/aeon/repos/nano-ros/.claude/worktrees/agent-<id>/.claude/worktrees/agent-<id>/packages/platform/nros-platform-api/include
```

The same variable in the main checkout is correct
(`/home/aeon/repos/nano-ros/packages/platform/nros-platform-api/include`).
The worktree segment appears TWICE.

## Why

`just/sdk-env.just` re-roots inherited absolute SDK paths (issue 1280) with a
prefix rewrite, ~20 lines of the shape:

```just
export NROS_PLATFORM_CFFI_INCLUDE := replace(
    env("NROS_PLATFORM_CFFI_INCLUDE", _NROS_HERE / "packages/platform/nros-platform-api/include"),
    _NROS_OTHER, _NROS_HERE)
```

`_NROS_OTHER` is the *other* checkout, from `foreign-checkout-root.sh`; the
comment at line 36 says rewriting `_NROS_HERE` to itself is "the no-op that lets
every line below be unconditional". That holds only when the two are DISJOINT.
An agent worktree lives INSIDE the checkout it re-roots from, so `_NROS_OTHER`
(`/…/nano-ros`) is a strict PREFIX of `_NROS_HERE`
(`/…/nano-ros/.claude/worktrees/<id>`) — and the rewrite is applied to the
DEFAULT too, which is already rooted at `_NROS_HERE`. So the prefix matches
inside a correct path and expands it: `<HERE>/x` becomes `<HERE>/<rel>/x`.

`replace` is unconditional and textual; it cannot tell "this value came from a
foreign checkout" from "this value is already mine".

## What it costs

Every build in such a worktree dies on a path that does not exist, and the
message names a source file rather than the environment:

```
cc1: fatal error: /…/agent-<id>/.claude/worktrees/agent-<id>/packages/platform/
     nros-platform-posix/src/platform.c: No such file or directory
fatal error: nros/platform.h: No such file or directory
```

Two agent sessions hit this independently on 2026-09-18 while fixing issues 1307
and 1039; each lost a full lane run to it, and each first read it as a NuttX
build break. It makes `just test-unit` and `just check build` unrunnable in
every `.claude/worktrees/*` worktree, which is exactly where parallel agent work
happens.

Workaround: `just --set _NROS_OTHER "$PWD" …`, which makes every `replace` the
documented no-op.

## The other spelling of this rule is already correct

`scripts/lib/checkout-paths.sh`'s `nros_reroot_checkout_path` resolves the
value's OWNING checkout and keeps the value when that owner is `here`, so it is
prefix-safe. Two spellings of one rule, and only the shell one is right — the
class CLAUDE.md names ("add ONE shared helper rather than a second spelling").

## Fix

Make the just side ask the same question the shell side asks: rewrite only when
the value belongs to the other checkout, and never when it is already rooted at
`_NROS_HERE`. Options: refuse the rewrite when `_NROS_OTHER` is a prefix of
`_NROS_HERE` (this case), or route the values through the existing shell helper
rather than re-deriving the rule in `just`. Prefer the second — it is the shared
helper the rule already has.

## Acceptance

In a worktree nested inside the checkout, `just --evaluate` of each rewritten
variable names a path that EXISTS, and `just test-unit` runs. A test must fail
against today's rewrite: assert that no evaluated path contains the worktree's
relative segment twice.
