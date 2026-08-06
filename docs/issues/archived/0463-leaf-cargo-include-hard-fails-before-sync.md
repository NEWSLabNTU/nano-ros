---
id: 463
title: "Tracked leaf `.cargo/config.toml` files `include` gitignored sync output, and a missing include is a HARD cargo error — not the silent drop #272 and #457 both assumed"
status: resolved
type: bug
area: build
related: [issue-0457, issue-0272, issue-0440, issue-0196, rfc-0048, phase-338]
resolved_in: phase-338
---

## Resolution

Two design comments in `cmd/ws.rs` justified their arrangement with a claim
about cargo that measurement contradicts. The claim is corrected, and the gap it
was hiding is closed at the seam:

- `_require-leaf-includes` (`scripts/build/leaf-config-includes.py`) checks every
  `include` target in every tracked `.cargo/config.toml` and, when any is
  missing, says **run `nros sync`** — instead of letting cargo say it four
  frames deep, once per leaf, naming a path that never mentions sync. Wired into
  `rust-rtos-link-check` and `build-test-fixtures-leaves`.
- `check-cargo-config-tracked` gained an arm rejecting an `include` that names
  anything other than the two generated targets. Such an entry has no generator
  at all, so no sync run can ever satisfy it and the leaf is bricked for
  everyone, permanently — a strictly worse failure than the one filed here, and
  previously ungated.
- Both source comments now state cargo's actual behaviour.

What was NOT done, and why, is in "Why the obvious fix is wrong" below.

## The wrong premise

`cmd/ws.rs` said, in two places:

> cargo ignores a missing `include` SILENTLY

Measured on cargo 1.97.1 (c980f4866 2026-06-30), it is a hard error, and it
fires during **manifest parse** — so the leaf becomes unreadable, not merely
unbuildable. `cargo metadata`, `cargo tree`, and every gate that walks the leaf
fail with it:

```
error: failed to parse manifest at `examples/qemu-arm-freertos/rust/talker/Cargo.toml`
Caused by: could not load Cargo configuration
Caused by: failed to load config include `nros-managed-patch.toml` from `…/.cargo/config.toml`
Caused by: No such file or directory (os error 2)
```

Both #272 (central `nros-patch.toml`) and #457 (the per-leaf
`nros-managed-patch.toml` sidecar) reasoned from that premise. #272 built a
sync-time reachability check to pre-empt a silent-resolution failure that does
not occur; #457 leaned on it to argue that dropping the include entry when the
managed set empties was a tidiness measure rather than a correctness one.

## Scope: wider than first filed

This issue was first written as a regression introduced by #457. That was wrong,
and the correction matters for where the fix belongs. Measured by removing each
target in turn and re-parsing a leaf:

| include target | tracked? | target gitignored? | leaves affected | missing ⇒ |
| --- | --- | --- | --- | --- |
| `…/nros-patch.toml` (central, #272) | entry is | yes (`.gitignore:133`) | 57 | exit 101, parse failure |
| `nros-managed-patch.toml` (sidecar, #457) | entry is | yes (`.gitignore:119`) | 48 | exit 101, parse failure |

So the hole has existed since #272; #457 added a second instance of it. On this
host the central file happened to exist (generated weeks earlier) while the
sidecar did not, which is the only reason #457 looked like the cause.

Confirmed the include is the whole failure: dropping a one-comment placeholder
in makes the leaf parse (exit 0), with nothing else changed.

## Why the obvious fix is wrong

"Make a fresh clone parse" sounds like the real fix. It is not available, and
more importantly it is not the goal:

- **Commit the targets.** Their rows name `generated/` message crates built from
  the USER's ament install. Committing them puts host-derived content in git and
  re-creates the churn #457 measured and removed. It also violates the standing
  rule that a tracked artifact must not assert which ROS install built it.
- **Drop the include from the tracked half and have sync add it.** Sync already
  writes this file; the entry would land in the tracked config and be committed,
  which is exactly the present state. There is nowhere else to put an entry
  whose whole job is to be cargo's entry point.
- **Make the include optional.** Cargo has no such syntax.

And a fresh clone cannot build these leaves regardless: the patches point at
`generated/` trees only sync produces. Requiring sync is inherent, so the defect
was never "sync is required" — it was that the requirement announced itself as
an unreadable cargo trace. That is what the guard fixes.

## Why the gates missed it

The tree that validated #457 had already run sync, so every sidecar existed
locally. Nothing asserted the bare-clone property the tracked half depends on —
the issue-0196 rule again, a gate narrower than the invariant it enforces.
`_require-fixtures` had an analogue for fixtures; leaves had none.

The new gate arm is verified to FIRE, not merely to pass: adding a
`"typo-patch.toml"` entry to one leaf config makes
`check-cargo-config-tracked` report it and exit non-zero.

## Follow-up (2026-08-07) — fewer leaves need sync at all

The fix above makes the *absence* of a sync legible. A second pass narrowed
which leaves depend on one, by splitting the managed rows by ORIGIN rather than
by "sync wrote it".

Measured across the tree, sync's managed set is mostly in-repo crates — **183
in-repo rows against 88 `generated/` ones**. An in-repo row (`nros-log`, a board
crate, `mps2-an385-pac`) is a relative path identical in every checkout, and a
clone needs it to resolve at all. Only the `generated/` rows are host-specific,
being built per host from the consumer's ament install.

So in-repo rows are inline in the tracked `config.toml` (tagged
`# nros-managed`, as before 0457) and only `generated/` rows go to the sidecar —
with the `include` written only when that file is. A leaf with no message
dependency therefore has no sidecar, no include, and resolves in a fresh clone
with no sync; what stays behind sync is exactly what only sync can produce.

Dropping the include alone was tried first and is not sufficient: it moves the
failure rather than removing it. With the whole set in the sidecar and no
include, `just check` dies on `no matching package named 'mps2-an385-pac'` —
an in-repo patch — instead of a parse error.

`_require-leaf-includes` still covers the leaves that do have generated deps.
The two changes compose: this one shrinks the set that needs sync, that one
explains the failure for the set that remains.
