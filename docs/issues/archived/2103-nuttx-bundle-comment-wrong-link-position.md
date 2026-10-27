---
id: 2103
title: "The NuttX platform build says `-bundle` puts a lib's `-l` after every rlib; rustc puts it right after its own crate"
status: resolved
type: limitation
area: [nuttx, build, docs]
severity: low
found: 2026-10-27
related: [2098, 0048, 0294]
resolved: 2026-10-27
resolved_in: "#2759"
---

## What

`packages/boards/nros-board-common/src/nuttx_platform_build.rs` explained its
link directive twice, for the platform port and for the run-tiers seam:

> `-bundle` keeps the port a standalone `lib*.a` and emits a trailing `-l` at
> the FINAL binary link — AFTER every rlib, so the referencers'
> `nros_platform_*` undefineds resolve.

That is not where rustc puts it. Issue 2098 measured it on rustc 1.99,
aarch64: a `-bundle` native lib's `-l` comes right after the crate that
declared it, inside the rlib order. Rlibs that link after that crate still
follow it.

The NuttX build is not wrong. Its port is ALSO `+whole-archive`, which pulls
every member at that point, so the order never mattered. Only the
explanation was wrong. It cost a debugging session: issue 2098's first fix
trusted it and linked the POSIX port `-bundle` alone, which moved nothing.

## Fix

Both comments now say what each modifier does. `-bundle` keeps the archive
standalone, which `+whole-archive` needs. `+whole-archive` is what makes the
link order-independent (RFC-0042 D3). `nuttx_image_link.rs`'s comment only
claims the `-l` lands before the trailing `-C link-arg` kernel group, which
is true, so it is unchanged. Archived issue 0294 keeps its original wording as
history.
