---
id: 1391
title: "`just/sdk-env.just` DOUBLES every defaulted SDK path in a worktree nested inside its parent checkout"
status: resolved
type: bug
area: [build, tooling]
related: [1280, 1039, 1336, 0196]
---

## What happened

Issue 1280 made every path-valued export in `just/sdk-env.just` re-root an
inherited absolute path onto the checkout being built. It did that with a
LEXICAL PREFIX REWRITE, one per line:

```just
export NROS_PLATFORM_CFFI_INCLUDE := replace(env("NROS_PLATFORM_CFFI_INCLUDE", _NROS_HERE / "packages/platform/nros-platform-api/include"), _NROS_OTHER, _NROS_HERE)
```

`_NROS_OTHER` comes from `scripts/lib/foreign-checkout-root.sh` — the other
nano-ros checkout this environment was inherited from.

An agent worktree lives at `<main>/.claude/worktrees/<id>`, so the two checkouts
**NEST**: `_NROS_OTHER` (`/home/aeon/repos/nano-ros`) is a strict PREFIX of
`_NROS_HERE`. And `replace()` is applied unconditionally — to the DEFAULT too,
which is already rooted at `_NROS_HERE`. So a correct path got expanded.

Measured on `811f081a3`, in a worktree at
`/home/aeon/repos/nano-ros/.claude/worktrees/agent-a66bce40c1e033271`, with no
`--set` workaround:

```
NROS_PLATFORM_CFFI_INCLUDE = DOUBLED MISSING= /home/aeon/repos/nano-ros/.claude/worktrees/agent-a66bce40c1e033271/.claude/worktrees/agent-a66bce40c1e033271/packages/platform/nros-platform-api/include
NROS_PLATFORM_FREERTOS_SRC = DOUBLED MISSING= …/.claude/worktrees/agent-…/.claude/worktrees/agent-…/packages/platform/nros-platform-freertos/src
NROS_PLATFORM_POSIX_SRC    = DOUBLED MISSING= …/packages/platform/nros-platform-posix/src
NROS_PLATFORM_THREADX_SRC  = DOUBLED MISSING= …/packages/platform/nros-platform-threadx/src
NROS_LAN9118_LWIP_DIR      = DOUBLED MISSING= …/packages/drivers/net/lan9118-lwip
NROS_VIRTIO_NET_NETX_DIR   = DOUBLED MISSING= …/packages/drivers/net/virtio-net-netx
NROS_C_INCLUDE             = DOUBLED MISSING= …/packages/api/nros-c/include
NROS_CPP_INCLUDE           = DOUBLED MISSING= …/packages/api/nros-cpp/include
TBAND_DIR                  = DOUBLED MISSING= …/third-party/tracing/Tonbandgeraet/tband
-> doubled: 9   non-existent: 12
```

**Exactly the nine whose environment variable was UNSET**, so the default arm
won. The other twelve were inherited from the parent checkout and were re-rooted
correctly — which is why the bug reads as random: whether a variable is right
depends on whether the shell that spawned the agent happened to export it.

The consequence a reader actually sees names a source file, not an environment:

```
fatal error: nros/platform.h: No such file or directory
```

Two agent sessions lost a full lane run each to it on 2026-09-18. The
`1039` addendum had already diagnosed it on 2026-09-16 ("it deserves its own
issue") and named the fix: the SHELL spelling of the same rule,
`nros_reroot_checkout_path`, does not have this bug.

## Why a prefix rewrite could never be the rule

`scripts/lib/checkout-paths.sh` states the rule as three-valued, on the value's
OWNING checkout:

| the value points … | |
| --- | --- |
| outside any nano-ros checkout | KEEP (a real out-of-tree SDK) |
| inside THIS checkout | KEEP |
| inside a DIFFERENT checkout | RE-ROOT here |

Under nesting, rows 2 and 3 are lexically indistinguishable: every
`_NROS_HERE`-rooted value is also `_NROS_OTHER`-rooted, because
`_NROS_OTHER` is a prefix of `_NROS_HERE`. Only the DEEPEST marker at or above
the value separates them, and that is a per-VALUE question no single prefix can
answer. Anchoring the rewrite does not help — the anchored prefix still matches
both. The `just` side was a second spelling of the rule that happened to agree
with the first in the side-by-side shape and disagree in the nested one.

## Resolution

**1. `just/sdk-env.just` now runs the rule itself, per value** (issue 1391).
One invocation spelling, shared by all 21 lines:

```just
_NROS_REROOT := '"$2"/scripts/lib/reroot-checkout-path.sh "$1" "$2"'
export NROS_PLATFORM_CFFI_INCLUDE := shell(_NROS_REROOT, env("NROS_PLATFORM_CFFI_INCLUDE", _NROS_HERE / "packages/platform/nros-platform-api/include"), _NROS_HERE)
```

`scripts/lib/reroot-checkout-path.sh` is a thin executable wrapper around
`nros_reroot_checkout_path` — the same function `scripts/sdk-env.sh` and
`build-root.sh` already call. No second spelling of the rule, and no list of
which variable is which. `_NROS_OTHER` is gone.

**Cost, measured on this host** (the reason to check before choosing one
process per value): 26–28 ms for all 21 helper invocations, against ~24 ms for
1280's single detector call. End to end, `just --evaluate` went **30 ms → 46 ms**
per invocation. That is the whole price, and it buys the rule instead of an
approximation of it.

**2. `foreign-checkout-root.sh` is kept, and its foreignness rule is
unchanged.** The question was raised whether it should report a PARENT checkout
as foreign when `here` is nested inside it. It should: an inherited value naming
the parent must still be re-rooted — that is exactly 1280 — and a value naming
the worktree is already skipped, because `nros_checkout_root` resolves its owner
to `here`. The detector was never wrong; the `just` side was.

What did change: its ANSWER is now advisory-only, so the "two foreign checkouts
in one environment" case is **reported rather than refused**. That refusal
existed because one prefix could not be right for both; per-value ownership has
no such ambiguity — measured, an environment naming two other checkouts now
resolves, each value re-rooted by its own owner, `rc=0`.

**The advisory needed an explicit EDGE to stay reachable**, and this was caught
by running it rather than reading it. Once nothing consumed
`_NROS_INHERITED_FROM`, `just --evaluate` stopped printing the diagnostic
entirely: a recipe run evaluates every assignment, but `--evaluate` does not
force a private variable nothing depends on — and `--evaluate` is precisely the
command a person runs when a path looks wrong. `_NROS_REROOT` is therefore an
`if _NROS_INHERITED_FROM == "" { CMD } else { CMD }` whose two arms are the same
string: it computes nothing and exists to make the detector a dependency of
every export. Probe 6 of the gate MEASURES the advisory's presence on stderr
(negative control: removing the edge reports exactly this, and nothing else).

**3. `check-inherited-checkout-paths` measures BOTH checkout shapes.** Its three
behaviour probes built synthetic checkouts SIDE BY SIDE in `/tmp`, so the nested
shape — the one agent sessions actually work in — was outside the gate's reach
while inside the rule's (the 0196 shape, and the same lesson as
`check-git-dir-layout-assumptions`). `probe_just_nested` now builds a real
`<parent>/.claude/worktrees/agent-0000` pair from the shipped files and asserts,
on all 25 evaluated variables:

