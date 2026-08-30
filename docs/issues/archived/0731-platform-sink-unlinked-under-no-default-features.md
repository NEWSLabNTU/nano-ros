---
id: 731
title: "`main` is red at tier 1: `PlatformSink` references `nros_platform_log_write` under `--no-default-features`, where no platform port provides it"
status: resolved
type: bug
severity: high
area: build, logging
related: [issue-0723, issue-0727, issue-0708, issue-0710, issue-0619]
---

## Symptom

`just ci` fails at `check-workspace-features`, on the host AND in the ROS
distrobox, on a clean tree:

```
cargo test --no-run --workspace --exclude nros-c --no-default-features --quiet

rust-lld: error: undefined symbol: nros_platform_log_write
  >>> referenced by sinks.rs:72 (packages/core/nros-log/src/sinks.rs:72)
  >>>   …(<nros_log::sinks::PlatformSink as nros_log::LogSink>::log) in archive
  >>>   …/libnros_log-*.rlib
rust-lld: error: undefined symbol: nros_platform_log_flush
  >>> referenced by sinks.rs:62 …::flush
collect2: error: ld returned 1 exit status

error: could not compile `nros-tests` (test "workspace_features_e2e")
error: could not compile `nros-tests` (test "large_msg")
```

Different test targets are named on different runs — it is whichever the linker
reaches first, not a property of those tests.

## What it is

`PlatformSink` calls into the platform ABI (`nros_platform_log_write` /
`_flush`). Under `--no-default-features` across the workspace, nothing selects a
platform port, so the symbols have no definition — but `PlatformSink` is still
compiled into `nros-log`'s rlib and still referenced, so every test binary that
links `nros-log` fails at link time.

This is issue 0619's class exactly ("link a platform port into nros-cpp's lib
test, and gate nros-c's"), one crate over: a type whose body needs a port, in a
build that was told to select none.

## Not caused by the changes it was found under

Found while verifying issue 0707's domain-assignment fix. It is not that, and it
is not the PlatformIO removal (#704) either — neither touches `nros-log`,
feature wiring or platform linking. Reproduced on a HOST checkout as well as in
the box, so it is not environmental.

The neighbourhood points at today's sink work: **#0708** ("boards publish the
`nros_log` sink list at every funnel") and **#0710** ("dispatch installs the
default sink, so no one has to find the boot paths"), both landed 2026-08-20 and
both about which sink gets installed where. Recorded as a lead, NOT as an
attribution — issue 0672's correction is the standing warning about confident
attribution to a commit window.

## Why it matters more than a lane failing

`check-workspace-features` is in `just ci`, so this is tier 1: the per-change
gate everyone runs. And the failing command is the one that asks the question
the fix has to answer — "does the workspace still build when the consumer picks
NO features" — which is the `std`-deletion contract ARCHITECTURE §2 and
phase-359 are enforcing. A build with no port is a legitimate configuration
here, not a misuse.

## Fix directions

1. **Gate `PlatformSink` on having a port.** A `cfg` or feature so the type is
   not compiled when no platform is selected. Matches how the rest of the tree
   treats port-dependent code, and keeps `--no-default-features` meaning what it
   says.
2. **Give the test targets a port.** A dev-dependency platform for `nros-tests`,
   the way #0619 linked one into `nros-cpp`'s lib test. Narrower, but it makes
   the LANE pass without making the CONFIGURATION valid, so a downstream
   consumer building with no features hits the same wall.

Direction 1 is the one that answers the question the lane is asking. Whoever
owns #708/#710 should confirm the intended default-sink shape first: if
`PlatformSink` is meant to be the default everywhere, the answer may be that it
needs a weak/fallback definition rather than a gate.

## Closed 2026-08-20 — DUPLICATE of #0723 / #0727, already fixed

Filed at 2026-08-20 while two other sessions were fixing it. Issue 0727's title
is this issue's subject verbatim: "`PlatformSink`'s extern pair breaks the
workspace no-default-features test-compile — the sink is link-time platform code
riding a library edge."

Verified rather than assumed: `just check workspace-features` now ends
`All feature checks passed!` on a clean host checkout, and the exact command
this issue quoted —
`cargo test --no-run --workspace --exclude nros-c --no-default-features` —
links.

### The fix, and why it is better than what this issue proposed

This issue offered two directions and argued for gating `PlatformSink` on having
a port. What landed (`3aff18c8c`, `46c0b8a16`, `9f2529934`) is that direction
taken further, and the reasoning is worth carrying:

**A dependency IS a property of the binary; a feature is a property of the
build.** `PlatformSink` moved to `nros_platform_cffi::log`, the crate that owns
the ABI, so "does this binary need `nros_platform_log_write`?" is answered by
what it depends on. A Cargo feature could not answer it at all — feature
unification means `nros-platform-cffi` and `nros-rmw-bridge` enable
`nros-node/rmw-cffi` unconditionally, so any forwarded gate is ON for every
workspace member (that is issue 0723). Direction 1 as written here would have
been defeated by exactly that.

It also puts the `extern` in one place: `nros-platform-cffi`'s bindgen output
from `<nros/platform.h>`, the SSoT RFC-0054 names, instead of a second
hand-written declaration in the facade.

A third session had landed weak host stubs plus a `platform-sink` feature and a
gate enforcing it; that was removed in favour of the move, on a project rule
about avoiding weak symbols. Its sharper observation was kept: the hazard is not
just that the sink is reachable but that whether the unreferenced vtable is
GC'd before the link is CODEGEN LUCK — which is why the lane failed
intermittently-looking rather than always.

### Worth keeping from this issue

Two observations the duplicates did not record:

* **Which test target is named varies with link order.** Runs blamed
  `large_msg` and `workspace_features_e2e` on different attempts. That is why it
  reads as an unrelated test breaking, and it is the first thing to recognise if
  the class returns.
* **It reproduces on a host checkout, not only in the container** — worth
  stating because the run that surfaced it was in the ROS distrobox, and
  "environmental" is the cheap wrong answer.

