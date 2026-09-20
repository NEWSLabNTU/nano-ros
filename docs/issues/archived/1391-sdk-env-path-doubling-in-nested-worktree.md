---
id: 1391
title: "`just/sdk-env.just`'s prefix rewrite DOUBLES every defaulted path in a
  worktree nested inside the checkout it re-roots from, and the failure names a
  missing header instead of the env"
status: resolved
type: bug
area: build, tooling
severity: medium
related: [issue-1280, issue-1336, issue-1039, issue-0196]
found: 2026-09-20
resolved: 2026-09-20
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

## Reproduction, before the fix

In `.claude/worktrees/agent-a66bce40c1e033271`, on `811f081a3`, with **no
`--set` workaround**. All 25 exports evaluated; the nine that doubled are
exactly the nine whose environment variable was UNSET, so the DEFAULT arm won:

```
NROS_PLATFORM_CFFI_INCLUDE = DOUBLED MISSING= …/agent-…/.claude/worktrees/agent-…/packages/platform/nros-platform-api/include
NROS_PLATFORM_FREERTOS_SRC = DOUBLED MISSING= …/packages/platform/nros-platform-freertos/src
NROS_PLATFORM_POSIX_SRC    = DOUBLED MISSING= …/packages/platform/nros-platform-posix/src
NROS_PLATFORM_THREADX_SRC  = DOUBLED MISSING= …/packages/platform/nros-platform-threadx/src
NROS_LAN9118_LWIP_DIR      = DOUBLED MISSING= …/packages/drivers/net/lan9118-lwip
NROS_VIRTIO_NET_NETX_DIR   = DOUBLED MISSING= …/packages/drivers/net/virtio-net-netx
NROS_C_INCLUDE             = DOUBLED MISSING= …/packages/api/nros-c/include
NROS_CPP_INCLUDE           = DOUBLED MISSING= …/packages/api/nros-cpp/include
TBAND_DIR                  = DOUBLED MISSING= …/third-party/tracing/Tonbandgeraet/tband
-> doubled: 9   non-existent: 12
```

The other twelve WERE inherited from the parent checkout and were re-rooted
correctly. That is why the bug reads as random: whether a given variable is
right depends on whether the shell that spawned the agent happened to export it.

## Resolution

Fixed by taking the issue's own preferred option — route the values through the
existing shell helper, not a second derivation in `just`.

**1. `just/sdk-env.just` runs the rule itself, per value.** One invocation
spelling shared by all 21 exports:

```just
_NROS_REROOT_CMD := '"$2"/scripts/lib/reroot-checkout-path.sh "$1" "$2"'
export NROS_PLATFORM_CFFI_INCLUDE := shell(_NROS_REROOT, env("NROS_PLATFORM_CFFI_INCLUDE", _NROS_HERE / "packages/platform/nros-platform-api/include"), _NROS_HERE)
```

`scripts/lib/reroot-checkout-path.sh` is a new thin executable wrapper around
`nros_reroot_checkout_path` — the same function `scripts/sdk-env.sh` and
`build-root.sh` already call. `_NROS_OTHER` is gone.

The narrow alternative the issue offers ("refuse the rewrite when `_NROS_OTHER`
is a prefix of `_NROS_HERE`") was **rejected as wrong, not merely weaker**: in a
nested worktree it would disable re-rooting entirely, re-creating 1280 for every
inherited value naming the parent — which in this very session was twelve of
twenty-five. And no prefix rewrite can be right here at all: under nesting every
`_NROS_HERE`-rooted value is also `_NROS_OTHER`-rooted, so "keep" and "re-root"
are the same lexical test and only the DEEPEST owning checkout separates them.

**Cost, measured** (the reason to check before choosing one process per value):
26–28 ms for all 21 helper invocations, against ~24 ms for the single detector
call 1280 used. End to end, `just --evaluate` went **30 ms → 46 ms**.

**2. `foreign-checkout-root.sh` keeps its foreignness rule.** The question was
raised whether it should report a PARENT checkout as foreign when `here` is
nested inside it. It should: an inherited value naming the parent must still be
re-rooted — that is exactly 1280 — and a value naming the worktree is already
skipped, because `nros_checkout_root` resolves its owner to `here`. The detector
was never wrong; the `just` side was.

What did change: its ANSWER is advisory-only now, so the "two foreign checkouts
in one environment" case is **reported rather than refused**. That refusal
existed because one prefix could not serve both; per-value ownership has no such
ambiguity — measured, such an environment now resolves `rc=0` with each value
re-rooted by its own owner.

**The advisory needed an explicit EDGE to stay reachable**, and that was caught
by running it rather than reading it. Once nothing consumed
`_NROS_INHERITED_FROM`, `just --evaluate` stopped printing the diagnostic: a
recipe run evaluates every assignment, but `--evaluate` does not force a private
variable nothing depends on — and `--evaluate` is precisely the command a person
runs when a path looks wrong. `_NROS_REROOT` is therefore an
`if _NROS_INHERITED_FROM == "" { CMD } else { CMD }` whose arms are the same
string: it computes nothing and makes the detector a dependency of every export.

**3. `check-inherited-checkout-paths` measures BOTH checkout shapes.** Its three
behaviour probes built synthetic checkouts SIDE BY SIDE in `/tmp`, so the nested
shape — the one agent sessions actually work in — was outside the gate's reach
while inside the rule's (the 0196 shape, and the same lesson as
`check-git-dir-layout-assumptions`). `probe_just_nested` builds a real
`<parent>/.claude/worktrees/agent-0000` pair from the shipped files and asserts,
over all 25 evaluated variables:

