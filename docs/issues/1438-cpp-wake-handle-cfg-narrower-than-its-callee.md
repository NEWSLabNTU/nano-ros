---
id: 1438
title: "`nros_cpp_executor_wake_handle` is gated on `rmw-cffi` alone while the
  `wake_raw_ptr` it calls needs `alloc` too, so a no-alloc C++ image does not
  compile — and the first tier-2 run that ever reached the build is how we found out"
status: open
type: bug
area: cpp, core, ci
severity: high
found: 2026-09-21
related: [1431, 1389, 1158, 1284]
---

## What happens

`run-matrix` (tier 2) run **35569312822** (schedule, 2026-09-21T06:37, head
`a2c2cbb49`'s predecessor `8ec2575ef`), job **106237388463**, step
`just build tier2`:

```
error[E0599]: no method named `wake_raw_ptr` found for struct `Executor<'s>` in the current scope
error: could not compile `nros-cpp` (lib) due to 1 previous error
error: recipe `build-fixtures` failed with exit code 2
```

The two cfg predicates disagree by one feature:

* `packages/core/nros-node/src/executor/spin.rs:2736` —
  `#[cfg(all(feature = "alloc", feature = "rmw-cffi"))] pub fn wake_raw_ptr(&self)`
* `packages/api/nros-cpp/src/lib.rs:3853` —
  `#[cfg(feature = "rmw-cffi")] pub unsafe extern "C" fn nros_cpp_executor_wake_handle`,
  whose body is `cpp.executor.wake_raw_ptr()`

So any build with `rmw-cffi` and **without** `alloc` compiles the caller and not
the callee. The callee's own predicate is not arbitrary: it reads
`self.node_wake`, whose type is
`Option<portable_atomic_util::Arc<NodeWake>>` (`spin.rs:760`, `:1436`), and an
`Arc` needs `alloc`. The narrow one is the CALLER's.

## Why nothing caught it

No merge-gating lane builds this feature combination. `check-workspace-all`
runs on the pull request and unifies features across the workspace, which is
exactly the shape that hides a per-combination break (the same reason
`--workspace` hid 20 errors in nros-node's own test target, recorded in
CLAUDE.md). The combination that fails is a C++ image with the CFFI RMW seam and
no allocator — an embedded C++ board, which only tier 2 and the nightly build.

Tier 2 itself could not report it either, for a different reason: every tier-2
run up to 2026-09-21 stopped in *provisioning* or at a *gate* (issues 1158,
1389, 1387), so the lane never reached the build stage at all. Run 35569312822
is the first one that did — its own `coverage` check-run says
`tier 2 — NO VERDICT: stopped in the build`, which is the stage axis issue 1158
added working as intended. A lane with no signal capacity cannot report a
regression; this is what was waiting behind that.

## What this is NOT

- **Not issue 1431.** The same job also fails
  `nros/node.hpp:18:10: fatal error: map: No such file or directory`, which is
  1431 and has a fix in flight (PR #1139). Two causes, one job — they must not
  be attributed together.
- **Not a regression from the tier-2 fixes.** Nothing in 1387/1389 touched
  these two predicates. The break was reachable before; the lane simply never
  got far enough to compile it.
- **Not a missing feature.** `alloc` is deliberately absent from this image;
  the executor's no-alloc path is the supported one.

## What would close it

The two predicates must agree, and the direction is a decision about the C ABI
rather than about the cfg:

1. **Gate the caller on both features** (`all(feature = "alloc", feature = "rmw-cffi")`).
   Then a no-alloc image exports no `nros_cpp_executor_wake_handle` at all, and
   whatever C++ calls it fails to LINK instead of to compile — which is only
   correct if nothing in a no-alloc C++ image calls it.
2. **Keep the symbol and return NULL without `alloc`.** The method already has
   exactly this arm for "no wake object" (`None => core::ptr::null_mut()`), and
   its doc comment says NULL means "this build has none" — so a no-alloc build
   having none is the same statement, spelled one layer up.

(2) looks right on the documented contract, but it is the caller's ABI and the
answer belongs to whoever owns phase-436 W7, so it is filed rather than patched.

Acceptance is a tier-2 or nightly C++ embedded cell compiling `nros-cpp` for the
no-alloc combination, plus a lane that builds `rmw-cffi` WITHOUT `alloc` so the
next divergence is caught by a gate rather than by a lane that spent a month
unable to reach its own build stage.
