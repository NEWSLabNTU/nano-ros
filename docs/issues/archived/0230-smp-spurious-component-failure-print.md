---
id: 230
title: "SMP: spurious 'ComponentNode failed at ? (code=0)' boot diagnostic while the component actually runs"
status: resolved
type: bug
area: core
related: [phase-292, rfc-0044]
---

## Summary

On the ASI FVP SMP-4 image (phase-292 W2 validation), every boot prints

```
[nros] FATAL: ComponentNode "controller" failed at ? (code=0) — halting boot
```

yet the component is fine: all entities created, spin proceeds, the full
closed-loop demo delivers. `what=?` and `code=0` mean the entry's
post-construct `ok()` check observed a false flag with NO recorded error
site — consistent with a cross-CPU visibility race between the core that
runs the component ctor (setting/clearing the error state) and the core
that runs the entry's check, with no synchronization on the flag.
Single-core images never print it.

## Impact

Cosmetic-but-scary: a FATAL line that claims "halting boot" on a healthy
boot. Also implies the check can MISS a real failure the same way.

## Fix direction

Make the ComponentNode error state (`ok_` flag + site + code) either
atomics with acquire/release, or force the check onto the constructing
thread / behind an explicit barrier in the entry seam.

## Resolution (2026-07-18) — invert the flag

Root cause pinpointed from the exact symptom (`what=?`, `code=0`): those are
the ZERO-init values of `__nros_comp_buf` (static BSS). The component's boot
state was tracked as `bool ok_ = true`, which requires a *store* to become
true — so a reader that observes the object before the ctor's stores
propagate (a different core than the constructing one, per the report) reads
the BSS zero as `ok_ == false` → the spurious "failed at ? (code=0)".

Fix (component_node.hpp only, zero template changes): track `has_error_`
(default `false` = the zero-init value) instead of `ok_`. The HEALTHY
common case is now the universally-visible zero state — no store, no
synchronization needed, so a cross-core reader always sees "ok". A real
failure is published with a `__atomic_store_n(RELEASE)` in `set_error` and
read with `__atomic_load_n(ACQUIRE)` in `ok()`/`error_what()`/`error_code()`,
which ALSO closes the dangerous direction the issue flagged: a genuine
failure can no longer be silently missed — any core that observes the
post-failure object sees the error and the release-published site/code.

`__atomic` builtins (not `<atomic>`) keep it header-free and freestanding-
safe on the Zephyr `-nostdinc++` minimal libcpp (issue 0112 class).

Verified: native rclcpp `component-node-poc` rebuilds + runs healthy — pub/
sub delivering, ZERO FATAL/`failed at` lines on boot. The SMP direction is
correct by the memory model (zero-memory ⇒ `ok()`); a live FVP-SMP re-run
is gated on issue #232's runtime lane.
