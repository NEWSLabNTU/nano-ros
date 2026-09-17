---
id: 1380
title: "`nros-rmw-zenoh` fails `clippy -D warnings` under `platform-bare-metal`
  and has for some time — no lane runs clippy on that feature combination"
status: open
type: bug
area: [ci, build]
severity: medium
found: 2026-09-17
related: [issue-1040, issue-1226, issue-0196]
---

## What happens

```
cargo clippy -p nros-rmw-zenoh --no-default-features \
  --features "platform-bare-metal,ros-humble" \
  --lib --target thumbv7m-none-eabi -- -D warnings
```

```
error: unneeded `return` statement
    --> packages/rmw/zenoh/nros-rmw-zenoh/src/shim/subscriber.rs:1220:13
error: could not compile `nros-rmw-zenoh` (lib) due to 1 previous error
```

The site is the bare-metal arm of `check_liveliness_and_fire`:

```rust
#[cfg(feature = "platform-bare-metal")]
{
    return;
}
```

The `return` is the LAST statement of the function under that cfg, so clippy's
`needless_return` fires. Under every other platform feature the arm is
compiled out and the lint never sees it.

## Why nothing caught it

`check-workspace-all` sweeps feature combinations, and `check` runs the
embedded clippy lanes, but neither reaches `-p nros-rmw-zenoh
--features platform-bare-metal` on a `thumbv7m-none-eabi` target. This is the
0196 shape again — a gate whose REACH is narrower than the rule it enforces —
and the 2026-07-28 audit's finding restated on a new lane: "clippy is clean" is
a claim about the feature combinations somebody enumerated, not about the
crate.

Found while adding the phase-392 amendment B instrument, whose own
`platform-bare-metal` clippy run surfaced this one beside it. Confirmed
PRE-EXISTING by running the same command with the new feature absent.

## Direction

Two separate fixes, and the second is the one that matters:

1. The lint itself — `{}` instead of `{ return; }`, or hoist the cfg to the
   function. One line.
2. **The reach.** A backend that ships on five platform features should have
   all five in a clippy lane, or the four that are not there will drift the
   same way. Whether that belongs in `check-workspace-all` (which already
   enumerates) or in the embedded clippy lane is the design question; adding
   only combination five by hand repeats the defect.
