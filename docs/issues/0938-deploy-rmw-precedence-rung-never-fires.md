---
id: 938
title: "RFC-0031's `[deploy.<t>].rmw` precedence rung never fires — one
  production caller passes `target = None`, and both live uses are masked"
status: open
type: bug
area: cli
related: [rfc-0031, phase-383, phase-255]
---

## Problem

`SystemToml::resolved_rmw` implements the RFC-0031 ladder, and says so:

> the CLI `--rmw` flag, then `[deploy.<target>].rmw`, then `[system].rmw`,
> then the `"zenoh"` default

The middle rung is unreachable. It is guarded on the `target` argument:

```rust
if let Some(t) = target
    && let Some(dt) = self.deploy.get(t)
    && let Some(r) = &dt.rmw
{ return r.clone(); }
```

and `resolved_rmw` has exactly ONE non-test caller — `bridged_rmws()`, which
passes `resolved_rmw(None, None)`. Every other reference is its own unit test
(`resolved_rmw_precedence_ladder`) or a doc comment. So in production the
function only ever answers `--rmw` → `[system].rmw` → `zenoh`, and a
`[deploy.<t>].rmw` a user writes is silently ignored.

## Why nobody noticed

The only two `[deploy.*].rmw` in the tree both set the value the default
already produces:

    multi_pkg_workspace_freertos  [system] rmw = "zenoh"  [deploy.qemu-mps2-an385] rmw = "zenoh"
    multi_pkg_workspace_nuttx     [system] rmw = "zenoh"  [deploy.qemu-arm-nuttx]  rmw = "zenoh"

Identical values, so the dead rung is unobservable. A user writing
`[system].rmw = "zenoh"` with `[deploy.qemu].rmw = "cyclonedds"` gets zenoh with
no diagnostic.

This is the second instance of one shape inside phase-383: the deprecation lint
W10.b depends on ALSO shipped "with four passing unit tests and NO production
caller". A unit test proves a function computes; it says nothing about whether
anything calls it.

## Fix, and the question under it

Mechanically: thread the deploy target into the call. `bridged_rmws()` is about
a whole binary's link set rather than one target, so the honest fix is probably
a second entry point that takes the target, used by whatever resolves a
per-target RMW today.

But the design question comes first, and RFC-0065 may have already answered it:
`[image.*]` now carries `rmw`, and phase-383 W10.b is retiring build fields from
`[deploy.*]` precisely because they belong to the image. If `[deploy.<t>].rmw`
is *supposed* to be dead, the fix is to delete the rung and its test rather than
wire it — and the RFC-0031 doc comment then needs correcting, since it currently
documents behaviour the code does not have.

Either way the present state is the bad one: a documented ladder with a silent
missing rung.

## Note for phase-383 W10.b

This makes the remaining 42 `[deploy.*]` build fields **inert after all**, which
is what that work item originally claimed and what a review of this issue's
first draft wrongly disputed. Nothing reads them at build time: `board` and
`target` were already established as unread, and `rmw` is unread for the reason
above.