1. no value repeats the worktree's own relative segment (that repetition IS the
   bug, and is the acceptance test this issue asked for);
2. no value still names the parent checkout;
3. an INHERITED `NUTTX_DIR` naming the parent is re-rooted (1280 kept);
4. a DEFAULTED `NROS_PLATFORM_CFFI_INCLUDE` is left alone (1391 fixed);
5. an out-of-tree `PX4_AUTOPILOT_DIR` is untouched, though the parent is a
   prefix of the worktree;
6. the re-rooting advisory actually reaches stderr.

Negative controls, both run: the pre-fix `sdk-env.just` in the synthetic
checkouts reports **22 problems** (including `IDF_PATH` tripled — it defaults to
`NROS_ESP_IDF_WORKSPACE`, itself already doubled); removing the advisory edge
reports probe 6 and nothing else. With the fix, 0.

Two static ratchets sit beside them: the retired `_NROS_OTHER` spelling may not
return in CODE (the header comment explains it on purpose, so the ratchet reads
comment-stripped text), and the advisory call must stay. Both carry self-test
mutations on the normal path.

## Acceptance — measured

Nested worktree, **no `--set _NROS_OTHER` workaround**:

```
=== BEFORE (811f081a3, 1280 prefix rewrite) ===
  -> doubled: 9   non-existent: 12
  -I …/agent-a66bce40c1e033271/.claude/worktrees/agent-a66bce40c1e033271/packages/platform/nros-platform-api/include
  FAILS (rc=1):
    fatal error: nros/platform.h: No such file or directory

=== AFTER (per-value re-root) ===
  -> doubled: 0   non-existent: 4
  -I …/agent-a66bce40c1e033271/packages/platform/nros-platform-api/include
  COMPILES (rc=0)
```

The four still non-existent are `TBAND_DIR` and the three `esp-idf-workspace`
paths — unprovisioned optional SDKs, missing in the main checkout too, and not
this bug.

**The main checkout is unchanged.** Old and new `sdk-env.just` evaluated in a
synthetic NON-nested checkout under one environment: all **25 variables
byte-identical** (`diff` empty).

**Foreign re-rooting still works** — what 1280 added this for. In that same
non-nested probe, `NUTTX_DIR` exported as `<other-checkout>/third-party/nuttx/
nuttx` answers `<this-checkout>/third-party/nuttx/nuttx`, and
`PX4_AUTOPILOT_DIR` exported as `<vendor>/nuttx` (outside any checkout) answers
`<vendor>/nuttx` unchanged. Live in the worktree, the twelve variables the agent
shell really had inherited from `/home/aeon/repos/nano-ros` (`FREERTOS_DIR`,
`LWIP_DIR`, `NUTTX_DIR`, `THREADX_DIR`, `NETX_*`, `PX4_*`, `IDF_*`, …) all
evaluate rooted at the worktree. `source ./activate.sh` agrees, because
`scripts/sdk-env.sh` reads the unset-variable defaults straight from this file.

## Files

* `just/sdk-env.just` — 21 exports re-rooted per value; `_NROS_OTHER` removed
* `scripts/lib/reroot-checkout-path.sh` — new; the `just`-side entry to the ONE rule
* `scripts/lib/foreign-checkout-root.sh` — advisory-only; multi-root reported, not refused
* `scripts/check-inherited-checkout-paths.py` — nested-shape probe + two ratchets

## Not done

`just test-unit` was not run: `just check fast` on this branch has three
failures that reproduce unchanged on `811f081a3` — `capability-conditionals` and
`xrce-vendored-versions` (submodules not provisioned in this worktree) and
`codegen-version-refusal` row E (a stale in-tree CLI emitting codegen version 3
against this tree's 6) — and the host was carrying another agent's heavy lane
(load average 28–36). The acceptance above compiles the exact header the issue
reports missing, which is the same question one layer down.