* no value repeats the worktree's own relative segment (that repetition IS the bug);
* no value still names the parent checkout;
* an INHERITED `NUTTX_DIR` naming the parent is re-rooted (1280 kept);
* a DEFAULTED `NROS_PLATFORM_CFFI_INCLUDE` is left alone (1391 fixed);
* an out-of-tree `PX4_AUTOPILOT_DIR` is untouched, even though the parent is a
  prefix of the worktree.

Negative control, run with the pre-fix `sdk-env.just` copied into the synthetic
checkouts: **22 problems reported**, including `IDF_PATH` tripled (it defaults to
`NROS_ESP_IDF_WORKSPACE`, which was itself already doubled). With the fix: 0.

Two static ratchets were added beside it: the retired `_NROS_OTHER` spelling may
not come back in CODE (the header comment explains it on purpose, so the ratchet
reads comment-stripped text), and the advisory call must stay. Both carry
self-test mutations on the normal path.

## Acceptance — measured

In the nested worktree, with **no `--set _NROS_OTHER` workaround**:

```
=== BEFORE (origin/main, 1280 prefix rewrite) ===
  -> doubled: 9   non-existent: 12
  compile a TU that includes <nros/platform.h>
  -I …/agent-a66bce40c1e033271/.claude/worktrees/agent-a66bce40c1e033271/packages/platform/nros-platform-api/include
  FAILS (rc=1):
    fatal error: nros/platform.h: No such file or directory

=== AFTER (1391 per-value re-root) ===
  -> doubled: 0   non-existent: 4
  -I …/agent-a66bce40c1e033271/packages/platform/nros-platform-api/include
  COMPILES (rc=0)
```

The four still non-existent after the fix are `TBAND_DIR` and the three
`esp-idf-workspace` paths — unprovisioned optional SDKs, missing in the main
checkout too, and not this bug.

**The main checkout is unchanged.** Old and new `sdk-env.just` evaluated in a
synthetic NON-nested checkout under one environment: all **25 variables
byte-identical** (`diff` empty).

**Foreign re-rooting still works** — the thing 1280 added this for. In the same
non-nested probe, with `NUTTX_DIR` exported as `<other-checkout>/third-party/
nuttx/nuttx`, the new rule answers `<this-checkout>/third-party/nuttx/nuttx`;
with `PX4_AUTOPILOT_DIR` exported as `<vendor>/nuttx` (outside any checkout) it
answers `<vendor>/nuttx` unchanged. And live in this worktree, the twelve
variables the agent shell really had inherited from `/home/aeon/repos/nano-ros`
(`FREERTOS_DIR`, `LWIP_DIR`, `NUTTX_DIR`, `THREADX_DIR`, `NETX_*`, `PX4_*`,
`IDF_*`, …) all evaluate rooted at the worktree.

## Files

* `just/sdk-env.just` — 21 exports re-rooted per value; `_NROS_OTHER` removed
* `scripts/lib/reroot-checkout-path.sh` — new; the `just`-side entry to the ONE rule
* `scripts/lib/foreign-checkout-root.sh` — advisory-only; multi-root reported, not refused
* `scripts/check-inherited-checkout-paths.py` — nested-shape probe + two ratchets
