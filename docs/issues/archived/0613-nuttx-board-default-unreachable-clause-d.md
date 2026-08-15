---
id: 613
title: "RETRACTED: `nros-board-nuttx`'s `default` was NOT unreachable — I read a stale clause (d), fixed 11 hours earlier"
status: resolved
type: bug
area: build
related: [issue-0593, phase-360]
---

## Symptom

`just ci` fails at `check-feature-contract`:

```
[FAIL] clause (d) — no `default` feature is unreachable
  packages/boards/nros-board-nuttx/Cargo.toml: `default` names ['image-runtime'],
      and all 1 in-workspace dep-sites on `nros-board-nuttx` pass
      `default-features = false` without naming them.
      Reachable only by feature unification in a whole-workspace build — it
      disappears in the per-package build cmake runs (issue 0593).
```

## This is exactly what issue 0593 asked the gate to catch

0593's own "Gate" section specified it:

> The class is "a load-bearing feature enabled only by a `default` that every
> real dep-site disables". phase-360 W4's `check-feature-contract` should assert
> it directly.

The gate was built, and the first thing it found was a second instance of the
same class in a different crate. That is the gate working, not a new defect
appearing.

## The topology

`nros-board-nuttx` has exactly ONE in-workspace dep-site:

```toml
# packages/boards/nros-board-nuttx-qemu/Cargo.toml
nros-board-nuttx = { path = "../nros-board-nuttx", default-features = false }
```

and the overlay then forwards the feature EXPLICITLY, defaulting it on there:

```toml
default = ["image-runtime"]
image-runtime = ["nros-board-nuttx/image-runtime"]
```

So the inner crate's `default` could never activate: its only consumer disables
defaults and re-enables the one feature by name.

## The fix that would have been WRONG

Naming `image-runtime` at that dep-site. The two FFI bins take the OVERLAY with
`default-features = false` precisely to get the runtime OFF — a C/C++ image
gets `#[panic_handler]` and `#[global_allocator]` from `nros-c`:

```
packages/boards/nros-board-nuttx-qemu/nros-nuttx-ffi/Cargo.toml
packages/boards/nros-board-nuttx-qemu/nros-nuttx-riscv-ffi/Cargo.toml
```

Forcing it on at the inner dep-site would give those images two of each — the
lang-item duplication the manifest's own comment warns about.

## Fix

`default = []` on `nros-board-nuttx`. The feature stays and is reached the way it
is actually used: through the overlay's forward.

**Verified inert**, which is the claim worth checking rather than asserting. The
dep-site already passed `default-features = false`, so emptying the default
cannot change what it resolves. Compared directly:

```
$ diff before.txt after.txt      # resolved nros-board-nuttx features
IDENTICAL — the change is inert for this dep-site
```

`check-feature-contract` now reports `ok (d) no default feature is unreachable`.

## What changes for an out-of-tree consumer

A direct dependant of `nros-board-nuttx` (there are none in this workspace) that
relied on defaults must now name `image-runtime`. That is the same trade issue
0593 made for `nros`, and it is the point of the rule: a load-bearing feature
should be requested where it is needed, not inherited from a default that every
real dep-site turns off.

## RETRACTED 2026-08-16 — this was a false positive, and the change is reverted

The premise was wrong. Clause (d) did not have a real finding here.

Upstream's `a32196ab2` (2026-08-15 13:49) had already fixed the clause's blind
spot: it now follows a feature reached by FORWARDING —

```toml
# nros-board-nuttx-qemu
image-runtime = ["nros-board-nuttx/image-runtime"]
```

— and not only by `features = [...]` on the dep line. Its own code comment says
so, and names this crate as the case that motivated it.

I diagnosed against a checkout ~11 hours older than that commit, saw the stale
failure, and emptied `nros-board-nuttx`'s `default` to satisfy it. With the
current gate, restoring `default = ["image-runtime"]` is GREEN — clause (d) does
not fire.

So the change was unnecessary, and not harmless: the crate's comment documents
that default as intentional, so a pure-Rust image gets `#[panic_handler]` and
`#[global_allocator]` without naming a flag. Emptying it moved that burden onto
every out-of-tree direct consumer for no gain. **Reverted** to
`default = ["image-runtime"]`.

The "verified inert" measurement in this issue was true and still is — it showed
the change did not alter what the ONE in-workspace dep-site resolves. What it
could not show, because I never asked, was whether the change was NEEDED. Inert
is not the same as warranted.

The real instance of this class is issue 0615, found immediately afterwards in
`nros-cpp`, where clause (d) reasons only about dep-sites and misses a crate
whose own artifact is final.
